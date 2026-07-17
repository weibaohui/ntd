//! 数据库迁移框架
//!
//! 通过 `schema_version` 表追踪已应用的迁移版本号，启动时只执行尚未应用的迁移，
//! 把冷启动成本从 O(全部 DDL) 降到 O(待执行迁移)。

use async_trait::async_trait;
use sea_orm::{ConnectionTrait, DbBackend, Statement};

use super::Database;

mod v1;
mod v2_v5;
mod v41_v46;
mod v47_v53;
mod v54;
mod v55;
mod v56;
mod v57;
mod v58;
mod v59;
mod v60;
mod v61;
mod v62;
mod v63;
mod v64;
mod v65;
mod v66;

pub use v2_v5::read_applied_versions;
pub use v2_v5::drop_column_if_exists;

/// 一个数据库迁移。每个迁移是「版本号 + 名字 + 升级函数」的不可变组合。
#[async_trait]
pub(super) trait Migration: Send + Sync {
    /// 单调递增的版本号。新迁移必须严格大于已有版本。
    fn version(&self) -> i64;

    /// 简短的可读名字，用于日志与 `schema_version.name` 列。
    fn name(&self) -> &'static str;

    /// 执行迁移。失败时返回 `Err` 让 runner 中止启动（区别于「无害失败」）。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr>;
}

/// 按版本号升序返回所有已注册的迁移。
///
/// 新增迁移：在末尾追加一行即可，runner 会自动跳过已应用的并执行新版本。
pub(super) fn all_migrations() -> Vec<Box<dyn Migration>> {
    vec![
        Box::new(v1::V1InitialSchema),
        Box::new(v2_v5::V2TodoRatingDropColumn),
        Box::new(v2_v5::V3LogsToExecutionLogs),
        Box::new(v2_v5::V4FeishuFkCascade),
        Box::new(v2_v5::V5ProjectDirectoryWorktree),
        Box::new(v41_v46::V41ConsolidatedLoopFeatures),
        Box::new(v41_v46::V42ConsolidatedWorkspaceRefactor),
        Box::new(v41_v46::V43ConsolidatedFinalFeatures),
        Box::new(v41_v46::V44AddFeishuMessagesProcessedId),
        Box::new(v41_v46::V45AddTodosActionType),
        Box::new(v41_v46::V46AddTodosActionKey),
        Box::new(v47_v53::V47ConsolidatedBlackboardFeatures),
        Box::new(v54::V54AddWikiChatExecutor),
        Box::new(v55::V55AddWikiChatSessions),
        // V56 必须排在 V55 之后：补齐早期 V47 跳过建表演进列的旧部署遗留
        Box::new(v56::V56AddMissingBlackboardColumns),
        // V57 在 V56 之后：把写死的 Wiki 执行超时做成 per-workspace 可配置项
        Box::new(v57::V57AddWikiTimeoutSecs),
        // V58 在 V57 之后：todos 新增 archived_at，支撑事项中心「已归档」分类
        Box::new(v58::V58AddTodosArchivedAt),
        // V59 在 V58 之后：为 archived_at 建索引，加速日常视图的未归档过滤
        Box::new(v59::V59AddTodosArchivedAtIndex),
        // V60 在 V59 之后：为 feishu_messages 增加 error 字段，记录处理失败原因
        Box::new(v60::V60AddFeishuMessagesError),
        // V61 在 V60 之后：为 project_directories 增加 executor_sessions，存储私聊执行器 session
        Box::new(v61::V61AddProjectDirectoriesExecutorSessions),
        // V62 在 V61 之后：为 blackboards 增加 enabled 总开关
        Box::new(v62::V62AddBlackboardEnabled),
        // V63 在 V62 之后：为 executors 增加 is_default 字段，支持设置默认执行器
        Box::new(v63::V63AddExecutorIsDefault),
        // V64 在 V63 之后：agent_bots 新增 owner_open_id，作为推送目标权威来源；
        // 并把存量 p2p_receive_id 迁移过来，废弃 /sethome 手动填 ID 机制
        Box::new(v64::V64AddAgentBotOwnerOpenId),
        // V65 在 V64 之后：为 todos 增加 expert_name 字段，支持配置专家/团队
        Box::new(v65::V65AddTodoExpertName),
        // V66 在 V65 之后：新建 quick_buttons 表，支撑回复框自定义快捷话术按钮
        Box::new(v66::V66AddQuickButtonsTable),
    ]
}

