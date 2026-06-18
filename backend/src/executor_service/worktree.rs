//! Git Worktree 集成（issue #643）
//!
//! 这里只放 worktree 相关的细粒度辅助函数。每个函数 ≤ 30 行，职责单一。
//! 编排层（stages::start_todo_and_prepare_spawn）调用这里的 helper 完成：
//!   1. 根据 todo.workspace 决定是否开 worktree（`resolve_worktree_context`）
//!   2. 把 worktree 路径回写到 execution_records（`record_worktree_path`）
//!   3. 执行结束后清理 worktree（`cleanup_worktree_if_needed`）
//!   4. 给 claude_code / hermes 注入 `--worktree` 开关（`apply_worktree_flag`）
//!   5. 杀进程组（`kill_process_tree`，command-group 集成）

use crate::db::Database;
use crate::models::{ExecutorType, Todo};
use crate::services::worktree::WorktreeService;

/// issue #643: 单次执行使用的 worktree 上下文。
///
/// - `effective_workspace`: 子进程的 cwd。None=继续用 todo.workspace；
///   Some(p)=worktree 目录被 ntd 接管，子进程在 worktree 内运行。
/// - `record_path`: 回写到 execution_records.worktree_path 的值（None=无需记录）。
/// - `auto_cleanup`: 终态时是否需要调用 WorktreeService::cleanup_worktree。
#[derive(Debug, Clone, Default)]
pub struct WorktreeContext {
    pub effective_workspace: Option<String>,
    pub record_path: Option<String>,
    pub auto_cleanup: bool,
}

/// 根据 todo.workspace 找到对应的 project_directory，决定是否开 worktree。
///
/// 不在 `WorktreeContext` 内持有数据库句柄——这是个**纯异步查询**函数，方便在
/// run_todo_execution 主路径上独立调用并把结果 move 进 spawn 闭包。
pub async fn resolve_worktree_context(db: &Database, todo: &Option<Todo>) -> WorktreeContext {
    // 没有 todo（被 hook 删除）/ 没有 workspace 关联项目目录——不启用 worktree
    let Some(t) = todo.as_ref() else {
        return WorktreeContext::default();
    };
    let Some(ws) = t.workspace.as_deref() else {
        return WorktreeContext::default();
    };
    // 目录在 project_directories 表里没登记——同样不启用（避免给任意 workspace 路径做 worktree）
    let Ok(Some(dir)) = db.get_project_directory_by_path(ws).await else {
        return WorktreeContext::default();
    };
    if !dir.git_worktree_enabled {
        return WorktreeContext::default();
    }

    // 走到这里说明用户在该目录下开启了 worktree 自动管理。
    // 创建失败时不阻塞执行——回退到原始 workspace，子进程仍然能跑通。
    let svc = WorktreeService::new();
    match svc.create_worktree(ws, t.id) {
        Ok(wt_path) => WorktreeContext {
            effective_workspace: Some(wt_path.clone()),
            record_path: Some(wt_path),
            auto_cleanup: dir.auto_cleanup,
        },
        Err(e) => {
            tracing::warn!(
                workspace = %ws,
                todo_id = t.id,
                error = %e,
                "failed to create git worktree, falling back to original workspace"
            );
            WorktreeContext::default()
        }
    }
}

/// 把 worktree_path 持久化到 execution_records。
///
/// 这一步不在 `resolve_worktree_context` 内做，因为该函数不持有 record_id；
/// 调用方在拿到 `create_execution_record` 返回的 id 之后再回填。
pub async fn record_worktree_path(db: &Database, record_id: i64, path: Option<&str>) {
    if let Some(p) = path {
        if let Err(e) = db.update_execution_record_worktree_path(record_id, p).await {
            tracing::warn!(record_id, error = ?e, "failed to persist worktree_path");
        }
    }
}

/// 执行结束后清理 worktree（如果启用了 auto_cleanup）。
///
/// `WorktreeError` 不会出现：本服务把失败映射成 warn，不再向上抛。
pub fn cleanup_worktree_if_needed(ctx: &WorktreeContext) {
    if !ctx.auto_cleanup {
        return;
    }
    let Some(path) = ctx.record_path.as_deref() else {
        return;
    };
    let svc = WorktreeService::new();
    if let Err(e) = svc.cleanup_worktree(path) {
        tracing::warn!(worktree = %path, error = %e, "worktree cleanup failed");
    }
}

