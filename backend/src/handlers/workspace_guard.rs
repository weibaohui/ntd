//! workspace 归属校验 helper（v1 路由的租户边界守卫）。
//!
//! v1 路由把资源嵌套在 `/api/v1/workspaces/{ws}/` 下，URL 里的 `{ws}` 是租户边界
//! （K8s namespace 语义）。但 axum 的 `.nest()` 只负责把 `{ws}` 暴露给 handler，
//! 既不强制 handler 提取它，也不校验「路径里的 ws」与「资源实际所属 ws」是否一致。
//! 若 handler 不主动校验，就会出现跨工作空间越权读写
//! （例如 `GET /api/v1/workspaces/999/loops/1` 读到别的 ws 的 loop）。
//!
//! 本模块把「查资源 → 比对 workspace_id → 映射错误」收敛为三个 `verify_*` 函数，
//! 所有 workspace-scoped handler 统一调用，避免校验逻辑散落各处。
//!
//! 归属判断本身（`workspace_id_matches`）抽成无 db 依赖的纯函数，便于单元测试
//! 覆盖全部分支（相等 / 跨 ws / None）；`verify_*` 是薄包装：查 db + 调纯函数 +
//! 把结果映射成 `AppError`。
//!
//! 跨 ws 失败统一返回 `BadRequest(400)`，沿用既有「trigger/step 不属于该 loop」
//! 的惯例（见 `loop_.rs`），不新增 Forbidden 错误类型。

use crate::db::Database;
use crate::handlers::AppError;

/// 纯判断：资源的 `workspace_id` 是否等于给定 ws。
///
/// 抽成纯函数便于单测（无需 in-memory db）。`None`（旧数据 / 未归属）一律视为
/// 不匹配 —— 跨 ws 访问必须被拒绝，避免「未归属的旧数据可被任意 ws 访问」。
fn workspace_id_matches(model_ws: Option<i64>, ws_id: i64) -> bool {
    model_ws == Some(ws_id)
}

/// 校验 loop 属于指定 workspace：查不到 → `NotFound`，归属不符 → `BadRequest(400)`。
///
/// `loops` 表自带 `workspace_id` 列，直接 `get_loop` 后比对即可。
pub async fn verify_loop_belongs_to_ws(
    db: &Database,
    loop_id: i64,
    ws_id: i64,
) -> Result<(), AppError> {
    // get_loop 返回 Option；None 表示该 loop 不存在（已删除或 id 非法），
    // 统一映射成 NotFound，避免向调用方泄露「loop 是否存在」的存在性细节。
    let model = db.get_loop(loop_id).await?.ok_or(AppError::NotFound)?;
    if workspace_id_matches(model.workspace_id, ws_id) {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "loop #{} 不属于工作空间 #{}",
            loop_id, ws_id
        )))
    }
}

/// 校验 todo 属于指定 workspace。todo 模型含 `workspace_id`，直接比对。
pub async fn verify_todo_belongs_to_ws(
    db: &Database,
    todo_id: i64,
    ws_id: i64,
) -> Result<(), AppError> {
    let todo = db.get_todo(todo_id).await?.ok_or(AppError::NotFound)?;
    if workspace_id_matches(todo.workspace_id, ws_id) {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "todo #{} 不属于工作空间 #{}",
            todo_id, ws_id
        )))
    }
}

/// 校验 execution record 属于指定 workspace。
///
/// `execution_records` 表没有 `workspace_id` 列，只能经其 `todo_id` 间接关联到
/// workspace —— 因此先查 record 拿 `todo_id`，再复用 `verify_todo_belongs_to_ws`，
/// 保证「record 属于 ws」与「record 的 todo 属于 ws」语义一致。
pub async fn verify_execution_belongs_to_ws(
    db: &Database,
    record_id: i64,
    ws_id: i64,
) -> Result<(), AppError> {
    let record = db
        .get_execution_record(record_id)
        .await?
        .ok_or(AppError::NotFound)?;
    // record.todo_id 是 i64（非 Option），直接交给 todo 校验；
    // record 不存在已在上一步映射为 NotFound。
    verify_todo_belongs_to_ws(db, record.todo_id, ws_id).await
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::useless_vec,
    clippy::redundant_pattern_matching,
    clippy::redundant_clone,
    clippy::len_zero,
    clippy::bool_assert_comparison,
    clippy::unnecessary_get_then_check,
    clippy::doc_lazy_continuation,
    clippy::clone_on_copy,
    clippy::print_stdout,
    clippy::needless_pass_by_value,
    clippy::sliced_string_as_bytes,
    clippy::manual_map,
    clippy::collapsible_match,
    clippy::question_mark
)]
mod tests {
    use super::*;
    use crate::db::execution::NewExecutionRecord;

    // ── 纯函数：归属判断的全部分支（无 db，核心正确性）──────────

    #[test]
    fn test_workspace_id_matches_when_equal() {
        // workspace_id 与传入 ws 相等 → 视为属于该 ws
        assert!(workspace_id_matches(Some(7), 7));
    }

    #[test]
    fn test_workspace_id_matches_when_different_ws() {
        // 资源属于 ws 7，却用 ws 8 访问 → 跨 ws，必须拒绝
        assert!(!workspace_id_matches(Some(7), 8));
    }

    #[test]
    fn test_workspace_id_matches_when_none() {
        // 旧数据 workspace_id=None（未归属任何 ws）：任意 ws 访问都拒绝，
        // 否则未归属数据会被全局可见，破坏隔离
        assert!(!workspace_id_matches(None, 7));
    }

    // ── 测试数据 helper（in-memory SQLite，参照 db/blackboard.rs 测试模式）──

