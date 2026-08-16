use std::collections::HashMap;
use std::sync::Arc;

use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::db::entity::workspaces;
use crate::db::Database;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: i64,
    pub path: String,
    pub name: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    /// issue #643: 是否在该目录下执行 Todo 时由 ntd 自动创建 git worktree。
    /// false（默认）= 行为与之前一致，由 Claude Code / Hermes 自己管理 worktree。
    #[serde(default)]
    pub git_worktree_enabled: bool,
    /// issue #643: 执行结束后（成功/失败/取消）是否自动清理 worktree。
    /// 仅在 `git_worktree_enabled = true` 时才有意义。
    #[serde(default)]
    pub auto_cleanup: bool,
}

impl Database {
    pub async fn get_workspaces(&self) -> Result<Vec<Workspace>, sea_orm::DbErr> {
        let models = workspaces::Entity::find()
            .order_by_asc(workspaces::Column::Path)
            .all(&self.conn)
            .await?;

        Ok(models
            .into_iter()
            .map(|m| Workspace {
                id: m.id,
                path: m.path,
                name: m.name,
                created_at: m.created_at,
                updated_at: m.updated_at,
                git_worktree_enabled: m.git_worktree_enabled,
                auto_cleanup: m.auto_cleanup,
            })
            .collect())
    }

    pub async fn create_workspace(
        &self,
        path: &str,
        name: Option<&str>,
        git_worktree_enabled: bool,
        auto_cleanup: bool,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = workspaces::ActiveModel {
            path: ActiveValue::Set(path.to_string()),
            name: ActiveValue::Set(name.map(|s| s.to_string())),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            // 新目录默认两个 worktree 开关都是关；调用方在 update 时再决定要不要打开。
            // 不在 create 接口暴露这两个字段是因为新增目录的意图是"登记项目"，具体执行策略
            // 属于后续编辑的场景，避免在新增弹窗里强加选择负担。
            git_worktree_enabled: ActiveValue::Set(git_worktree_enabled),
            auto_cleanup: ActiveValue::Set(auto_cleanup),
            ..Default::default()
        };
        let inserted = am.insert(&self.conn).await?;
        Ok(inserted.id)
    }

