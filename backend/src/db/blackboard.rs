//! 黑板（Blackboard）数据库层方法。
//!
//! 提供黑板的 CRUD 操作，每个工作空间最多一条黑板记录（由 UNIQUE 约束保证）。

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use sea_orm::{
    sea_query::OnConflict, ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait,
    QueryFilter, UpdateResult,
};
use tokio::sync::Mutex;

use super::entity::blackboards;
use super::Database;

/// Wiki 执行超时的下限（秒）。低于此值的配置会被钳制到此值，
/// 避免用户误填 0 或极小值导致 Wiki 任务刚启动就超时。
pub const MIN_WIKI_TIMEOUT_SECS: i64 = 60;
/// Wiki 执行超时的上限（秒）。超过此值视为异常配置，避免单次任务无限期占用资源。
pub const MAX_WIKI_TIMEOUT_SECS: i64 = 3600;
/// Wiki 执行超时的默认值（秒），与历史写死的 5 分钟一致。
pub const DEFAULT_WIKI_TIMEOUT_SECS: i64 = 300;

/// 将用户输入的 wiki 超时秒数钳制到合法区间。
///
/// 设计取舍：超时太小会让 Wiki 任务刚启动就超时；太大则可能让异常任务长期占用资源。
/// 因此设 [MIN, MAX] 区间，超界值会被静默钳制而非拒收，避免用户反复试错。
fn clamp_wiki_timeout(secs: i64) -> i64 {
    secs.clamp(MIN_WIKI_TIMEOUT_SECS, MAX_WIKI_TIMEOUT_SECS)
}

/// per-workspace 互斥锁，串行化 pending 队列的「读-改-写」操作。
///
/// 背景：`append_pending_record_id` 与 `remove_specific_pending_record_ids` 都是
/// 非原子的「读 → 改 → 写」三步。两者并发执行时，后写的会覆盖前写的，
/// 导致已被 remove 移除的 ID 被 append 的旧快照「复活」，队列永远不收敛——
/// worker 反复分析同一批 record，UI 持续显示「等待刷新 / N / 阈值 条」。
///
/// 用 per-workspace Mutex 把同一工作空间的 append / remove 串行化，
/// 不同 workspace 之间不互相阻塞。Mutex 懒初始化（首次用到才创建）。
static PENDING_QUEUE_LOCKS: OnceLock<std::sync::Mutex<HashMap<i64, Arc<Mutex<()>>>>> =
    OnceLock::new();

