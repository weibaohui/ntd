use std::collections::HashMap;
use std::sync::Arc;

use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::db::entity::project_directories;
use crate::db::Database;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDirectory {
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
    pub async fn get_project_directories(&self) -> Result<Vec<ProjectDirectory>, sea_orm::DbErr> {
        let models = project_directories::Entity::find()
            .order_by_asc(project_directories::Column::Path)
            .all(&self.conn)
            .await?;

        Ok(models
            .into_iter()
            .map(|m| ProjectDirectory {
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

    pub async fn create_project_directory(
        &self,
        path: &str,
        name: Option<&str>,
        git_worktree_enabled: bool,
        auto_cleanup: bool,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = project_directories::ActiveModel {
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

    /// 更新项目目录字段。
    /// - `name=None` 表示"不修改名称"（与 `get_or_create` 的语义保持一致），
    ///   实现用 `ActiveValue::Unchanged` 跳过 name 列；handler 层负责把空字符串 trim 拒绝。
    /// - `name=Some(s)` 直接覆盖当前名称。
    /// - `git_worktree_enabled` / `auto_cleanup` 是 issue #643 新增的可选修改项；
    ///   传入 None 时跳过对应列，传入 Some(bool) 时写入新值。
    pub async fn update_project_directory(
        &self,
        id: i64,
        name: Option<&str>,
        git_worktree_enabled: Option<bool>,
        auto_cleanup: Option<bool>,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        // 用 match 把 Option<&str> 直接落到三种语义：None=Unchanged, Some("")=仍 Unchanged
        // （handler 已拒绝空串，这里再做一次兜底），Some(non-empty)=Set。避免出现「Set(None) 把列写成 NULL」的反直觉行为。
        let mut am = project_directories::ActiveModel {
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

    pub async fn delete_project_directory(&self, id: i64) -> Result<(), sea_orm::DbErr> {
        project_directories::Entity::delete_by_id(id)
            .exec(&self.conn)
            .await
            .map(|_| ())
    }

    pub async fn get_project_directory_by_path(
        &self,
        path: &str,
    ) -> Result<Option<ProjectDirectory>, sea_orm::DbErr> {
        let model = project_directories::Entity::find()
            .filter(project_directories::Column::Path.eq(path))
            .one(&self.conn)
            .await?;

        Ok(model.map(|m| ProjectDirectory {
            id: m.id,
            path: m.path,
            name: m.name,
            created_at: m.created_at,
            updated_at: m.updated_at,
            git_worktree_enabled: m.git_worktree_enabled,
            auto_cleanup: m.auto_cleanup,
        }))
    }

    pub async fn get_project_directory_by_id(
        &self,
        id: i64,
    ) -> Result<Option<ProjectDirectory>, sea_orm::DbErr> {
        let model = project_directories::Entity::find_by_id(id)
            .one(&self.conn)
            .await?;

        Ok(model.map(|m| ProjectDirectory {
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
    pub async fn get_or_create_project_directory(
        &self,
        path: &str,
        name: Option<&str>,
    ) -> Result<ProjectDirectory, sea_orm::DbErr> {
        if let Some(existing) = self.get_project_directory_by_path(path).await? {
            // name=None 时是 no-op：不应被解读为"清空名称"，仅保持现有值不变。
            // name=Some 且与现有名称不同时才触发更新，兼容"先有路径、后补名称"的使用路径。
            if let Some(new_name) = name {
                if existing.name.as_deref() != Some(new_name) {
                    self.update_project_directory(existing.id, Some(new_name), None, None)
                        .await?;
                    return self
                        .get_project_directory_by_id(existing.id)
                        .await?
                        .ok_or_else(|| {
                            sea_orm::DbErr::Custom("Directory disappeared after rename".into())
                        });
                }
            }
            return Ok(existing);
        }

        match self.create_project_directory(path, name, false, false).await {
            Ok(id) => {
                // 创建成功后从数据库查询以获取准确的时间戳
                self.get_project_directory_by_id(id)
                    .await?
                    .ok_or_else(|| sea_orm::DbErr::Custom("Failed to retrieve created directory".into()))
            }
            Err(e) => {
                // 如果是唯一约束冲突，说明另一个请求已经创建了该目录，重试查询
                if is_unique_constraint_error(&e) {
                    let existing = self
                        .get_project_directory_by_path(path)
                        .await?
                        .ok_or_else(|| sea_orm::DbErr::Custom("Directory disappeared after conflict".into()))?;
                    if let Some(new_name) = name {
                        if existing.name.as_deref() != Some(new_name) {
                            self.update_project_directory(existing.id, Some(new_name), None, None)
                                .await?;
                            return self
                                .get_project_directory_by_id(existing.id)
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

impl Database {
    /// 获取指定工作空间指定执行器的私聊会话 session_id。
    ///
    /// 返回值：
    /// - `Ok(None)`：工作空间不存在
    /// - `Ok(Some(None))`：工作空间存在但该执行器没有 session
    /// - `Ok(Some(Some(sid)))`：工作空间存在且该执行器有 session
    ///
    /// 并发安全：持 per-workspace 互斥锁，与 set_executor_session 互斥，
    /// 防止并发请求读取到过期的 session_id。
    pub async fn get_executor_session(
        &self,
        workspace_id: i64,
        executor: &str,
    ) -> Result<Option<Option<String>>, sea_orm::DbErr> {
        let lock = executor_session_lock(workspace_id);
        let _guard = lock.lock().await;

        let dir = project_directories::Entity::find_by_id(workspace_id)
            .one(&self.conn)
            .await?;

        let sessions_json = match dir {
            Some(d) => d.executor_sessions,
            None => return Ok(None),
        };

        // 解析 JSON 获取对应执行器的 session
        let sessions: HashMap<String, Option<String>> =
            serde_json::from_str(sessions_json.as_deref().unwrap_or("{}"))
            .unwrap_or_default();

        Ok(sessions.get(executor).cloned())
    }

    /// 更新指定工作空间指定执行器的私聊会话 session_id。
    ///
    /// 流程：
    /// 1. 读取现有 sessions JSON
    /// 2. 更新对应执行器的 session
    /// 3. 写回数据库
    ///
    /// 并发安全：持 per-workspace 互斥锁，与 get_executor_session 互斥，
    /// 防止并发请求的 session_id 互相覆盖。
    pub async fn set_executor_session(
        &self,
        workspace_id: i64,
        executor: &str,
        session_id: Option<String>,
    ) -> Result<(), sea_orm::DbErr> {
        // 持 per-workspace 互斥锁串行化「读-改-写」，与 get_executor_session 互斥。
        // 避免并发请求读取到过期的 session_id，或多个请求的 session_id 互相覆盖。
        let lock = executor_session_lock(workspace_id);
        let _guard = lock.lock().await;

        // 读取现有记录
        let dir = project_directories::Entity::find_by_id(workspace_id)
            .one(&self.conn)
            .await?
            .ok_or_else(|| sea_orm::DbErr::RecordNotFound("project directory not found".into()))?;

        // 解析现有 JSON
        let mut sessions: HashMap<String, Option<String>> =
            serde_json::from_str(dir.executor_sessions.as_deref().unwrap_or("{}"))
            .unwrap_or_default();

        // 更新该执行器的 session
        sessions.insert(executor.to_string(), session_id);

        // 序列化并写回
        let now = crate::models::utc_timestamp();
        let am = project_directories::ActiveModel {
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
