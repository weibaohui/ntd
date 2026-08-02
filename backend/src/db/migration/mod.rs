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
mod v67;
mod v68;
mod v69;
mod v70;
mod v71;
mod v72;
mod v73;
mod v74;
mod v75;
mod v76;
mod v77;
mod v78;
mod v79;
mod v80;
mod v81;
mod v82;
mod v83;
mod v84;
mod v85;
mod v86;
mod v87;

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
        // V67 在 V66 之后：execution_records 新增 agent_runs，存储多 Agent 协作的子 agent 元数据（JSON）
        Box::new(v67::V67AddExecutionRecordsAgentRuns),
        // V68 在 V67 之后：executors 加 default_model、todos 加 model，支撑「执行器/todo 级别指定执行模型」
        Box::new(v68::V68AddModelColumns),
        // V69 在 V68 之后：quick_buttons 增加 workspace_id，实现 workspace 隔离
        Box::new(v69::V69AddQuickButtonsWorkspaceId),
        // V70 在 V69 之后：workspace_settings 增加 system_prompt 列，
        // 支撑需求 022「工作空间 Prompt」——每个 workspace 一份共享前置 prompt
        Box::new(v70::V70AddWorkspaceSettingsSystemPrompt),
        // V71 在 V70 之后：工艺管理 M1 数据模型，
        // 支撑需求 025「工艺管理」——模板、阶段、产物、门禁、返工
        Box::new(v71::V71ProcessManagement),
        // V72 在 V71 之后：工艺管理 M3 ——
        // 版本管理（previous_version_id）+ 四流闭预留表（洞察/治理/资产流）
        Box::new(v72::V72ProcessManagementV2),
        Box::new(v73::V73TaskManagement),
        // V74 在 V73 之后：回填历史环节 todo 的 auto_review_enabled，
        // 配合 installer 穿透修复「工艺选 AI 评审却从不打分」的存量数据
        Box::new(v74::V74BackfillAutoReview),
        // V75 在 V74 之后：loop_steps 新增 review_prompt 列，
        // 支撑需求 033「环节评审模板」——环节内联完整评审模板，按环节定制评审
        Box::new(v75::V75AddLoopStepReviewPrompt),
        // V76 在 V75 之后：loops 新增 abnormal_handler_prompt 列，
        // 支撑需求 035「工艺驱动异常处理」——异常处理提示词改为工艺 YAML 定义
        Box::new(v76::V76AddLoopAbnormalHandlerPrompt),
        // V77 在 V76 之后：删除 loop_phases.acceptance_criteria 死列，
        // 支撑需求 036「验收标准归环节」——阶段级验收标准移除，只归环节
        Box::new(v77::V77DropLoopPhaseAcceptanceCriteria),
        // V78 在 V77 之后：工艺定义正文移出 process_templates 表，
        // 改为只存 source_path 并按路径从磁盘文件读取，磁盘成为唯一真源。
        Box::new(v78::V78ProcessDefinitionToFile),
        // V79 在 V78 之后：process_templates 引入 guid 身份列并重建表，
        // 支撑需求 040「工艺模板 GUID 身份」——name 放开唯一，复制同名共存
        Box::new(v79::V79ProcessTemplateGuid),
        // V80 在 V79 之后：环路瘦身（需求 044）——手工环路级联删除，
        // 触发器表下线，loops/loop_steps 冗余定义列删除；YAML 成为唯一定义来源
        // V81 在 V80 之后：删除 loop_steps.review_type 死列（需求 048）——
        // 评审/门禁统一由 gate_config 表达，review_type 与 gate 语义重复已废弃
        Box::new(v80::V80LoopSlim),
        Box::new(v81::V81DropLoopStepReviewType),
        // V82 在 V81 之后：删除废弃的 process_step_templates 表（需求 052）——
        // step_template 原型机制已内联进 YAML link，表仅剩 bundled 缓存与查错表的 warn
        Box::new(v82::V82DropProcessStepTemplates),
        // V83 在 V82 之后：loop_steps 新增 step_template_refs 列（需求 054）——
        // 持久化环节 spec 模板引用，供执行器执行时注入 prompt「重点阅读」
        Box::new(v83::V83AddLoopStepTemplateRefs),
        // V84 在 V83 之后：todos 新增 skills 列（需求 055）——
        // 事项携带技能列表，安装工艺时从环节 skills 写入，执行时注入 prompt
        Box::new(v84::V84AddTodoSkills),
        // V85 在 V84 之后：loop_phase_executions 外键改为 SET NULL（BUG-008）——
        // 升级重建 loop_phases 时不再级联删除历史 phase 执行记录
        Box::new(v85::V85PhaseExecSetNull),
        // V86 在 V85 之后：process_template_versions 版本快照表（BUG-005）——
        // 保存工艺时记录版本快照，versions/diff 从快照读取
        Box::new(v86::V86ProcessTemplateVersions),
        // V87 在 V86 之后：残留态 DB 自愈（BUG-009 / Issue #973）——
        // 幂等探测并补齐历史残留态缺失的关键列，让中断/回退过的库能自愈启动
        Box::new(v87::V87SelfHealResidual),
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