/// 取得指定 workspace 的队列互斥锁句柄（不存在则创建）。
///
/// 返回 Arc<Mutex> 而非直接守卫，是因为调用方需要在 await 之前获取守卫、
/// 在 await 之后释放，Arc 让守卫可以跨 await 点持有。
fn queue_lock(workspace_id: i64) -> Arc<Mutex<()>> {
    let outer = PENDING_QUEUE_LOCKS.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
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
    /// 根据 workspace_id 获取黑板内容。
    ///
    /// 返回 Option<blackboards::Model>，None 表示该工作空间还没有黑板记录。
    /// 新工作空间首次访问时返回 None，由 Service 层的 find_or_create 方法处理初始化。
    pub async fn get_blackboard(
        &self,
        workspace_id: i64,
    ) -> Result<Option<blackboards::Model>, sea_orm::DbErr> {
        blackboards::Entity::find()
            .filter(blackboards::Column::WorkspaceId.eq(workspace_id))
            .one(&self.conn)
            .await
    }

    /// 返回所有黑板记录（供 flush listener 广播状态用）。
    pub async fn get_all_blackboards(&self) -> Result<Vec<blackboards::Model>, sea_orm::DbErr> {
        blackboards::Entity::find().all(&self.conn).await
    }

    /// 为指定工作空间创建一条空的黑板记录。
    ///
    /// 幂等实现：使用 `ON CONFLICT(workspace_id) DO NOTHING` + 重新查询，
    /// 避免并发场景下两个请求同时走"先查后建"路径时因 UNIQUE 约束相互失败。
    /// 返回值始终是该工作空间当前的黑板记录（新建或已存在）。
    ///
    /// 新记录初始化时配置字段均采用默认值（防抖周期 600s、阈值 10 条、提示词为空使用内置）。
    pub async fn create_blackboard(
        &self,
        workspace_id: i64,
    ) -> Result<blackboards::Model, sea_orm::DbErr> {
        // 用 utc_timestamp() 统一时间源，避免不同 DB driver 时区差异
        let now = crate::models::utc_timestamp();
        // 构造 ActiveModel：除主键外的字段显式赋值，主键交由 SQLite 自增
        let model = blackboards::ActiveModel {
            workspace_id: ActiveValue::Set(workspace_id),
            // 初始内容为空：创建时的黑板无内容，由后续 LLM 更新填充
            content: ActiveValue::Set(String::new()),
            // 初始 pending 队列为空
            pending_record_ids: ActiveValue::Set("[]".to_string()),
            // 默认防抖周期 600 秒
            blackboard_debounce_secs: ActiveValue::Set(600),
            // 默认防抖条数阈值 10 条
            blackboard_debounce_count: ActiveValue::Set(10),
            // 空字符串表示使用内置默认提示词模板
            wiki_prompt: ActiveValue::Set(String::new()),
            // None 表示使用默认执行器 "claudecode"
            wiki_chat_executor: ActiveValue::Set(None),
            // Wiki 执行超时默认 5 分钟，与历史写死值一致
            wiki_timeout_secs: ActiveValue::Set(DEFAULT_WIKI_TIMEOUT_SECS),
            // 黑板功能默认启用
            enabled: ActiveValue::Set(1),
            updated_at: ActiveValue::Set(Some(now.clone())),
            created_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        // ON CONFLICT(workspace_id) DO NOTHING：若记录已存在则跳过 insert,
        // 避免并发竞争下两个并发请求都走 insert 路径时第二个失败。
        // 后续重读以拿到稳定的 Model（含实际的主键 id）。
        blackboards::Entity::insert(model)
            .on_conflict(
                OnConflict::column(blackboards::Column::WorkspaceId)
                    .do_nothing()
                    .to_owned(),
            )
            .exec_without_returning(&self.conn)
            .await?;
        // 重读：insert 的 ON CONFLICT DO NOTHING 不会返回行，必须重新查询拿主键
        blackboards::Entity::find()
            .filter(blackboards::Column::WorkspaceId.eq(workspace_id))
            .one(&self.conn)
            .await?
            // 极端情况：上一句 insert 后立刻被外部删除，理论上不会发生
            .ok_or_else(|| {
                sea_orm::DbErr::RecordNotFound(format!(
                    "blackboard for workspace {} not found after upsert",
                    workspace_id
                ))
            })
    }

    /// 更新指定工作空间的黑板内容（记录必须已存在）。
    ///
    /// 性能取舍：单条 `UPDATE ... WHERE workspace_id = ?`，避免原先 SELECT-then-UPDATE
    /// 的两次往返 + TOCTOU 窗口。如果记录不存在，rows_affected = 0，
    /// 返回 `RecordNotFound` 让调用方能识别这种情况。
    pub async fn update_blackboard_content(
        &self,
        workspace_id: i64,
        content: &str,
    ) -> Result<(), sea_orm::DbErr> {
        // 时间戳：单独变量确保 created_at / updated_at 用同一时刻
        let now = crate::models::utc_timestamp();
        // 单语句 UPDATE：workspace_id 是 UNIQUE 索引，命中后只更新一行
        let res: UpdateResult = blackboards::Entity::update_many()
            .col_expr(blackboards::Column::Content, content.into())
            .col_expr(blackboards::Column::UpdatedAt, now.into())
            .filter(blackboards::Column::WorkspaceId.eq(workspace_id))
            .exec(&self.conn)
            .await?;
        // rows_affected == 0 表示记录不存在（区别于"存在但内容相同"的 0 变更）
        if res.rows_affected == 0 {
            return Err(sea_orm::DbErr::RecordNotFound(format!(
                "blackboard for workspace {} not found",
                workspace_id
            )));
        }
        Ok(())
    }

    /// Upsert 黑板内容：记录不存在则创建，存在则更新。
    ///
    /// 通过 `INSERT ... ON CONFLICT(workspace_id) DO UPDATE` 一次往返完成
    /// 创建/更新判断 + 写入，避免 service 层先 get 再 create 再 update 的 3 次往返。
    /// 用 workspace_id 唯一约束做冲突判定，与 schema UNIQUE 保持一致。
    ///
    /// 新增字段（防抖阈值、提示词）仅在 INSERT 时填充默认值，冲突时不覆盖，
    /// 保持已有工作空间的配置不被意外重置。
    pub async fn upsert_blackboard_content(
        &self,
        workspace_id: i64,
        content: &str,
    ) -> Result<(), sea_orm::DbErr> {
        // 同一时刻填充 created_at 和 updated_at：upsert 时两个字段语义一致
        let now = crate::models::utc_timestamp();
        // 构造 ActiveModel：与 create_blackboard 保持一致的初始结构
        let am = blackboards::ActiveModel {
            workspace_id: ActiveValue::Set(workspace_id),
            content: ActiveValue::Set(content.to_string()),
            updated_at: ActiveValue::Set(Some(now.clone())),
            created_at: ActiveValue::Set(Some(now)),
            pending_record_ids: ActiveValue::Set("[]".to_string()),
            blackboard_debounce_secs: ActiveValue::Set(600),
            blackboard_debounce_count: ActiveValue::Set(10),
            wiki_prompt: ActiveValue::Set(String::new()),
            wiki_chat_executor: ActiveValue::Set(None),
            // 新建行时给 Wiki 超时填默认值；冲突分支不覆盖，保留用户已调过的配置
            wiki_timeout_secs: ActiveValue::Set(DEFAULT_WIKI_TIMEOUT_SECS),
            ..Default::default()
        };
        // ON CONFLICT(workspace_id)：命中后只覆盖 content/updated_at，保留 created_at 和配置字段
        blackboards::Entity::insert(am)
            .on_conflict(
                OnConflict::column(blackboards::Column::WorkspaceId)
                    .update_columns([blackboards::Column::Content, blackboards::Column::UpdatedAt])
                    .to_owned(),
            )
            .exec(&self.conn)
            .await?;
        Ok(())
    }

    /// 追加一个 execution_record_id 到黑板的 pending 队列。
    ///
    /// ORM 方式：读 → JSON parse → push → 序列化 → 写回。
    /// 并发安全由 workspace_id 唯一约束保证串行写入。
    pub async fn append_pending_record_id(
        &self,
        workspace_id: i64,
        record_id: i64,
    ) -> Result<(), sea_orm::DbErr> {
        // 持 per-workspace 互斥锁串行化「读-改-写」，避免与并发的
        // remove_specific_pending_record_ids 互相覆盖：旧实现里 append 读快照后，
        // 若期间 remove 写回了新队列，append 仍会用旧快照+push 新 ID 覆盖回去，
        // 把已被 remove 移除的 ID「复活」，导致队列永不收敛。
        let lock = queue_lock(workspace_id);
        let _guard = lock.lock().await;

        // 读取当前队列
        let board = blackboards::Entity::find()
            .filter(blackboards::Column::WorkspaceId.eq(workspace_id))
            .one(&self.conn)
            .await?
            .ok_or_else(|| sea_orm::DbErr::RecordNotFound(format!(
                "blackboard for workspace {} not found",
                workspace_id
            )))?;

        // 解析 + 追加
        let mut ids: Vec<i64> = serde_json::from_str(&board.pending_record_ids)
            .unwrap_or_default();
        ids.push(record_id);
        let ids_json = serde_json::to_string(&ids).unwrap_or_default();

        // 写回：必须用 Unchanged 设置主键，sea-orm 的 update() 要求主键必须存在
        let now = crate::models::utc_timestamp();
        let _res = blackboards::ActiveModel {
            id: ActiveValue::Unchanged(board.id),
            workspace_id: ActiveValue::Unchanged(workspace_id),
            pending_record_ids: ActiveValue::Set(ids_json),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        }.update(&self.conn).await?;

        Ok(())
    }

    // 原 take_pending_record_ids（取出并清空 pending 队列）已删除：
    // 该方法从未被调用——实际 flush 路径（executor_service/completion.rs）用
    // get_blackboard 非破坏性读 + remove_specific_pending_record_ids 精准移除，
    // 不走"全量清空"。且其"读快照→写 []"非原子，若被并发调用会与 append 竞态丢记录；
    // 删除以消除这一潜在隐患，需要"取并清空"语义时请用带 queue_lock 的原子实现。

    /// 从 pending 队列中移除指定的 execution_record_id 列表，保留其余。
    ///
    /// 本方法只删除传入的 ID（而非全量清空），用于 flush listener 在 wiki 更新成功后
    /// 只移除已处理的记录、保留期间新到达的记录。
    pub async fn remove_specific_pending_record_ids(
        &self,
        workspace_id: i64,
        ids_to_remove: &[i64],
    ) -> Result<(), sea_orm::DbErr> {
        // 持 per-workspace 互斥锁串行化「读-改-写」，与 append_pending_record_id 互斥。
        // 旧实现非原子：remove 读快照后若期间 append 写回了新队列（含新 ID），
        // remove 仍会用旧快照 retain 后的结果覆盖回去，丢失 append 刚写入的新 ID；
        // 反之 append 读快照后若 remove 写回了精简队列，append 会把已移除的 ID「复活」。
        // 串行化后任一时刻只有一个操作在改队列，彻底消除覆盖竞态。
        let lock = queue_lock(workspace_id);
        let _guard = lock.lock().await;

        // 读取当前队列
        let board = blackboards::Entity::find()
            .filter(blackboards::Column::WorkspaceId.eq(workspace_id))
            .one(&self.conn)
            .await?
            .ok_or_else(|| sea_orm::DbErr::RecordNotFound(format!(
                "blackboard for workspace {} not found",
                workspace_id
            )))?;

        // 解析 → 过滤 → 写回（仅当有变化时才写 DB，减少无谓 IO）
        let mut ids: Vec<i64> = serde_json::from_str(&board.pending_record_ids)
            .unwrap_or_default();
        let before_len = ids.len();
        // 用 HashSet 做高效成员判断
        let remove_set: std::collections::HashSet<i64> = ids_to_remove.iter().copied().collect();
        ids.retain(|id| !remove_set.contains(id));
        if ids.len() != before_len {
            let now = crate::models::utc_timestamp();
            let _res = blackboards::ActiveModel {
                id: ActiveValue::Unchanged(board.id),
                workspace_id: ActiveValue::Unchanged(workspace_id),
                pending_record_ids: ActiveValue::Set(serde_json::to_string(&ids).unwrap_or_default()),
                updated_at: ActiveValue::Set(Some(now)),
                ..Default::default()
            }.update(&self.conn).await?;
        }
        Ok(())
    }

    /// 获取指定工作空间的黑板配置（防抖阈值、提示词、wiki_chat_executor）。
    ///
    /// 记录不存在时返回 None；调用方应确保黑板记录已通过 create_blackboard 初始化。
    pub async fn get_blackboard_config(
        &self,
        workspace_id: i64,
    ) -> Result<Option<BlackboardConfig>, sea_orm::DbErr> {
        let board = blackboards::Entity::find()
            .filter(blackboards::Column::WorkspaceId.eq(workspace_id))
            .one(&self.conn)
            .await?;
        Ok(board.map(|b| BlackboardConfig {
            debounce_secs: b.blackboard_debounce_secs,
            debounce_count: b.blackboard_debounce_count,
            wiki_prompt: b.wiki_prompt,
            wiki_chat_executor: b.wiki_chat_executor,
            wiki_chat_sessions: b.wiki_chat_sessions,
            wiki_timeout_secs: b.wiki_timeout_secs,
            enabled: b.enabled != 0,
        }))
    }

    /// 更新指定工作空间的黑板配置。
    ///
    /// 输入：workspace_id + 五个可选字段（debounce_secs、debounce_count、wiki_prompt、
    /// wiki_chat_executor、wiki_timeout_secs）。
    /// 流程：先按 workspace_id 查出黑板记录（不存在则 RecordNotFound）→ 构造 ActiveModel
    /// → 只对传入 Some 的字段写入，传入 None 的保持原值不变 → update。
    /// 防抖阈值有下限保护：debounce_secs >= 10，debounce_count >= 1。
    /// wiki_timeout_secs 有区间保护：钳制到 [MIN_WIKI_TIMEOUT_SECS, MAX_WIKI_TIMEOUT_SECS]。
    /// wiki_chat_executor 使用 Option<Option<String>>：
    ///   - 外层 None：不修改
    ///   - 外层 Some(None)：设为 NULL（使用默认执行器）
    ///   - 外层 Some(Some(s))：设为指定执行器名
    ///
    /// 记录不存在时返回 RecordNotFound；调用方须确保黑板记录已存在。
    #[allow(clippy::too_many_arguments)]
    pub async fn update_blackboard_config(
        &self,
        workspace_id: i64,
        debounce_secs: Option<i64>,
        debounce_count: Option<i64>,
        wiki_prompt: Option<String>,
        wiki_chat_executor: Option<Option<String>>,
        wiki_timeout_secs: Option<i64>,
        enabled: Option<bool>,
    ) -> Result<(), sea_orm::DbErr> {
        let board = blackboards::Entity::find()
            .filter(blackboards::Column::WorkspaceId.eq(workspace_id))
            .one(&self.conn)
            .await?
            .ok_or_else(|| {
                sea_orm::DbErr::RecordNotFound(format!(
                    "blackboard for workspace {} not found",
                    workspace_id
                ))
            })?;
        let now = crate::models::utc_timestamp();
        let mut am = blackboards::ActiveModel {
            id: ActiveValue::Unchanged(board.id),
            workspace_id: ActiveValue::Unchanged(workspace_id),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        if let Some(v) = debounce_secs {
            am.blackboard_debounce_secs = ActiveValue::Set(v.max(10));
        }
        if let Some(v) = debounce_count {
            am.blackboard_debounce_count = ActiveValue::Set(v.max(1));
        }
        if let Some(v) = wiki_prompt {
            am.wiki_prompt = ActiveValue::Set(v);
        }
        if let Some(v) = wiki_chat_executor {
            am.wiki_chat_executor = ActiveValue::Set(v);
        }
        if let Some(v) = wiki_timeout_secs {
            am.wiki_timeout_secs = ActiveValue::Set(clamp_wiki_timeout(v));
        }
        if let Some(v) = enabled {
            am.enabled = ActiveValue::Set(v as i64);
        }
        am.update(&self.conn).await?;
        Ok(())
    }

    /// 获取指定工作空间指定执行器的 Wiki Chat session ID。
    ///
    /// 返回值：
    /// - Some(Some(sid))：该执行器有 session
    /// - Some(None)：该执行器无 session（首次对话或不支持 session）
    /// - None：黑板记录不存在
    ///
    /// 并发安全：持 per-workspace 互斥锁，与 set_wiki_chat_session 互斥，
    /// 防止并发请求读取到过期的 session_id。
    pub async fn get_wiki_chat_session(
        &self,
        workspace_id: i64,
        executor: &str,
    ) -> Result<Option<Option<String>>, sea_orm::DbErr> {
        let lock = queue_lock(workspace_id);
        let _guard = lock.lock().await;

        let board = blackboards::Entity::find()
            .filter(blackboards::Column::WorkspaceId.eq(workspace_id))
            .one(&self.conn)
            .await?;

        let sessions_json = match board {
            Some(b) => b.wiki_chat_sessions,
            None => return Ok(None),
        };

        // 解析 JSON 获取对应执行器的 session
        let sessions: std::collections::HashMap<String, Option<String>> =
            serde_json::from_str(sessions_json.as_deref().unwrap_or("{}"))
            .unwrap_or_default();

        Ok(sessions.get(executor).cloned())
    }

    /// 更新指定工作空间指定执行器的 Wiki Chat session ID。
    ///
    /// 流程：
    /// 1. 读取现有 sessions JSON
    /// 2. 更新对应执行器的 session
    /// 3. 写回数据库
    ///
    /// 并发安全：持 per-workspace 互斥锁，与 get_wiki_chat_session 互斥，
    /// 防止并发请求的 session_id 互相覆盖。
    pub async fn set_wiki_chat_session(
        &self,
        workspace_id: i64,
        executor: &str,
        session_id: Option<String>,
    ) -> Result<(), sea_orm::DbErr> {
        // 持 per-workspace 互斥锁串行化「读-改-写」，与 get_wiki_chat_session 互斥。
        // 避免并发请求读取到过期的 session_id，或多个请求的 session_id 互相覆盖。
        let lock = queue_lock(workspace_id);
        let _guard = lock.lock().await;

        // 读取现有 sessions
        let board = blackboards::Entity::find()
            .filter(blackboards::Column::WorkspaceId.eq(workspace_id))
            .one(&self.conn)
            .await?
            .ok_or_else(|| sea_orm::DbErr::RecordNotFound("blackboard not found".into()))?;

        // 解析现有 JSON
        let mut sessions: std::collections::HashMap<String, Option<String>> =
            serde_json::from_str(board.wiki_chat_sessions.as_deref().unwrap_or("{}"))
            .unwrap_or_default();

        // 更新该执行器的 session
        sessions.insert(executor.to_string(), session_id);

        // 序列化并写回
        let now = crate::models::utc_timestamp();
        let am = blackboards::ActiveModel {
            id: ActiveValue::Unchanged(board.id),
            workspace_id: ActiveValue::Unchanged(workspace_id),
            wiki_chat_sessions: ActiveValue::Set(Some(serde_json::to_string(&sessions).unwrap_or_default())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        am.update(&self.conn).await?;
        Ok(())
    }
}

/// 黑板 per-workspace 配置数据结构，对应 blackboards 表的配置列。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BlackboardConfig {
    pub debounce_secs: i64,
    pub debounce_count: i64,
    pub wiki_prompt: String,
    /// Wiki 对话使用的执行器名称，None 或空表示使用默认值 "claudecode"
    pub wiki_chat_executor: Option<String>,
    /// Wiki 对话各执行器的 session ID（JSON 对象）。
    /// 例如：{"claudecode": "uuid-session-1", "hermes": "uuid-session-2"}
    pub wiki_chat_sessions: Option<String>,
    /// Wiki 执行超时（秒），控制 update_blackboard_wiki 等待与 Wiki 对话子进程的时长。
    pub wiki_timeout_secs: i64,
    /// 黑板功能总开关：true=启用，false=禁用。
    /// 关闭后不执行任何黑板相关逻辑（防抖入队、flush 刷新、Wiki 自动维护）。
    pub enabled: bool,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;
    use crate::db::Database;

    /// 创建一个测试用工作空间（project_directories），返回其 id。
    async fn create_test_workspace(db: &Database) -> i64 {
        db.create_project_directory("/tmp/test-blackboard-workspace", None, false, false)
            .await
            .expect("create workspace must succeed")
    }

    /// 验证 get_blackboard 在无记录时返回 None。
    #[tokio::test]
    async fn test_get_blackboard_returns_none_when_empty() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        // 不存在的 workspace_id 应返回 None
        let result = db.get_blackboard(999).await.unwrap();
        assert!(result.is_none());
    }

    /// 验证 create_blackboard 成功创建一条空黑板记录。
    #[tokio::test]
    async fn test_create_blackboard_success() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        let ws_id = create_test_workspace(&db).await;

        let board = db.create_blackboard(ws_id).await.unwrap();
        assert_eq!(board.workspace_id, ws_id);
        assert_eq!(board.content, "");
        assert!(board.created_at.is_some());
        assert!(board.updated_at.is_some());
        // 新增字段：默认值验证
        assert_eq!(board.blackboard_debounce_secs, 600);
        assert_eq!(board.blackboard_debounce_count, 10);
        assert_eq!(board.wiki_prompt, "");
        assert_eq!(board.wiki_chat_executor, None);
        assert_eq!(board.wiki_timeout_secs, DEFAULT_WIKI_TIMEOUT_SECS);

        // 验证可通过 get 查到
        let fetched = db.get_blackboard(ws_id).await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().id, board.id);
    }

    /// 验证 create_blackboard 在记录已存在时返回相同记录（幂等）。
    /// 防止并发场景下两个请求同时首次创建时第二个因 UNIQUE 约束失败。
    /// 行为：第二次调用应直接拿到第一条记录，不应 panic / 返回 Err。
    #[tokio::test]
    async fn test_create_blackboard_is_idempotent_for_same_workspace() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        let ws_id = create_test_workspace(&db).await;

        // 第一次：创建
        let first = db.create_blackboard(ws_id).await.unwrap();
        // 第二次：应幂等返回同一条记录（不会因 UNIQUE 冲突失败）
        let second = db.create_blackboard(ws_id).await.unwrap();
        assert_eq!(
            first.id, second.id,
            "重复 create_blackboard 应返回同一条记录的 id"
        );
        assert_eq!(second.workspace_id, ws_id);
        assert_eq!(second.content, "");
        // 数据库中应只有一条记录，没有产生重复行
        let all = blackboards::Entity::find()
            .filter(blackboards::Column::WorkspaceId.eq(ws_id))
            .all(&db.conn)
            .await
            .unwrap();
        assert_eq!(all.len(), 1, "同一 workspace 只能有一条黑板记录");
    }

    /// 验证 update_blackboard_content 更新成功。
    #[tokio::test]
    async fn test_update_blackboard_content_success() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        let ws_id = create_test_workspace(&db).await;
        let _ = db.create_blackboard(ws_id).await.unwrap();

        db.update_blackboard_content(ws_id, "# 更新后的内容")
            .await
            .unwrap();

        let fetched = db.get_blackboard(ws_id).await.unwrap().unwrap();
        assert_eq!(fetched.content, "# 更新后的内容");
    }

    /// 验证 update_blackboard_content 在不存在的 workspace 上返回 RecordNotFound。
    #[tokio::test]
    async fn test_update_blackboard_content_record_not_found() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");

        let result = db.update_blackboard_content(999, "# test").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            sea_orm::DbErr::RecordNotFound(_) => {} // 期望的错误类型
            other => panic!("expected RecordNotFound, got: {:?}", other),
        }
    }

    /// 验证 upsert_blackboard_content 在记录不存在时直接创建。
    #[tokio::test]
    async fn test_upsert_creates_when_missing() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        let ws_id = create_test_workspace(&db).await;

        // 首次 upsert：记录不存在，应当走 INSERT 分支
        db.upsert_blackboard_content(ws_id, "# 初始内容")
            .await
            .unwrap();

        let fetched = db.get_blackboard(ws_id).await.unwrap().unwrap();
        assert_eq!(fetched.content, "# 初始内容");
    }

    /// 验证 upsert_blackboard_content 在记录已存在时更新内容并保留 created_at。
    #[tokio::test]
    async fn test_upsert_updates_when_exists() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        let ws_id = create_test_workspace(&db).await;

        // 先 upsert 一次拿到初始记录
        db.upsert_blackboard_content(ws_id, "# 第一次")
            .await
            .unwrap();
        let first = db.get_blackboard(ws_id).await.unwrap().unwrap();
        let first_created = first.created_at.clone();
        let first_id = first.id;

        // 二次 upsert：ON CONFLICT 分支，应当覆盖 content 但保留 id/created_at
        db.upsert_blackboard_content(ws_id, "# 第二次")
            .await
            .unwrap();
        let second = db.get_blackboard(ws_id).await.unwrap().unwrap();

        assert_eq!(second.id, first_id, "upsert 不应改变主键");
        assert_eq!(second.content, "# 第二次", "content 应当被覆盖");
        assert_eq!(second.created_at, first_created, "created_at 应当保留");
    }

    /// 验证 get_blackboard_config 在无记录时返回 None。
    #[tokio::test]
    async fn test_get_blackboard_config_returns_none_when_missing() {
        let db = Database::new(":memory:").await.expect(":memory: must open");
        let result = db.get_blackboard_config(999).await.unwrap();
        assert!(result.is_none());
    }

    /// 验证 get_blackboard_config 在有记录时返回正确默认值。
    #[tokio::test]
    async fn test_get_blackboard_config_returns_defaults_after_create() {
        let db = Database::new(":memory:").await.expect(":memory: must open");
        let ws_id = create_test_workspace(&db).await;
        db.create_blackboard(ws_id).await.unwrap();

        let cfg = db.get_blackboard_config(ws_id).await.unwrap().unwrap();
        assert_eq!(cfg.debounce_secs, 600);
        assert_eq!(cfg.debounce_count, 10);
        assert_eq!(cfg.wiki_prompt, "");
        assert_eq!(cfg.wiki_chat_executor, None);
        assert_eq!(cfg.wiki_timeout_secs, DEFAULT_WIKI_TIMEOUT_SECS);
    }

    /// 验证 update_blackboard_config 正确更新各字段。
    #[tokio::test]
    async fn test_update_blackboard_config_updates_fields() {
        let db = Database::new(":memory:").await.expect(":memory: must open");
        let ws_id = create_test_workspace(&db).await;
        db.create_blackboard(ws_id).await.unwrap();

        db.update_blackboard_config(ws_id, Some(300), Some(5), Some("wiki".to_string()), Some(Some("codex".to_string())), None, None)
            .await
            .unwrap();

        let cfg = db.get_blackboard_config(ws_id).await.unwrap().unwrap();
        assert_eq!(cfg.debounce_secs, 300);
        assert_eq!(cfg.debounce_count, 5);
        assert_eq!(cfg.wiki_prompt, "wiki");
        assert_eq!(cfg.wiki_chat_executor, Some("codex".to_string()));
        // wiki_timeout_secs 未传 None → 应保留 create_blackboard 写入的默认 300
        assert_eq!(cfg.wiki_timeout_secs, DEFAULT_WIKI_TIMEOUT_SECS);
    }

    /// 验证 update_blackboard_config 对 None 字段保持原值。
    #[tokio::test]
    async fn test_update_blackboard_config_preserves_unchanged_fields() {
        let db = Database::new(":memory:").await.expect(":memory: must open");
        let ws_id = create_test_workspace(&db).await;
        db.create_blackboard(ws_id).await.unwrap();

        // 先全部更新
        db.update_blackboard_config(ws_id, Some(300), Some(5), Some("wiki".to_string()), Some(Some("kimi".to_string())), Some(600), None)
            .await
            .unwrap();

        // 再只更新其中两个
        db.update_blackboard_config(ws_id, Some(900), None, None, None, None, None)
            .await
            .unwrap();

        let cfg = db.get_blackboard_config(ws_id).await.unwrap().unwrap();
        assert_eq!(cfg.debounce_secs, 900);
        assert_eq!(cfg.debounce_count, 5, "debounce_count 应保留之前的值");
        assert_eq!(cfg.wiki_prompt, "wiki", "wiki_prompt 应保留之前的值");
        assert_eq!(cfg.wiki_chat_executor, Some("kimi".to_string()), "wiki_chat_executor 应保留之前的值");
        // wiki_timeout_secs 第二次传 None → 应保留第一次写入的 600
        assert_eq!(cfg.wiki_timeout_secs, 600, "wiki_timeout_secs 应保留之前的值");
    }

    /// 验证 update_blackboard_config 在记录不存在时返回 RecordNotFound。
    #[tokio::test]
    async fn test_update_blackboard_config_record_not_found() {
        let db = Database::new(":memory:").await.expect(":memory: must open");
        // 第 4 个参数 wiki_prompt 传 None，不参与此次测试验证
        let result = db.update_blackboard_config(999, Some(300), None, None, None, None, None).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            sea_orm::DbErr::RecordNotFound(_) => {}
            other => panic!("expected RecordNotFound, got: {:?}", other),
        }
    }

    /// 验证 update_blackboard_config 对防抖阈值做下限保护。
    #[tokio::test]
    async fn test_update_blackboard_config_debounce_minimum_guard() {
        let db = Database::new(":memory:").await.expect(":memory: must open");
        let ws_id = create_test_workspace(&db).await;
        db.create_blackboard(ws_id).await.unwrap();

        // 传入小于最小值的 debounce_secs，应被钳制到 10
        db.update_blackboard_config(ws_id, Some(3), None, None, None, None, None)
            .await
            .unwrap();
        let cfg = db.get_blackboard_config(ws_id).await.unwrap().unwrap();
        assert_eq!(cfg.debounce_secs, 10);

        // 传入小于最小值的 debounce_count，应被钳制到 1
        db.update_blackboard_config(ws_id, None, Some(0), None, None, None, None)
            .await
            .unwrap();
        let cfg = db.get_blackboard_config(ws_id).await.unwrap().unwrap();
        assert_eq!(cfg.debounce_count, 1);
    }

    /// 验证 update_blackboard_config 的 wiki_chat_executor: Some(None) 设为 NULL。
    #[tokio::test]
    async fn test_update_blackboard_config_set_executor_to_null() {
        let db = Database::new(":memory:").await.expect(":memory: must open");
        let ws_id = create_test_workspace(&db).await;
        db.create_blackboard(ws_id).await.unwrap();

        // 先设置一个值
        db.update_blackboard_config(ws_id, None, None, None, Some(Some("codex".to_string())), None, None)
            .await
            .unwrap();
        let cfg = db.get_blackboard_config(ws_id).await.unwrap().unwrap();
        assert_eq!(cfg.wiki_chat_executor, Some("codex".to_string()));

        // 再设为 None（清空回退到默认）
        db.update_blackboard_config(ws_id, None, None, None, Some(None), None, None)
            .await
            .unwrap();
        let cfg = db.get_blackboard_config(ws_id).await.unwrap().unwrap();
        assert_eq!(cfg.wiki_chat_executor, None);
    }

    /// 验证 update_blackboard_config 对 wiki_timeout_secs 做区间钳制。
    /// 低于下限应钳到 MIN_WIKI_TIMEOUT_SECS，高于上限应钳到 MAX_WIKI_TIMEOUT_SECS，
    /// 区间内原样保留。避免用户误填 0 或超大值导致任务刚启动就超时 / 长期占用资源。
    #[tokio::test]
    async fn test_update_blackboard_config_wiki_timeout_clamp() {
        let db = Database::new(":memory:").await.expect(":memory: must open");
        let ws_id = create_test_workspace(&db).await;
        db.create_blackboard(ws_id).await.unwrap();

        // 传入低于下限的值（0），应被钳制到 MIN_WIKI_TIMEOUT_SECS
        db.update_blackboard_config(ws_id, None, None, None, None, Some(0), None)
            .await
            .unwrap();
        let cfg = db.get_blackboard_config(ws_id).await.unwrap().unwrap();
        assert_eq!(cfg.wiki_timeout_secs, MIN_WIKI_TIMEOUT_SECS);

        // 传入高于上限的值，应被钳制到 MAX_WIKI_TIMEOUT_SECS
        db.update_blackboard_config(ws_id, None, None, None, None, Some(MAX_WIKI_TIMEOUT_SECS + 1000), None)
            .await
            .unwrap();
        let cfg = db.get_blackboard_config(ws_id).await.unwrap().unwrap();
        assert_eq!(cfg.wiki_timeout_secs, MAX_WIKI_TIMEOUT_SECS);

        // 区间内的值原样保留
        db.update_blackboard_config(ws_id, None, None, None, None, Some(600), None)
            .await
            .unwrap();
        let cfg = db.get_blackboard_config(ws_id).await.unwrap().unwrap();
        assert_eq!(cfg.wiki_timeout_secs, 600);
    }

    /// 并发回归测试：append 与 remove 交错执行时，队列必须正确收敛，
    /// 不允许 append 的旧快照「复活」已被 remove 移除的 ID。
    ///
    /// 场景：先 append [1,2,3]，启动一个并发任务不断 append 新 ID（4..N），
    /// 主任务同时调 remove_specific_pending_record_ids([1,2,3])。
    /// 修复前（无互斥锁）：append 与 remove 的读-改-写交错会互相覆盖，
    ///   要么丢新追加的 ID，要么把已移除的 1/2/3 写回来。
    /// 修复后（per-workspace Mutex）：操作串行化，最终队列应恰好等于
    ///   「本次并发追加的所有 ID 减去被 remove 的 [1,2,3]」，无丢失无复活。
    ///
    /// 由于 SQLite :memory: 单连接 + tokio 单线程协作式调度，
    /// 真正的并发竞态需要 yield 点制造交错——append/remove 内部的 await
    /// 就是天然的 yield 点，多任务并发调用时调度器会在这些点切换。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_append_remove_converges_under_concurrency() {
        let db = std::sync::Arc::new(
            Database::new(":memory:").await.expect(":memory: must open"),
        );
        let ws_id = create_test_workspace(&db).await;
        db.create_blackboard(ws_id).await.unwrap();

        // 预填充 [1,2,3]
        for id in 1..=3 {
            db.append_pending_record_id(ws_id, id).await.unwrap();
        }

        // 并发任务：连续 append 4..=100，与主任务的 remove 交错
        let db_clone = db.clone();
        let append_handle = tokio::spawn(async move {
            for id in 4..=100 {
                // 每次都重新 clone Arc，避免 move 语义影响下一轮
                db_clone.append_pending_record_id(ws_id, id).await.unwrap();
            }
        });

        // 主任务：反复 remove [1,2,3]，确保它们在任何时刻都不会被「复活」
        // 循环多次以增加与 append 交错的概率
        for _ in 0..20 {
            db.remove_specific_pending_record_ids(ws_id, &[1, 2, 3])
                .await
                .unwrap();
        }

        append_handle.await.unwrap();

        // 最终一致性检查：队列应恰好包含 4..=100，不含 1/2/3
        let board = db.get_blackboard(ws_id).await.unwrap().unwrap();
        let ids: Vec<i64> = serde_json::from_str(&board.pending_record_ids).unwrap_or_default();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(
            sorted, (4..=100).collect::<Vec<_>>(),
            "队列未收敛：append 与 remove 并发后应恰好是 4..=100，实际 {:?}。\
             若含 1/2/3 说明 append 旧快照复活了已移除 ID；若缺号说明 remove 覆盖丢失了 append 的新 ID",
            ids
        );
    }

    /// 并发回归测试：多个 append 并发执行不应丢失任何 ID。
    ///
    /// 修复前：两个 append 并发读同一快照，各自 push 后写回，后写覆盖前写，
    ///   丢失一个 ID。修复后：per-workspace Mutex 串行化，所有 append 都落库。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_concurrent_appends_no_loss() {
        let db = std::sync::Arc::new(
            Database::new(":memory:").await.expect(":memory: must open"),
        );
        let ws_id = create_test_workspace(&db).await;
        db.create_blackboard(ws_id).await.unwrap();

        // 启动 10 个并发任务，每个 append 10 个不同 ID（1..=100）
        let mut handles = Vec::new();
        for chunk_start in (1..=100).step_by(10) {
            let db_clone = db.clone();
            handles.push(tokio::spawn(async move {
                for id in chunk_start..chunk_start + 10 {
                    db_clone.append_pending_record_id(ws_id, id).await.unwrap();
                }
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        // 100 个 ID 必须全部落库，无丢失
        let board = db.get_blackboard(ws_id).await.unwrap().unwrap();
        let ids: Vec<i64> = serde_json::from_str(&board.pending_record_ids).unwrap_or_default();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(
            sorted, (1..=100).collect::<Vec<_>>(),
            "并发 append 丢失 ID：期望 1..=100 全部落库，实际 {:?}",
            ids
        );
    }

    /// 验证 update_blackboard_config 正确更新 enabled 字段。
    #[tokio::test]
    async fn test_update_blackboard_config_enabled_toggle() {
        let db = Database::new(":memory:").await.expect(":memory: must open");
        let ws_id = create_test_workspace(&db).await;
        db.create_blackboard(ws_id).await.unwrap();

        // 默认启用
        let cfg = db.get_blackboard_config(ws_id).await.unwrap().unwrap();
        assert!(cfg.enabled, "默认应为启用");

        // 禁用
        db.update_blackboard_config(ws_id, None, None, None, None, None, Some(false))
            .await
            .unwrap();
        let cfg = db.get_blackboard_config(ws_id).await.unwrap().unwrap();
        assert!(!cfg.enabled, "应为禁用");

        // 重新启用
        db.update_blackboard_config(ws_id, None, None, None, None, None, Some(true))
            .await
            .unwrap();
        let cfg = db.get_blackboard_config(ws_id).await.unwrap().unwrap();
        assert!(cfg.enabled, "应为启用");
    }
}