/// 用 `PRAGMA table_info` 判断某列是否存在。
pub(super) async fn table_has_column(db: &Database, table: &str, column: &str) -> Result<bool, sea_orm::DbErr> {
    let sql = format!(
        "SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name='{}'",
        table, column
    );
    let row = db
        .conn
        .query_one(Statement::from_string(DbBackend::Sqlite, sql))
        .await?;
    Ok(row
        .and_then(|r| r.try_get_by_index::<i64>(0).ok())
        .unwrap_or(0)
        > 0)
}

/// 检测 sqlite_master 上是否有该表。
pub(super) async fn table_exists(db: &Database, table: &str) -> Result<bool, sea_orm::DbErr> {
    let sql = format!(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{}'",
        table
    );
    debug_assert!(
        !table.is_empty() && table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "table_exists: invalid table name {table:?}"
    );
    let row = db
        .conn
        .query_one(Statement::from_string(DbBackend::Sqlite, sql))
        .await?;
    Ok(row
        .and_then(|r| r.try_get_by_index::<i64>(0).ok())
        .unwrap_or(0)
        > 0)
}

/// 「探测列存在性 → 缺则 ALTER 追加」。
pub(super) async fn add_column_if_missing(
    db: &Database,
    table: &str,
    column: &str,
    alter_sql: &str,
) -> Result<(), sea_orm::DbErr> {
    if !table_has_column(db, table, column).await? {
        db.exec(alter_sql).await?;
    }
    Ok(())
}

/// 「执行一条 ALTER TABLE ADD COLUMN,失败仅 warn」。
pub(super) async fn add_column_warn(db: &Database, sql: &str) {
    if let Err(e) = db.exec(sql).await {
        tracing::warn!("migration v1: {}: {} (column likely already exists)", sql, e);
    }
}

/// 「先试 IF NOT EXISTS 版本,失败则回退到普通 ADD COLUMN」。
pub(super) async fn add_column_with_fallback(
    db: &Database,
    if_not_exists_sql: &str,
    fallback_sql: &str,
) -> Result<(), sea_orm::DbErr> {
    if let Err(e) = db.exec(if_not_exists_sql).await {
        tracing::debug!(
            "migration v1: IF NOT EXISTS ADD COLUMN failed ({}), falling back",
            e
        );
        add_column_warn(db, fallback_sql).await;
    }
    Ok(())
}

/// 按 path 查询 project_directories.id。
pub(super) async fn get_project_directory_id_by_path(
    db: &Database,
    path: &str,
) -> Result<Option<i64>, sea_orm::DbErr> {
    let stmt = sea_orm::Statement::from_sql_and_values(
        sea_orm::DbBackend::Sqlite,
        "SELECT id FROM project_directories WHERE path = ?1",
        vec![path.into()],
    );
    let row = db.conn.query_one(stmt).await?;
    let Some(row) = row else { return Ok(None) };
    let id: Option<i64> = row.try_get_by("id").ok().flatten();
    Ok(id)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod v1_helpers_tests {
    use super::*;

    async fn fresh_db() -> Database {
        Database::new(":memory:")
            .await
            .expect(":memory: db must open")
    }

    #[tokio::test]
    async fn table_has_column_true_for_existing() {
        let db = fresh_db().await;
        assert!(table_has_column(&db, "todos", "id").await.unwrap());
    }

    #[tokio::test]
    async fn table_has_column_false_for_missing() {
        let db = fresh_db().await;
        assert!(!table_has_column(&db, "todos", "nonexistent_col").await.unwrap());
    }

    #[tokio::test]
    async fn table_has_column_false_for_missing_table() {
        let db = fresh_db().await;
        assert!(!table_has_column(&db, "nonexistent_table", "id").await.unwrap());
    }

    #[tokio::test]
    async fn table_exists_true_for_existing() {
        let db = fresh_db().await;
        assert!(table_exists(&db, "todos").await.unwrap());
    }

    #[tokio::test]
    async fn table_exists_false_for_missing() {
        let db = fresh_db().await;
        assert!(!table_exists(&db, "nonexistent_table").await.unwrap());
    }
}