    /// 更新工作空间字段。
    /// - `name=None` 表示"不修改名称"（与 `get_or_create` 的语义保持一致），
    ///   实现用 `ActiveValue::Unchanged` 跳过 name 列；handler 层负责把空字符串 trim 拒绝。
    /// - `name=Some(s)` 直接覆盖当前名称。
    /// - `git_worktree_enabled` / `auto_cleanup` 是 issue #643 新增的可选修改项；
    ///   传入 None 时跳过对应列，传入 Some(bool) 时写入新值。
    pub async fn update_workspace(
        &self,
        id: i64,
        name: Option<&str>,
        git_worktree_enabled: Option<bool>,
        auto_cleanup: Option<bool>,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        // 用 match 把 Option<&str> 直接落到三种语义：None=Unchanged, Some("")=仍 Unchanged
        // （handler 已拒绝空串，这里再做一次兜底），Some(non-empty)=Set。避免出现「Set(None) 把列写成 NULL」的反直觉行为。
        let mut am = workspaces::ActiveModel {
            id: ActiveValue::Unchanged(id),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        match name {
            Some(s) if !s.is_empty() => {
                am.name = ActiveValue::Set(Some(s.to_string()));
            }
            _ => {
                am.name = ActiveValue::Unchanged(Default::default());
            }
        }
        // ActiveValue::Set 写 NULL 不安全（NOT NULL 列），所以用 None 显式表达"跳过"
        if let Some(flag) = git_worktree_enabled {
            am.git_worktree_enabled = ActiveValue::Set(flag);
        }
        if let Some(flag) = auto_cleanup {
            am.auto_cleanup = ActiveValue::Set(flag);
        }
        self.exec_update(am).await
    }

    pub async fn delete_workspace(&self, id: i64) -> Result<(), sea_orm::DbErr> {
        workspaces::Entity::delete_by_id(id)
            .exec(&self.conn)
            .await
            .map(|_| ())
    }

    pub async fn get_workspace_by_path(
        &self,
        path: &str,
    ) -> Result<Option<Workspace>, sea_orm::DbErr> {
        let model = workspaces::Entity::find()
            .filter(workspaces::Column::Path.eq(path))
            .one(&self.conn)
            .await?;

        Ok(model.map(|m| Workspace {
            id: m.id,
            path: m.path,
            name: m.name,
            created_at: m.created_at,
            updated_at: m.updated_at,
            git_worktree_enabled: m.git_worktree_enabled,
            auto_cleanup: m.auto_cleanup,
        }))
    }

    pub async fn get_workspace_by_id(
        &self,
        id: i64,
    ) -> Result<Option<Workspace>, sea_orm::DbErr> {
        let model = workspaces::Entity::find_by_id(id)
            .one(&self.conn)
            .await?;

        Ok(model.map(|m| Workspace {
            id: m.id,
            path: m.path,
            name: m.name,
            created_at: m.created_at,
            updated_at: m.updated_at,
            git_worktree_enabled: m.git_worktree_enabled,
            auto_cleanup: m.auto_cleanup,
        }))
    }

    /// 如果目录不存在则创建，返回目录信息
    /// 处理并发竞态：捕获唯一约束冲突并重试查询
    /// 当 `name` 不为 None 时，若目标记录已存在且名称不同，会同步把名称更新为新值，
    /// 避免前端补全名称时留下"无名"历史记录
    ///
    /// issue #643 备注：worktree 开关属于"执行策略"，本接口不修改它们——`get_or_create`
    /// 主要被 Todo 创建路径调用，新目录登记时不应自动开启 worktree。
    pub async fn get_or_create_workspace(
        &self,
        path: &str,
        name: Option<&str>,
    ) -> Result<Workspace, sea_orm::DbErr> {
        if let Some(existing) = self.get_workspace_by_path(path).await? {
            // name=None 时是 no-op：不应被解读为"清空名称"，仅保持现有值不变。
            // name=Some 且与现有名称不同时才触发更新，兼容"先有路径、后补名称"的使用路径。
            if let Some(new_name) = name {
                if existing.name.as_deref() != Some(new_name) {
                    self.update_workspace(existing.id, Some(new_name), None, None)
                        .await?;
                    return self
                        .get_workspace_by_id(existing.id)
                        .await?
                        .ok_or_else(|| {
                            sea_orm::DbErr::Custom("Directory disappeared after rename".into())
                        });
                }
            }
            return Ok(existing);
        }

        match self.create_workspace(path, name, false, false).await {
            Ok(id) => {
                // 创建成功后从数据库查询以获取准确的时间戳
                self.get_workspace_by_id(id)
                    .await?
                    .ok_or_else(|| sea_orm::DbErr::Custom("Failed to retrieve created directory".into()))
            }
            Err(e) => {
                // 如果是唯一约束冲突，说明另一个请求已经创建了该目录，重试查询
                if is_unique_constraint_error(&e) {
                    let existing = self
                        .get_workspace_by_path(path)
                        .await?
                        .ok_or_else(|| sea_orm::DbErr::Custom("Directory disappeared after conflict".into()))?;
                    if let Some(new_name) = name {
                        if existing.name.as_deref() != Some(new_name) {
                            self.update_workspace(existing.id, Some(new_name), None, None)
                                .await?;
                            return self
                                .get_workspace_by_id(existing.id)
                                .await?
                                .ok_or_else(|| {
                                    sea_orm::DbErr::Custom("Directory disappeared after rename".into())
                                });
                        }
                    }
                    Ok(existing)
                } else {
                    Err(e)
                }
            }
        }
    }
}

/// per-workspace 执行器 session 操作的互斥锁池，避免并发读写 session 导致数据不一致。
static EXECUTOR_SESSION_LOCKS: std::sync::LazyLock<std::sync::Mutex<HashMap<i64, Arc<Mutex<()>>>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// 取得指定 workspace 的执行器 session 互斥锁句柄（不存在则创建）。
///
/// 返回 Arc<Mutex> 而非直接守卫，是因为调用方需要在 await 之前获取守卫、
/// 在 await 之后释放，Arc 让守卫可以跨 await 点持有。
fn executor_session_lock(workspace_id: i64) -> Arc<Mutex<()>> {
    let outer = &*EXECUTOR_SESSION_LOCKS;
    // Mutex poisoning 只在持有者 panic 时发生；这里锁的是空 HashMap，不会 panic
    let mut guard = outer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard
        .entry(workspace_id)
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

/// session 存储的 chat 维度（110 修订）：单聊与群聊各自独立的会话上下文。
///
/// 背景：session 键原本只有 (workspace, executor)，单聊与群聊互相 resume 对方的
/// 会话——110 把单聊（纯直聊）/群聊（管家+专家人设）语义分家后，两种上下文
/// 互串会造成专家人设污染单聊裸会话（或反之），故按 chat scope 分键存储。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutorSessionScope {
    /// 单聊（dm_chat）：用户与执行器的一对一对话
    Dm,
    /// 群聊（butler_chat）：群聊管家对话
    Group,
}

impl ExecutorSessionScope {
    /// 从消息 chat_type 解析：p2p→Dm，其余（含未知类型兜底）→Group——
    /// 与 FeishuListener::chat_trigger_type 的分流口径保持一致，
    /// 避免 session 键与 trigger_type 落在两个不同的维度上。
    pub fn from_chat_type(chat_type: &str) -> Self {
        if chat_type == "p2p" { Self::Dm } else { Self::Group }
    }

