//! 数据库迁移 V71：工艺管理（Process Management）M1 数据模型
//!
//! ## 背景
//! 需求 025「工艺管理」要求把可复用的流程套路显式建模为工艺模板，
//! 并在 Loop 执行中支持阶段、产物捕获、门禁评价与返工。
//!
//! ## 本次迁移内容
//! 1. 新建 6 张表：
//!    - `process_templates`：工艺模板市场。
//!    - `process_step_templates`：工艺环节原型。
//!    - `loop_phases`：Loop 内阶段定义。
//!    - `loop_step_artifacts`：环节产物快照。
//!    - `loop_step_execution_gates`：门禁评价记录。
//!    - `loop_phase_executions`：一次 Loop 执行中各阶段运行记录。
//! 2. 扩展 3 张表：
//!    - `loops`：增加 `process_template_id`、`process_template_version`。
//!    - `loop_steps`：增加 `phase_id`、`expected_artifacts`、`gate_config`、
//!      `max_rework`、`skill_names`、`expert_name`。
//!    - `loop_step_executions`：增加 `rework_count`。
//!
//! ## 幂等
//! 所有 `CREATE TABLE` 均带 `IF NOT EXISTS`；
//! 所有 `ALTER TABLE ADD COLUMN` 均通过 `add_column_if_missing` 先检查列存在性。
//! 重复执行不会报错。

use async_trait::async_trait;

use super::super::Database;
use super::{add_column_if_missing, Migration};

/// V71：工艺管理 M1 数据模型。
pub(super) struct V71ProcessManagement;

#[async_trait]
impl Migration for V71ProcessManagement {
    /// 单调递增版本号，紧接 V70。
    fn version(&self) -> i64 {
        71
    }

    /// 日志与 `schema_version.name` 列使用的简短名字。
    fn name(&self) -> &'static str {
        "V71ProcessManagement"
    }

    /// 创建工艺管理相关表并扩展现有表。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        create_process_templates_table(db).await?;
        create_process_step_templates_table(db).await?;
        create_loop_phases_table(db).await?;
        create_loop_step_artifacts_table(db).await?;
        create_loop_step_execution_gates_table(db).await?;
        create_loop_phase_executions_table(db).await?;
        extend_existing_tables(db).await?;
        tracing::info!("V71: 工艺管理 M1 数据模型已应用");
        Ok(())
    }
}

/// 创建 `process_templates` 表。
async fn create_process_templates_table(db: &Database) -> Result<(), sea_orm::DbErr> {
    db.exec(
        "CREATE TABLE IF NOT EXISTS process_templates (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            display_name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            category TEXT NOT NULL DEFAULT '',
            complexity TEXT NOT NULL DEFAULT 'standard',
            version TEXT NOT NULL DEFAULT '1.0.0',
            definition TEXT NOT NULL,
            source_path TEXT,
            workspace_id INTEGER,
            is_system INTEGER NOT NULL DEFAULT 0,
            created_at TEXT,
            updated_at TEXT
        )",
    )
    .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_process_templates_name ON process_templates(name)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_process_templates_category ON process_templates(category)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_process_templates_workspace ON process_templates(workspace_id)").await?;
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_process_templates_created_at_utc AFTER INSERT ON process_templates
         WHEN new.created_at IS NULL OR new.created_at = ''
         BEGIN UPDATE process_templates SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid; END",
    )
    .await?;
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_process_templates_updated_at_utc BEFORE UPDATE ON process_templates
         WHEN new.updated_at IS NULL OR new.updated_at = ''
         BEGIN UPDATE process_templates SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid; END",
    )
    .await?;
    Ok(())
}

/// 创建 `process_step_templates` 表。
async fn create_process_step_templates_table(db: &Database) -> Result<(), sea_orm::DbErr> {
    db.exec(
        "CREATE TABLE IF NOT EXISTS process_step_templates (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL,
            prompt TEXT NOT NULL DEFAULT '',
            executor TEXT,
            expert_name TEXT,
            skill_names TEXT NOT NULL DEFAULT '[]',
            model TEXT,
            acceptance_criteria TEXT NOT NULL DEFAULT '',
            workspace_id INTEGER,
            is_system INTEGER NOT NULL DEFAULT 0,
            source_path TEXT,
            created_at TEXT,
            updated_at TEXT
        )",
    )
    .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_process_step_templates_name ON process_step_templates(name)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_process_step_templates_workspace ON process_step_templates(workspace_id)").await?;
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_process_step_templates_created_at_utc AFTER INSERT ON process_step_templates
         WHEN new.created_at IS NULL OR new.created_at = ''
         BEGIN UPDATE process_step_templates SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid; END",
    )
    .await?;
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_process_step_templates_updated_at_utc BEFORE UPDATE ON process_step_templates
         WHEN new.updated_at IS NULL OR new.updated_at = ''
         BEGIN UPDATE process_step_templates SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid; END",
    )
    .await?;
    Ok(())
}