    /// 建一个工作空间（project_directories），返回其 id。
    /// path 有唯一约束：同一 db 内建多个 ws 必须传不同 path。
    async fn create_workspace(db: &Database, path: &str) -> i64 {
        db.create_project_directory(path, None, false, false)
            .await
            .expect("create workspace must succeed")
    }

    /// 建一个 loop，workspace_id 可选（None 模拟旧数据未归属）。
    async fn create_loop(db: &Database, ws_id: Option<i64>) -> i64 {
        db.create_loop(
            "test-loop", "", ws_id, None, false, "", None, None, None, "",
        )
        .await
        .expect("create loop must succeed")
        .id
    }

    /// 建一个 todo，强制归属到 ws_id（create_todo_with_extras 要求 ws 必填）。
    async fn create_todo_in_ws(db: &Database, ws_id: i64) -> i64 {
        db.create_todo_with_extras("test-todo", "", None, None, false, ws_id, "/tmp")
            .await
            .expect("create todo must succeed")
    }

    /// 建一条 execution record，挂在指定 todo 下。
    async fn create_record_for_todo(db: &Database, todo_id: i64) -> i64 {
        db.create_execution_record(NewExecutionRecord {
            todo_id: Some(todo_id),
            command: "cmd",
            executor: "claudecode",
            trigger_type: "manual",
            task_id: "task-1",
            session_id: None,
            resume_message: None,
            source_todo_id: None,
            source_todo_title: None,
            loop_step_execution_id: None,
            step_id: None,
        })
        .await
        .expect("create execution record must succeed")
    }

    // ── verify_loop_belongs_to_ws ───────────────────────────────

    #[tokio::test]
    async fn test_verify_loop_belongs_to_ws_ok() {
        let db = Database::new(":memory:").await.expect("db must open");
        let ws = create_workspace(&db, "/tmp/ws-a").await;
        let loop_id = create_loop(&db, Some(ws)).await;
        // loop 属于该 ws → 放行
        assert!(verify_loop_belongs_to_ws(&db, loop_id, ws).await.is_ok());
    }

    #[tokio::test]
    async fn test_verify_loop_belongs_to_ws_cross_ws_rejected() {
        let db = Database::new(":memory:").await.expect("db must open");
        let ws_a = create_workspace(&db, "/tmp/ws-a").await;
        let ws_b = create_workspace(&db, "/tmp/ws-b").await;
        let loop_id = create_loop(&db, Some(ws_a)).await;
        // 用 ws_b 访问 ws_a 的 loop → 跨 ws，返回 BadRequest 并带「不属于工作空间」
        match verify_loop_belongs_to_ws(&db, loop_id, ws_b).await {
            Err(AppError::BadRequest(msg)) => assert!(msg.contains("不属于工作空间")),
            other => panic!("跨 ws 应返回 BadRequest，实际: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_verify_loop_belongs_to_ws_not_found() {
        let db = Database::new(":memory:").await.expect("db must open");
        let ws = create_workspace(&db, "/tmp/ws-a").await;
        // 不存在的 loop id → NotFound（AppError 未 derive PartialEq，用 matches!）
        assert!(matches!(
            verify_loop_belongs_to_ws(&db, 99999, ws).await.unwrap_err(),
            AppError::NotFound
        ));
    }

    // ── verify_todo_belongs_to_ws ───────────────────────────────

    #[tokio::test]
    async fn test_verify_todo_belongs_to_ws_ok() {
        let db = Database::new(":memory:").await.expect("db must open");
        let ws = create_workspace(&db, "/tmp/ws-a").await;
        let todo_id = create_todo_in_ws(&db, ws).await;
        assert!(verify_todo_belongs_to_ws(&db, todo_id, ws).await.is_ok());
    }

    #[tokio::test]
    async fn test_verify_todo_belongs_to_ws_cross_ws_rejected() {
        let db = Database::new(":memory:").await.expect("db must open");
        let ws_a = create_workspace(&db, "/tmp/ws-a").await;
        let ws_b = create_workspace(&db, "/tmp/ws-b").await;
        let todo_id = create_todo_in_ws(&db, ws_a).await;
        // 用 ws_b 访问 ws_a 的 todo → 跨 ws，返回 BadRequest
        assert!(matches!(
            verify_todo_belongs_to_ws(&db, todo_id, ws_b).await,
            Err(AppError::BadRequest(_))
        ));
    }

    // ── verify_execution_belongs_to_ws ──────────────────────────

    #[tokio::test]
    async fn test_verify_execution_belongs_to_ws_ok() {
        let db = Database::new(":memory:").await.expect("db must open");
        let ws = create_workspace(&db, "/tmp/ws-a").await;
        let todo_id = create_todo_in_ws(&db, ws).await;
        let record_id = create_record_for_todo(&db, todo_id).await;
        // record 的 todo 属于该 ws → record 也属于该 ws，放行
        assert!(verify_execution_belongs_to_ws(&db, record_id, ws).await.is_ok());
    }

    #[tokio::test]
    async fn test_verify_execution_belongs_to_ws_cross_ws_rejected() {
        let db = Database::new(":memory:").await.expect("db must open");
        let ws_a = create_workspace(&db, "/tmp/ws-a").await;
        let ws_b = create_workspace(&db, "/tmp/ws-b").await;
        let todo_id = create_todo_in_ws(&db, ws_a).await;
        let record_id = create_record_for_todo(&db, todo_id).await;
        // 用 ws_b 访问 ws_a 的 record → 间接跨 ws（record→todo→ws），返回 BadRequest
        assert!(matches!(
            verify_execution_belongs_to_ws(&db, record_id, ws_b).await,
            Err(AppError::BadRequest(_))
        ));
    }
}