    /// JSON map 的键前缀：`dm`/`group`，拼上执行器名成复合键（如 `dm:pi`）。
    /// 选冒号分隔是因为执行器名不含冒号（ExecutorType::as_str 是 ASCII 标识符），
    /// 不存在歧义碰撞。
    fn key_prefix(self) -> &'static str {
        match self {
            Self::Dm => "dm",
            Self::Group => "group",
        }
    }

    /// 拼 JSON 复合键
    fn scoped_key(self, executor: &str) -> String {
        format!("{}:{}", self.key_prefix(), executor)
    }
}

impl Database {
    /// 获取指定工作空间指定执行器在指定 chat 维度的会话 session_id。
    ///
    /// 返回值：
    /// - `Ok(None)`：工作空间不存在
    /// - `Ok(Some(None))`：工作空间存在但该维度执行器没有 session
    /// - `Ok(Some(Some(sid)))`：有 session
    ///
    /// 兼容：110 前的存量 JSON 键是裸执行器名（无 scope 前缀）。scoped 键缺失时
    /// 回退读裸键——升级后单聊继续 resume 原会话（110 前会话主体是单聊直聊），
    /// 群聊首次会开新会话，无需数据迁移。
    ///
    /// 并发安全：持 per-workspace 互斥锁，与 set_executor_session 互斥，
    /// 防止并发请求读取到过期的 session_id。
    pub async fn get_executor_session(
        &self,
        workspace_id: i64,
        executor: &str,
        scope: ExecutorSessionScope,
    ) -> Result<Option<Option<String>>, sea_orm::DbErr> {
        let lock = executor_session_lock(workspace_id);
        let _guard = lock.lock().await;

        let dir = workspaces::Entity::find_by_id(workspace_id)
            .one(&self.conn)
            .await?;

        let sessions_json = match dir {
            Some(d) => d.executor_sessions,
            None => return Ok(None),
        };

        // 解析 JSON 获取对应维度的 session
        let sessions: HashMap<String, Option<String>> =
            serde_json::from_str(sessions_json.as_deref().unwrap_or("{}"))
            .unwrap_or_default();

        // scoped 键优先；缺失时回退裸键（110 前存量，见函数注释兼容说明）
        Ok(sessions
            .get(&scope.scoped_key(executor))
            .or_else(|| sessions.get(executor))
            .cloned())
    }