/// 创建 `loop_phases` 表。
async fn create_loop_phases_table(db: &Database) -> Result<(), sea_orm::DbErr> {
    db.exec(
        "CREATE TABLE IF NOT EXISTS loop_phases (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            loop_id INTEGER NOT NULL,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            order_index INTEGER NOT NULL DEFAULT 0,
            spec TEXT NOT NULL DEFAULT '',
            acceptance_criteria TEXT NOT NULL DEFAULT '',
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT,
            FOREIGN KEY (loop_id) REFERENCES loops(id) ON DELETE CASCADE
        )",
    )
    .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_loop_phases_loop_id ON loop_phases(loop_id)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_loop_phases_loop_order ON loop_phases(loop_id, order_index)").await?;
    db.exec(
        "CREATE TRIGGER IF NOT EXISTS set_loop_phases_created_at_utc AFTER INSERT ON loop_phases
         WHEN new.created_at IS NULL OR new.created_at = ''
         BEGIN UPDATE loop_phases SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid; END",
    )
    .await?;
    Ok(())
}

/// 创建 `loop_step_artifacts` 表。
async fn create_loop_step_artifacts_table(db: &Database) -> Result<(), sea_orm::DbErr> {
    db.exec(
        "CREATE TABLE IF NOT EXISTS loop_step_artifacts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            loop_step_execution_id INTEGER NOT NULL,
            name TEXT NOT NULL,
            artifact_type TEXT NOT NULL,
            locator TEXT NOT NULL,
            content_text TEXT,
            captured_at TEXT NOT NULL,
            captured_by TEXT,
            FOREIGN KEY (loop_step_execution_id) REFERENCES loop_step_executions(id) ON DELETE CASCADE
        )",
    )
    .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_loop_step_artifacts_exec ON loop_step_artifacts(loop_step_execution_id)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_loop_step_artifacts_name ON loop_step_artifacts(loop_step_execution_id, name)").await?;
    Ok(())
}

/// 创建 `loop_step_execution_gates` 表。
async fn create_loop_step_execution_gates_table(db: &Database) -> Result<(), sea_orm::DbErr> {
    db.exec(
        "CREATE TABLE IF NOT EXISTS loop_step_execution_gates (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            loop_step_execution_id INTEGER NOT NULL,
            gate_type TEXT NOT NULL,
            gate_name TEXT NOT NULL,
            config TEXT NOT NULL DEFAULT '{}',
            status TEXT NOT NULL DEFAULT 'pending',
            result TEXT,
            evaluated_at TEXT,
            evaluated_by TEXT,
            FOREIGN KEY (loop_step_execution_id) REFERENCES loop_step_executions(id) ON DELETE CASCADE
        )",
    )
    .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_loop_step_execution_gates_exec ON loop_step_execution_gates(loop_step_execution_id)").await?;
    Ok(())
}

/// 创建 `loop_phase_executions` 表。
async fn create_loop_phase_executions_table(db: &Database) -> Result<(), sea_orm::DbErr> {
    db.exec(
        "CREATE TABLE IF NOT EXISTS loop_phase_executions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            loop_execution_id INTEGER NOT NULL,
            phase_id INTEGER NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            started_at TEXT,
            finished_at TEXT,
            FOREIGN KEY (loop_execution_id) REFERENCES loop_executions(id) ON DELETE CASCADE,
            FOREIGN KEY (phase_id) REFERENCES loop_phases(id) ON DELETE CASCADE
        )",
    )
    .await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_loop_phase_executions_exec ON loop_phase_executions(loop_execution_id)").await?;
    db.exec("CREATE INDEX IF NOT EXISTS idx_loop_phase_executions_phase ON loop_phase_executions(phase_id)").await?;
    Ok(())
}