/// 给 `command_args` 插入 `--worktree` 开关。
///
/// - 仅当 `worktree_enabled == true` 且 executor 是 `Claudecode` 或 `Hermes` 时生效；
///   其他 executor 即使 todo 开启了 worktree，也会被静默忽略。
/// - 位置约束：必须放在 `--session-id` / `--resume` 之前，否则 Claude Code / Hermes
///   在 resume session 时不会触发 worktree 初始化。
///   没找到这些开关时 append 到末尾，依然能让 Claude Code 自动管理 worktree。
pub fn apply_worktree_flag(
    command_args: &mut Vec<String>,
    exec_type: ExecutorType,
    worktree_enabled: bool,
) {
    if !worktree_enabled {
        return;
    }
    match exec_type {
        ExecutorType::Claudecode | ExecutorType::Hermes => {
            // 找 `--session-id` 或 `--resume` 的位置；找不到就 append 到末尾。
            let insert_pos = command_args
                .iter()
                .position(|s| s == "--session-id" || s == "--resume")
                .unwrap_or(command_args.len());
            command_args.insert(insert_pos, "--worktree".to_string());
        }
        // 其他 executor 不支持 worktree flag，todo 配置的 `worktree_enabled`
        // 对它们而言无意义；显式忽略避免误把 flag 透传给不识别的二进制。
        _ => {}
    }
}

/// 使用 command-group 安全地杀死进程树
/// command-group 会自动创建进程组，kill() 时会杀死整个进程组
pub async fn kill_process_tree(child: &mut command_group::AsyncGroupChild) {
    if let Err(e) = child.kill().await {
        tracing::warn!("Failed to kill process group: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_worktree_flag_inserts_before_session_id() {
        // Claude Code：插入到 --session-id 之前。
        let mut args = vec!["--print".to_string(), "--session-id".to_string(), "abc".to_string()];
        apply_worktree_flag(&mut args, ExecutorType::Claudecode, true);
        assert_eq!(args, vec!["--print", "--worktree", "--session-id", "abc"]);

        // Hermes：插入到 --resume 之前。
        let mut args = vec!["-p".to_string(), "--resume".to_string(), "xyz".to_string()];
        apply_worktree_flag(&mut args, ExecutorType::Hermes, true);
        assert_eq!(args, vec!["-p", "--worktree", "--resume", "xyz"]);

        // 找不到 --session-id / --resume 时 append 到末尾。
        let mut args = vec!["-p".to_string()];
        apply_worktree_flag(&mut args, ExecutorType::Claudecode, true);
        assert_eq!(args, vec!["-p", "--worktree"]);

        // worktree_enabled = false 时不插入。
        let mut args = vec!["--print".to_string()];
        apply_worktree_flag(&mut args, ExecutorType::Claudecode, false);
        assert_eq!(args, vec!["--print"]);

        // 其他 executor 即使 worktree_enabled = true 也不插入。
        let mut args = vec!["--print".to_string()];
        apply_worktree_flag(&mut args, ExecutorType::Codex, true);
        assert_eq!(args, vec!["--print"]);
        apply_worktree_flag(&mut args, ExecutorType::Pi, true);
        assert_eq!(args, vec!["--print"]);
    }

    #[test]
    fn test_cleanup_worktree_if_needed_disabled() {
        let ctx = WorktreeContext {
            effective_workspace: None,
            record_path: Some("/tmp/ntd-643-disabled".into()),
            auto_cleanup: false,
        };
        // 不应 panic，不应触发任何 git 调用
        cleanup_worktree_if_needed(&ctx);
    }

    #[test]
    fn test_cleanup_worktree_if_needed_no_path() {
        let ctx = WorktreeContext {
            effective_workspace: None,
            record_path: None,
            auto_cleanup: true,
        };
        cleanup_worktree_if_needed(&ctx);
    }

    #[test]
    fn test_worktree_context_default_is_disabled() {
        let ctx = WorktreeContext::default();
        assert!(ctx.effective_workspace.is_none());
        assert!(ctx.record_path.is_none());
        assert!(!ctx.auto_cleanup);
    }
}