    /// 更新指定工作空间指定执行器在指定 chat 维度的会话 session_id。
    ///
    /// 流程：
    /// 1. 读取现有 sessions JSON
    /// 2. 清同执行器裸键（110 前存量）+ 写 scoped 复合键
    /// 3. 写回数据库
    ///
    /// 顺手清裸键：首次写入即完成该执行器的键迁移，避免裸键残留导致
    /// 后续读取回退到旧会话。remove 忽略不存在的情况（首次写入无裸键可清）。
    ///
    /// 并发安全：持 per-workspace 互斥锁，与 get_executor_session 互斥，
    /// 防止并发请求的 session_id 互相覆盖。
    pub async fn set_executor_session(
        &self,
        workspace_id: i64,
        executor: &str,
        scope: ExecutorSessionScope,
        session_id: Option<String>,
    ) -> Result<(), sea_orm::DbErr> {
        // 持 per-workspace 互斥锁串行化「读-改-写」，与 get_executor_session 互斥。
        // 避免并发请求读取到过期的 session_id，或多个请求的 session_id 互相覆盖。
        let lock = executor_session_lock(workspace_id);
        let _guard = lock.lock().await;

        // 读取现有记录
        let dir = workspaces::Entity::find_by_id(workspace_id)
            .one(&self.conn)
            .await?
            .ok_or_else(|| sea_orm::DbErr::RecordNotFound("workspace not found".into()))?;

        // 解析现有 JSON
        let mut sessions: HashMap<String, Option<String>> =
            serde_json::from_str(dir.executor_sessions.as_deref().unwrap_or("{}"))
            .unwrap_or_default();

        // 键迁移：清裸键 + 写 scoped 键（理由见函数注释）
        sessions.remove(executor);
        sessions.insert(scope.scoped_key(executor), session_id);

        // 序列化并写回
        let now = crate::models::utc_timestamp();
        let am = workspaces::ActiveModel {
            id: ActiveValue::Unchanged(dir.id),
            executor_sessions: ActiveValue::Set(Some(serde_json::to_string(&sessions).unwrap_or_default())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        am.update(&self.conn).await?;
        Ok(())
    }
}

fn is_unique_constraint_error(err: &sea_orm::DbErr) -> bool {
    let err_str = format!("{:?}", err);
    err_str.contains("UNIQUE constraint failed")
}

#[cfg(test)]
// session 读写走真实 SQLite（:memory:），断言 JSON 键迁移与维度隔离的真实行为；
// 测试断言用 expect 直接失败即可，与项目内其他 DB 测试的豁免口径一致。
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod executor_session_tests {
    //! 验证 110 修订的 session chat 维度隔离：
    //! 单聊（Dm）与群聊（Group）各自独立读写互不串扰；旧裸键回退兼容；写时键迁移。

    use super::ExecutorSessionScope;
    use crate::db::Database;

    /// 建 workspace 并返回 id（每个用例独立库独立行，避免相互影响）
    async fn new_workspace(db: &Database) -> i64 {
        db.create_workspace("/tmp/ws-test", Some("ws"), false, false)
            .await
            .expect("create workspace must succeed")
    }

    /// 核心行为：单聊与群聊 session 互相隔离——写群聊不影响单聊读，反之亦然。
    /// 这是 110 修复的目标（原实现单聊群聊共用一键，专家人设上下文互串）。
    #[tokio::test]
    async fn test_executor_session_dm_and_group_are_isolated() {
        let db = Database::new(":memory:").await.unwrap();
        let wid = new_workspace(&db).await;

        // 单聊写入 session-a
        db.set_executor_session(wid, "pi", ExecutorSessionScope::Dm, Some("ses-dm".to_string()))
            .await
            .unwrap();
        // 群聊写入 session-b（同执行器！）
        db.set_executor_session(wid, "pi", ExecutorSessionScope::Group, Some("ses-group".to_string()))
            .await
            .unwrap();

        // 各读各的：互不串扰
        assert_eq!(
            db.get_executor_session(wid, "pi", ExecutorSessionScope::Dm).await.unwrap(),
            Some(Some("ses-dm".to_string())),
            "单聊维度应读到单聊 session，不被群聊写入覆盖"
        );
        assert_eq!(
            db.get_executor_session(wid, "pi", ExecutorSessionScope::Group).await.unwrap(),
            Some(Some("ses-group".to_string())),
            "群聊维度应读到群聊 session，不被单聊写入覆盖"
        );
    }

    /// 清空只影响本维度：群聊 /new 不误伤单聊会话（用户在群里点新会话，私聊上下文保留）。
    #[tokio::test]
    async fn test_executor_session_clear_is_scoped() {
        let db = Database::new(":memory:").await.unwrap();
        let wid = new_workspace(&db).await;
        db.set_executor_session(wid, "pi", ExecutorSessionScope::Dm, Some("ses-dm".to_string()))
            .await
            .unwrap();
        db.set_executor_session(wid, "pi", ExecutorSessionScope::Group, Some("ses-group".to_string()))
            .await
            .unwrap();

        // 清群聊维度
        db.set_executor_session(wid, "pi", ExecutorSessionScope::Group, None)
            .await
            .unwrap();

        assert_eq!(
            db.get_executor_session(wid, "pi", ExecutorSessionScope::Group).await.unwrap(),
            Some(None),
            "群聊维度应已清空"
        );
        assert_eq!(
            db.get_executor_session(wid, "pi", ExecutorSessionScope::Dm).await.unwrap(),
            Some(Some("ses-dm".to_string())),
            "单聊维度不受群聊清空影响"
        );
    }

    /// 兼容：110 前存量 JSON 键是裸执行器名。scoped 键缺失时回退读裸键——
    /// 升级后单聊（110 前会话的主体）继续 resume 原会话，不断档。
    /// 直接用 SQL 铺裸键数据，模拟旧版本写入的存量。
    #[tokio::test]
    async fn test_executor_session_falls_back_to_legacy_bare_key() {
        let db = Database::new(":memory:").await.unwrap();
        let wid = new_workspace(&db).await;
        // 模拟 110 前 set 写下的裸键 JSON
        db.exec(&format!(
            "UPDATE workspaces SET executor_sessions = '{{\"pi\": \"ses-legacy\"}}' WHERE id = {wid}"
        ))
        .await
        .unwrap();

        // scoped 键缺失 → 回退裸键（两个维度都回退：读侧无法判断旧会话属于哪边，
        // 单聊是旧会话主体，群聊首次读也拿到它只是开一次带上下文的会话，无害）
        assert_eq!(
            db.get_executor_session(wid, "pi", ExecutorSessionScope::Dm).await.unwrap(),
            Some(Some("ses-legacy".to_string())),
            "scoped 键缺失时应回退读裸键（110 前存量）"
        );
    }

    /// 键迁移：首次 scoped 写入顺手清掉裸键——避免后续读取永远回退到旧会话。
    #[tokio::test]
    async fn test_executor_session_write_migrates_legacy_bare_key() {
        let db = Database::new(":memory:").await.unwrap();
        let wid = new_workspace(&db).await;
        db.exec(&format!(
            "UPDATE workspaces SET executor_sessions = '{{\"pi\": \"ses-legacy\"}}' WHERE id = {wid}"
        ))
        .await
        .unwrap();

        // scoped 写入新 session
        db.set_executor_session(wid, "pi", ExecutorSessionScope::Dm, Some("ses-new".to_string()))
            .await
            .unwrap();

        // 裸键已被清：群聊维度读不到 legacy。返回 None（而非 Some(None)）——
        // group:pi 键从未写入、裸键已删，两个 get 都落空；None 语义即「无 session」，
        // 调用方（resolve_executor_session）对 None 与 Some(None) 同样降级为新会话
        assert_eq!(
            db.get_executor_session(wid, "pi", ExecutorSessionScope::Group).await.unwrap(),
            None,
            "scoped 写入后裸键应被迁移清除，群聊维度不应回退到旧会话"
        );
    }

    /// from_chat_type 与 chat_trigger_type 分流口径一致：p2p→Dm，其余→Group。
    #[test]
    fn test_executor_session_scope_from_chat_type() {
        assert_eq!(ExecutorSessionScope::from_chat_type("p2p"), ExecutorSessionScope::Dm);
        assert_eq!(ExecutorSessionScope::from_chat_type("group"), ExecutorSessionScope::Group);
        // 未知类型兜底群聊，与 listener 的 chat_trigger_type 同口径
        assert_eq!(ExecutorSessionScope::from_chat_type("unknown"), ExecutorSessionScope::Group);
    }
}