/// 扩展现有 `loops`、`loop_steps`、`loop_step_executions` 表。
async fn extend_existing_tables(db: &Database) -> Result<(), sea_orm::DbErr> {
    add_column_if_missing(
        db,
        "loops",
        "process_template_id",
        "ALTER TABLE loops ADD COLUMN process_template_id INTEGER REFERENCES process_templates(id) ON DELETE SET NULL",
    )
    .await?;
    add_column_if_missing(
        db,
        "loops",
        "process_template_version",
        "ALTER TABLE loops ADD COLUMN process_template_version TEXT",
    )
    .await?;

    add_column_if_missing(
        db,
        "loop_steps",
        "phase_id",
        "ALTER TABLE loop_steps ADD COLUMN phase_id INTEGER REFERENCES loop_phases(id) ON DELETE SET NULL",
    )
    .await?;
    add_column_if_missing(
        db,
        "loop_steps",
        "expected_artifacts",
        "ALTER TABLE loop_steps ADD COLUMN expected_artifacts TEXT NOT NULL DEFAULT '[]'",
    )
    .await?;
    add_column_if_missing(
        db,
        "loop_steps",
        "gate_config",
        "ALTER TABLE loop_steps ADD COLUMN gate_config TEXT NOT NULL DEFAULT '[]'",
    )
    .await?;
    add_column_if_missing(
        db,
        "loop_steps",
        "max_rework",
        "ALTER TABLE loop_steps ADD COLUMN max_rework INTEGER NOT NULL DEFAULT 3",
    )
    .await?;
    add_column_if_missing(
        db,
        "loop_steps",
        "skill_names",
        "ALTER TABLE loop_steps ADD COLUMN skill_names TEXT NOT NULL DEFAULT '[]'",
    )
    .await?;
    add_column_if_missing(
        db,
        "loop_steps",
        "expert_name",
        "ALTER TABLE loop_steps ADD COLUMN expert_name TEXT",
    )
    .await?;

    add_column_if_missing(
        db,
        "loop_step_executions",
        "rework_count",
        "ALTER TABLE loop_step_executions ADD COLUMN rework_count INTEGER NOT NULL DEFAULT 0",
    )
    .await?;

    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::bool_assert_comparison
)]
mod v71_tests {
    use super::super::{table_exists, table_has_column};
    use super::*;

    async fn fresh_db() -> Database {
        Database::new(":memory:")
            .await
            .expect(":memory: db must open")
    }

    #[tokio::test]
    async fn v71_creates_process_tables() {
        let db = fresh_db().await;
        V71ProcessManagement.up(&db).await.expect("V71 must apply");
        for table in [
            "process_templates",
            "process_step_templates",
            "loop_phases",
            "loop_step_artifacts",
            "loop_step_execution_gates",
            "loop_phase_executions",
        ] {
            assert!(
                table_exists(&db, table).await.unwrap(),
                "{table} must exist after V71"
            );
        }
    }

    #[tokio::test]
    async fn v71_extends_loop_columns() {
        let db = fresh_db().await;
        V71ProcessManagement.up(&db).await.expect("V71 must apply");
        assert!(table_has_column(&db, "loops", "process_template_id")
            .await
            .unwrap());
        assert!(table_has_column(&db, "loops", "process_template_version")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn v71_extends_loop_step_columns() {
        let db = fresh_db().await;
        V71ProcessManagement.up(&db).await.expect("V71 must apply");
        for col in [
            "phase_id",
            "expected_artifacts",
            "gate_config",
            "max_rework",
            "skill_names",
            "expert_name",
        ] {
            assert!(
                table_has_column(&db, "loop_steps", col).await.unwrap(),
                "loop_steps.{col} must exist after V71"
            );
        }
    }

    #[tokio::test]
    async fn v71_extends_loop_step_execution_columns() {
        let db = fresh_db().await;
        V71ProcessManagement.up(&db).await.expect("V71 must apply");
        assert!(table_has_column(&db,
            "loop_step_executions",
            "rework_count"
        )
        .await
        .unwrap());
    }

    #[tokio::test]
    async fn v71_is_idempotent() {
        let db = fresh_db().await;
        V71ProcessManagement.up(&db).await.expect("first V71 apply");
        V71ProcessManagement.up(&db).await.expect("second V71 apply must be idempotent");
        assert!(table_exists(&db, "process_templates").await.unwrap());
    }
}
