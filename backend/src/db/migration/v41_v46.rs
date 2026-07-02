use async_trait::async_trait;
use sea_orm::{ConnectionTrait, DbBackend, Statement};

use super::super::Database;
use super::{Migration, add_column_if_missing, add_column_warn, drop_column_if_exists, get_project_directory_id_by_path, table_has_column, table_exists};

pub(super) struct V41ConsolidatedLoopFeatures;

#[async_trait]
impl Migration for V41ConsolidatedLoopFeatures {
    fn version(&self) -> i64 { 41 }
    fn name(&self) -> &'static str { "consolidated_loop_features" }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // ---- V6: todos.kind 列 + 索引 ----
        add_column_warn(db, "ALTER TABLE todos ADD COLUMN kind TEXT NOT NULL DEFAULT 'item'").await;
        if table_has_column(db, "todos", "kind").await?
            && table_exists(db, "loop_steps").await?
        {
            // 回填：被 loop_steps 引用的 todo 升级为 step（兼容 step_id 和 todo_id 列名）
            if table_has_column(db, "loop_steps", "todo_id").await? {
                db.exec("UPDATE todos SET kind = 'step' WHERE id IN (SELECT DISTINCT todo_id FROM loop_steps)")
                    .await?;
            } else {
                db.exec("UPDATE todos SET kind = 'step' WHERE id IN (SELECT DISTINCT step_id FROM loop_steps)")
                    .await?;
            }
        }
        db.exec("CREATE INDEX IF NOT EXISTS idx_todos_kind ON todos(kind)").await?;

        // ---- V7: Loop Studio 6 张表 ----
        for stmt in CONSOLIDATED_LOOP_STUDIO_DDL {
            db.exec(stmt).await?;
        }

        // ---- V8: loops.workspace 列 ----
        add_column_warn(db, "ALTER TABLE loops ADD COLUMN workspace TEXT").await;

        // ---- V9: steps 独立表 ----
        db.exec(
            "CREATE TABLE IF NOT EXISTS steps (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                prompt TEXT NOT NULL DEFAULT '',
                executor TEXT,
                acceptance_criteria TEXT,
                source_todo_id INTEGER,
                created_at TEXT,
                updated_at TEXT,
                FOREIGN KEY (source_todo_id) REFERENCES todos(id) ON DELETE SET NULL
            )",
        )
        .await?;
        db.exec("CREATE INDEX IF NOT EXISTS idx_steps_source_todo ON steps(source_todo_id)").await?;
        db.exec(
            "CREATE TRIGGER IF NOT EXISTS set_steps_created_at_utc AFTER INSERT ON steps
             WHEN new.created_at IS NULL OR new.created_at = ''
             BEGIN UPDATE steps SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid; END",
        )
        .await?;
        db.exec(
            "CREATE TRIGGER IF NOT EXISTS set_steps_updated_at_utc AFTER UPDATE ON steps
             WHEN new.updated_at IS NULL OR new.updated_at = ''
             BEGIN UPDATE steps SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid; END",
        )
        .await?;
        // 回填 steps
        db.exec(
            "INSERT INTO steps (title, prompt, executor, acceptance_criteria, source_todo_id, created_at, updated_at)
             SELECT title, COALESCE(prompt, ''), executor, acceptance_criteria, id, created_at, updated_at
             FROM todos WHERE kind = 'step' AND id NOT IN (SELECT source_todo_id FROM steps WHERE source_todo_id IS NOT NULL)",
        )
        .await?;

        // ---- V10: steps.color ----
        add_column_warn(db, "ALTER TABLE steps ADD COLUMN color TEXT NOT NULL DEFAULT '#722ed1'").await;

        // ---- V11: 流程控制字段 ----
        add_column_warn(db, "ALTER TABLE loop_steps ADD COLUMN on_success TEXT NOT NULL DEFAULT 'next'").await;
        add_column_warn(db, "ALTER TABLE loop_steps ADD COLUMN success_goto_step_id BIGINT").await;
        add_column_warn(db, "ALTER TABLE loop_steps ADD COLUMN on_rating_fail TEXT NOT NULL DEFAULT 'break'").await;
        add_column_warn(db, "ALTER TABLE loop_steps ADD COLUMN fail_goto_step_id BIGINT").await;
        add_column_warn(db, "ALTER TABLE loops ADD COLUMN limits_config TEXT NOT NULL DEFAULT '{}'").await;
        add_column_warn(db, "ALTER TABLE loop_executions ADD COLUMN total_executed_steps INTEGER NOT NULL DEFAULT 0").await;
        add_column_warn(db, "ALTER TABLE loop_step_executions ADD COLUMN sequence_index INTEGER NOT NULL DEFAULT 0").await;
        add_column_warn(db, "ALTER TABLE loop_step_executions ADD COLUMN conclusion TEXT").await;

        // ---- V12: execution_records 追踪列 ----
        add_column_warn(db, "ALTER TABLE execution_records ADD COLUMN loop_step_execution_id BIGINT").await;
        add_column_warn(db, "ALTER TABLE execution_records ADD COLUMN step_id BIGINT").await;

        // ---- V14: loops.review_template_id ----
        add_column_warn(db, "ALTER TABLE loops ADD COLUMN review_template_id INTEGER").await;

        // ---- V15: review_templates 表 ----
        db.exec(
            "CREATE TABLE IF NOT EXISTS review_templates (
                id INTEGER PRIMARY KEY,
                name VARCHAR(128) NOT NULL,
                description VARCHAR(512),
                prompt TEXT NOT NULL,
                created_at TEXT,
                updated_at TEXT
            )",
        )
        .await?;
        // 迁移历史评审任务
        db.exec(
            "INSERT OR IGNORE INTO review_templates (id, name, description, prompt, created_at, updated_at)
             SELECT id, '默认评审任务', NULL, prompt, created_at, updated_at
             FROM todos WHERE todo_type = 1",
        )
        .await?;
        // 默认模板兜底
        let default_prompt = crate::services::auto_review::DEFAULT_REVIEWER_PROMPT;
        db.exec(&format!(
            "INSERT OR IGNORE INTO review_templates (name, description, prompt, created_at, updated_at)
             SELECT '默认评审任务', NULL, '{}', \
                     strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), \
                     strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
             WHERE NOT EXISTS (SELECT 1 FROM review_templates WHERE name = '默认评审任务')",
            default_prompt.replace('\'', "''")
        ))
        .await?;
        // 解绑并删除 type=1 todos（兼容 step_id 和 todo_id 列名）
        if table_has_column(db, "loop_steps", "todo_id").await? {
            db.exec("DELETE FROM loop_steps WHERE todo_id IN (SELECT id FROM todos WHERE todo_type = 1)")
                .await?;
        } else {
            db.exec("DELETE FROM loop_steps WHERE step_id IN (SELECT id FROM todos WHERE todo_type = 1)")
                .await?;
        }
        db.exec("DELETE FROM loop_hooks WHERE target_todo_id IN (SELECT id FROM todos WHERE todo_type = 1)")
            .await?;
        db.exec("DELETE FROM todos WHERE todo_type = 1").await?;
        add_column_warn(db, "ALTER TABLE todos ADD COLUMN review_template_id INTEGER").await;
        db.exec("CREATE INDEX IF NOT EXISTS idx_todos_review_template_id ON todos(review_template_id)").await?;

        // ---- V16: loop_step_executions 快照列 ----
        add_column_if_missing(db, "loop_step_executions", "min_rating",
            "ALTER TABLE loop_step_executions ADD COLUMN min_rating INTEGER").await?;
        add_column_if_missing(db, "loop_step_executions", "unrated_policy",
            "ALTER TABLE loop_step_executions ADD COLUMN unrated_policy TEXT").await?;
        add_column_if_missing(db, "loop_step_executions", "rating",
            "ALTER TABLE loop_step_executions ADD COLUMN rating INTEGER").await?;

        // ---- V17: 去重评审实例 todos ----
        db.exec(
            "DELETE FROM todos
             WHERE todo_type = 2
               AND deleted_at IS NULL
               AND id NOT IN (
                   SELECT MAX(id) FROM todos
                   WHERE todo_type = 2 AND deleted_at IS NULL
                   GROUP BY review_template_id
               )",
        )
        .await?;

        // ---- V18: 人工评审字段 ----
        add_column_warn(db, "ALTER TABLE loop_steps ADD COLUMN review_type TEXT NOT NULL DEFAULT 'ai'").await;
        add_column_warn(db, "ALTER TABLE loop_step_executions ADD COLUMN approval_status TEXT").await;
        add_column_warn(db, "ALTER TABLE loop_step_executions ADD COLUMN approval_comment TEXT").await;

        // ---- V19: 标签关联表 ----
        db.exec(
            "CREATE TABLE IF NOT EXISTS step_tags (
                step_id INTEGER NOT NULL, tag_id INTEGER NOT NULL,
                PRIMARY KEY (step_id, tag_id),
                FOREIGN KEY (step_id) REFERENCES steps(id) ON DELETE CASCADE,
                FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
            )",
        )
        .await?;
        db.exec("CREATE INDEX IF NOT EXISTS idx_step_tags_step_id ON step_tags(step_id)").await?;
        db.exec(
            "CREATE TABLE IF NOT EXISTS loop_tags (
                loop_id INTEGER NOT NULL, tag_id INTEGER NOT NULL,
                PRIMARY KEY (loop_id, tag_id),
                FOREIGN KEY (loop_id) REFERENCES loops(id) ON DELETE CASCADE,
                FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
            )",
        )
        .await?;
        db.exec("CREATE INDEX IF NOT EXISTS idx_loop_tags_loop_id ON loop_tags(loop_id)").await?;

        // ---- V23: 删除 hooks 列 ----
        drop_column_if_exists(db, "todos", "hooks").await?;
        drop_column_if_exists(db, "execution_records", "source_hook_id").await?;

        // ---- V24: 重建 loop_steps，修正 todo_id FK ----
        // 逻辑：step_id 列存在 → 旧库数据有误，重建表纠正；否则跳过
        if table_has_column(db, "loop_steps", "step_id").await? {
            db.exec("PRAGMA foreign_keys = OFF").await?;
            db.exec("DROP TABLE IF EXISTS loop_steps_new").await?;
            db.exec(
                "CREATE TABLE loop_steps_new (
                    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
                    loop_id BIGINT NOT NULL, name TEXT NOT NULL,
                    description TEXT NOT NULL DEFAULT '',
                    order_index INTEGER NOT NULL DEFAULT 0,
                    todo_id BIGINT NOT NULL,
                    run_mode TEXT NOT NULL DEFAULT 'sequential',
                    skip_on_source_failed INTEGER NOT NULL DEFAULT 0,
                    min_rating BIGINT,
                    unrated_policy TEXT NOT NULL DEFAULT 'skip',
                    on_success TEXT NOT NULL DEFAULT 'next',
                    success_goto_step_id BIGINT,
                    on_rating_fail TEXT NOT NULL DEFAULT 'break',
                    fail_goto_step_id BIGINT,
                    review_type TEXT NOT NULL DEFAULT 'ai',
                    enabled INTEGER NOT NULL DEFAULT 1,
                    created_at TEXT,
                    FOREIGN KEY (loop_id) REFERENCES loops(id) ON DELETE CASCADE,
                    FOREIGN KEY (todo_id) REFERENCES todos(id) ON DELETE RESTRICT
                )",
            )
            .await?;
            db.exec(
                "INSERT INTO loop_steps_new (id, loop_id, name, description, order_index,
                    todo_id, run_mode, skip_on_source_failed, min_rating, unrated_policy,
                    on_success, success_goto_step_id, on_rating_fail, fail_goto_step_id,
                    review_type, enabled, created_at)
                 SELECT id, loop_id, name, description, order_index,
                    step_id, run_mode, skip_on_source_failed, min_rating, unrated_policy,
                    on_success, success_goto_step_id, on_rating_fail, fail_goto_step_id,
                    review_type, enabled, created_at
                 FROM loop_steps",
            )
            .await?;
            db.exec("DROP TABLE loop_steps").await?;
            db.exec("ALTER TABLE loop_steps_new RENAME TO loop_steps").await?;
            db.exec("CREATE INDEX IF NOT EXISTS idx_loop_steps_loop_id ON loop_steps(loop_id)").await?;
            db.exec("CREATE INDEX IF NOT EXISTS idx_loop_steps_loop_order ON loop_steps(loop_id, order_index)").await?;
            db.exec("PRAGMA foreign_keys = ON").await?;
            tracing::info!("V41: loop_steps rebuilt, step_id → todo_id FK corrected");
        } else {
            tracing::info!("V41: loop_steps.step_id not present, skip rebuild (schema already correct)");
        }

        // ---- V26: 禁用普通事项自动评审 ----
        if table_has_column(db, "todos", "auto_review_enabled").await? {
            db.exec("UPDATE todos SET auto_review_enabled = 0 WHERE auto_review_enabled IS NULL OR auto_review_enabled = 1")
                .await?;
        }

        // ---- V27: 异常处理 Todo ----
        add_column_if_missing(db, "loops", "abnormal_handler_todo_id",
            "ALTER TABLE loops ADD COLUMN abnormal_handler_todo_id INTEGER REFERENCES todos(id) ON DELETE SET NULL").await?;
        add_column_if_missing(db, "loops", "abnormal_handler_trigger_on",
            "ALTER TABLE loops ADD COLUMN abnormal_handler_trigger_on TEXT NOT NULL DEFAULT '[\"capped_step\",\"capped_token\",\"failed\"]'").await?;

        // ---- V28: 移除 loop_step_executions.step_id FK 约束 ----
        db.exec("PRAGMA foreign_keys = OFF").await?;
        let drop_fk_sql = "ALTER TABLE loop_step_executions DROP FOREIGN KEY step_id";
        if db.exec(drop_fk_sql).await.is_err() {
            // 重建表方式
            let backup = "loop_step_executions_backup_v28";
            db.exec(&format!("ALTER TABLE loop_step_executions RENAME TO {}", backup)).await?;
            db.exec(
                r#"CREATE TABLE loop_step_executions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    loop_execution_id INTEGER NOT NULL, step_id INTEGER NOT NULL,
                    todo_id INTEGER NOT NULL, execution_record_id INTEGER,
                    status TEXT NOT NULL DEFAULT 'pending',
                    started_at TEXT, finished_at TEXT, error_message TEXT,
                    rating INTEGER, unrated_policy TEXT, conclusion TEXT,
                    approval_status TEXT, approval_comment TEXT,
                    min_rating INTEGER, sequence_index INTEGER NOT NULL DEFAULT 0,
                    FOREIGN KEY (loop_execution_id) REFERENCES loop_executions(id) ON DELETE CASCADE,
                    FOREIGN KEY (execution_record_id) REFERENCES execution_records(id) ON DELETE SET NULL
                )"#,
            )
            .await?;
            db.exec(&format!(
                "INSERT INTO loop_step_executions (id, loop_execution_id, step_id, todo_id, execution_record_id, status, started_at, finished_at, error_message, rating, unrated_policy, conclusion, approval_status, approval_comment, min_rating, sequence_index)
                 SELECT id, loop_execution_id, step_id, todo_id, execution_record_id, status, started_at, finished_at, error_message, rating, unrated_policy, conclusion, approval_status, approval_comment, min_rating, sequence_index FROM {}",
                backup
            ))
            .await?;
            db.exec("CREATE INDEX IF NOT EXISTS idx_loop_step_executions_loop_exec ON loop_step_executions(loop_execution_id)").await?;
            db.exec("CREATE INDEX IF NOT EXISTS idx_loop_step_executions_record ON loop_step_executions(execution_record_id)").await?;
            db.exec(&format!("DROP TABLE {}", backup)).await?;
        }
        db.exec("PRAGMA foreign_keys = ON").await?;

        // ---- V29: webhook_enabled 列 ----
        add_column_if_missing(db, "todos", "webhook_enabled",
            "ALTER TABLE todos ADD COLUMN webhook_enabled INTEGER NOT NULL DEFAULT 0").await?;
        add_column_if_missing(db, "loops", "webhook_enabled",
            "ALTER TABLE loops ADD COLUMN webhook_enabled INTEGER NOT NULL DEFAULT 0").await?;

        tracing::info!("V41 (consolidated_loop_features) applied");
        Ok(())
    }
}

/// V41 专用的 Loop Studio DDL：使用最终修正后的 schema（loop_steps.todo_id → todos(id)）。
/// 历史原因：原始 V7 使用 step_id → steps(id)，但 steps 表从未正确关联，
/// V24 重建后数据实际指向 todos.id，V41 直接使用修正后的 DDL，
/// 让从未运行过 V7 的数据库从一开始就走正确 schema。
const CONSOLIDATED_LOOP_STUDIO_DDL: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS loops (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL, description TEXT DEFAULT '',
        workspace TEXT, webhook_enabled INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL DEFAULT 'draft', color TEXT DEFAULT '#722ed1',
        icon TEXT DEFAULT 'loop', created_at TEXT, updated_at TEXT)",
    "CREATE INDEX IF NOT EXISTS idx_loops_status ON loops(status)",
    "CREATE INDEX IF NOT EXISTS idx_loops_updated_at ON loops(updated_at DESC)",
    "CREATE TRIGGER IF NOT EXISTS set_loops_created_at_utc AFTER INSERT ON loops
     WHEN new.created_at IS NULL OR new.created_at = ''
     BEGIN UPDATE loops SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ','now','utc') WHERE rowid = new.rowid; END",
    "CREATE TRIGGER IF NOT EXISTS set_loops_updated_at_utc BEFORE UPDATE ON loops
     WHEN new.updated_at IS NULL OR new.updated_at = ''
     BEGIN UPDATE loops SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now','utc') WHERE rowid = new.rowid; END",
    "CREATE TABLE IF NOT EXISTS loop_triggers (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        loop_id INTEGER NOT NULL, trigger_type TEXT NOT NULL,
        config TEXT DEFAULT '{}', enabled INTEGER NOT NULL DEFAULT 1,
        priority INTEGER NOT NULL DEFAULT 0, created_at TEXT,
        FOREIGN KEY (loop_id) REFERENCES loops(id) ON DELETE CASCADE)",
    "CREATE INDEX IF NOT EXISTS idx_loop_triggers_loop_id ON loop_triggers(loop_id)",
    "CREATE INDEX IF NOT EXISTS idx_loop_triggers_type_enabled ON loop_triggers(trigger_type, enabled)",
    "CREATE TRIGGER IF NOT EXISTS set_loop_triggers_created_at_utc AFTER INSERT ON loop_triggers
     WHEN new.created_at IS NULL OR new.created_at = ''
     BEGIN UPDATE loop_triggers SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ','now','utc') WHERE rowid = new.rowid; END",
    // 注意：这里用 todo_id 而非 step_id，FK → todos(id)（修正后的最终 schema）
    "CREATE TABLE IF NOT EXISTS loop_steps (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        loop_id INTEGER NOT NULL, name TEXT NOT NULL,
        description TEXT DEFAULT '', order_index INTEGER NOT NULL DEFAULT 0,
        todo_id BIGINT NOT NULL,
        run_mode TEXT NOT NULL DEFAULT 'sequential',
        skip_on_source_failed INTEGER NOT NULL DEFAULT 0,
        min_rating INTEGER,
        unrated_policy TEXT NOT NULL DEFAULT 'skip',
        on_success TEXT NOT NULL DEFAULT 'next',
        success_goto_step_id BIGINT,
        on_rating_fail TEXT NOT NULL DEFAULT 'break',
        fail_goto_step_id BIGINT,
        review_type TEXT NOT NULL DEFAULT 'ai',
        enabled INTEGER NOT NULL DEFAULT 1, created_at TEXT,
        FOREIGN KEY (loop_id) REFERENCES loops(id) ON DELETE CASCADE,
        FOREIGN KEY (todo_id) REFERENCES todos(id) ON DELETE RESTRICT)",
    "CREATE INDEX IF NOT EXISTS idx_loop_steps_loop_id ON loop_steps(loop_id)",
    "CREATE INDEX IF NOT EXISTS idx_loop_steps_loop_order ON loop_steps(loop_id, order_index)",
    "CREATE TRIGGER IF NOT EXISTS set_loop_steps_created_at_utc AFTER INSERT ON loop_steps
     WHEN new.created_at IS NULL OR new.created_at = ''
     BEGIN UPDATE loop_steps SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ','now','utc') WHERE rowid = new.rowid; END",
    "CREATE TABLE IF NOT EXISTS loop_hooks (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        loop_id INTEGER NOT NULL, hook_position TEXT NOT NULL,
        source_step_id INTEGER, target_todo_id INTEGER NOT NULL,
        skip_if_missing INTEGER NOT NULL DEFAULT 0,
        enabled INTEGER NOT NULL DEFAULT 1,
        min_rating INTEGER,
        unrated_policy TEXT NOT NULL DEFAULT 'skip', created_at TEXT,
        FOREIGN KEY (loop_id) REFERENCES loops(id) ON DELETE CASCADE,
        FOREIGN KEY (source_step_id) REFERENCES loop_steps(id) ON DELETE CASCADE,
        FOREIGN KEY (target_todo_id) REFERENCES todos(id) ON DELETE RESTRICT)",
    "CREATE INDEX IF NOT EXISTS idx_loop_hooks_loop_id ON loop_hooks(loop_id)",
    "CREATE INDEX IF NOT EXISTS idx_loop_hooks_source_stage ON loop_hooks(source_step_id)",
    "CREATE TRIGGER IF NOT EXISTS set_loop_hooks_created_at_utc AFTER INSERT ON loop_hooks
     WHEN new.created_at IS NULL OR new.created_at = ''
     BEGIN UPDATE loop_hooks SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ','now','utc') WHERE rowid = new.rowid; END",
    "CREATE TABLE IF NOT EXISTS loop_executions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        loop_id INTEGER NOT NULL, trigger_id INTEGER,
        trigger_type TEXT NOT NULL, trigger_meta TEXT DEFAULT '{}',
        started_at TEXT NOT NULL, finished_at TEXT,
        status TEXT NOT NULL DEFAULT 'running',
        total_steps INTEGER NOT NULL DEFAULT 0,
        completed_steps INTEGER NOT NULL DEFAULT 0,
        failed_steps INTEGER NOT NULL DEFAULT 0,
        FOREIGN KEY (loop_id) REFERENCES loops(id) ON DELETE CASCADE,
        FOREIGN KEY (trigger_id) REFERENCES loop_triggers(id) ON DELETE SET NULL)",
    "CREATE INDEX IF NOT EXISTS idx_loop_executions_loop_id ON loop_executions(loop_id)",
    "CREATE INDEX IF NOT EXISTS idx_loop_executions_started_at ON loop_executions(started_at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_loop_executions_status ON loop_executions(status)",
    "CREATE TABLE IF NOT EXISTS loop_step_executions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        loop_execution_id INTEGER NOT NULL,
        step_id INTEGER NOT NULL, todo_id INTEGER NOT NULL,
        execution_record_id INTEGER,
        status TEXT NOT NULL DEFAULT 'pending',
        started_at TEXT, finished_at TEXT, error_message TEXT,
        FOREIGN KEY (loop_execution_id) REFERENCES loop_executions(id) ON DELETE CASCADE,
        FOREIGN KEY (execution_record_id) REFERENCES execution_records(id) ON DELETE SET NULL)",
    "CREATE INDEX IF NOT EXISTS idx_loop_step_executions_loop_exec ON loop_step_executions(loop_execution_id)",
    "CREATE INDEX IF NOT EXISTS idx_loop_step_executions_record ON loop_step_executions(execution_record_id)",
];

/// V42 合并迁移：Workspace 多租户架构
///
/// 合并了以下历史迁移的完整逻辑：
/// V30  WorkspaceRefactor                   - workspace_slash_commands + workspace_settings 表 + agent_bots.workspace_id
/// V31  AddTodosWorkspaceId                - todos.workspace_id 列 + 回填
/// V32  ReviewTemplatesWorkspaceId          - review_templates.workspace_id 列
/// V33  EnsureReviewTemplatesWorkspaceId     - 修正 V32（确保 INTEGER 类型）
/// V34  MigrateOrphansToTempWorkspace       - 孤儿 todos/loops 迁到 /tmp 工作空间
/// V35  RenameWorkspaceToWorkspacePath       - todos/loops.workspace → workspace_path + loops.workspace_id
///                                        - 含 V35 的安全补丁（处理残留 NULL workspace_id）
///
/// 设计：6 个迁移逻辑上是一个大迁移的 6 个步骤，必须按顺序原子执行。
/// 幂等：每步内部检查「是否已做过」，确保从任意中间状态重启都能走完。
pub(super) struct V42ConsolidatedWorkspaceRefactor;

#[async_trait]
impl Migration for V42ConsolidatedWorkspaceRefactor {
    fn version(&self) -> i64 { 42 }
    fn name(&self) -> &'static str { "consolidated_workspace_refactor" }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // ---- V30: workspace_slash_commands + workspace_settings + agent_bots.workspace_id ----
        db.exec(
            "CREATE TABLE IF NOT EXISTS workspace_slash_commands (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                workspace_id INTEGER NOT NULL,
                slash_command TEXT NOT NULL, todo_id INTEGER NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at TEXT, updated_at TEXT,
                UNIQUE(workspace_id, slash_command)
            )",
        )
        .await?;
        db.exec(
            "CREATE TABLE IF NOT EXISTS workspace_settings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                workspace_id INTEGER NOT NULL UNIQUE,
                default_response_todo_id INTEGER, updated_at TEXT
            )",
        )
        .await?;
        add_column_if_missing(db, "agent_bots", "workspace_id",
            "ALTER TABLE agent_bots ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 0").await?;

        // ---- V31: todos.workspace_id ----
        add_column_if_missing(db, "todos", "workspace_id",
            "ALTER TABLE todos ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 0").await?;
        // 回填：按 workspace 路径匹配 project_directories.id
        // 兼容 workspace 和 workspace_path 列名（取决于数据库是否已跑过 V35）
        let ws_col = if table_has_column(db, "todos", "workspace").await? {
            "workspace"
        } else {
            "workspace_path"
        };
        db.exec(&format!(
            "UPDATE todos
             SET workspace_id = (
                 SELECT pd.id FROM project_directories pd
                 WHERE pd.path = todos.{0}
                 LIMIT 1
             )
             WHERE {0} IS NOT NULL AND {0} != '' AND (workspace_id IS NULL OR workspace_id = 0)",
            ws_col
        ))
        .await?;

        // ---- V32: review_templates.workspace_id ----
        add_column_if_missing(db, "review_templates", "workspace_id",
            "ALTER TABLE review_templates ADD COLUMN workspace_id INTEGER").await?;

        // ---- V33: 确保 review_templates.workspace_id 是 INTEGER（修正 V32 可能误加的 TEXT 列）----
        // 检查当前列类型，如果是 TEXT 则追加 INTEGER 列（ADD COLUMN IF NOT EXISTS 不会覆盖已有列）
        let col_type_ok = {
            let row = db.conn.query_one(
                Statement::from_string(DbBackend::Sqlite,
                    "SELECT COUNT(*) FROM pragma_table_info('review_templates') WHERE name='workspace_id' AND type='INTEGER'")
            ).await?;
            row.and_then(|r| r.try_get_by_index::<i64>(0).ok()).unwrap_or(0) > 0
        };
        if !col_type_ok {
            // 列存在但类型不对，尝试添加正确类型列（让应用层回填覆盖）
            add_column_if_missing(db, "review_templates", "workspace_id_int",
                "ALTER TABLE review_templates ADD COLUMN workspace_id_int INTEGER").await?;
        }

        // ---- V34: 孤儿记录迁到临时工作空间 /tmp ----
        // 创建 /tmp 工作空间（如不存在）
        db.exec(
            "INSERT OR IGNORE INTO project_directories (path, name, created_at, updated_at)
             SELECT '/tmp', '临时工作空间',
                    strftime('%Y-%m-%dT%H:%M:%SZ','now','utc'),
                    strftime('%Y-%m-%dT%H:%M:%SZ','now','utc')
             WHERE NOT EXISTS (SELECT 1 FROM project_directories WHERE path = '/tmp')",
        )
        .await?;
        // 获取 temp workspace id
        let temp_id = get_project_directory_id_by_path(db, "/tmp")
            .await?
            .unwrap_or(1);
        // 迁移孤儿 todos（同时兼容 workspace 和 workspace_path 列名）
        let todos_workspace_col = if table_has_column(db, "todos", "workspace").await? {
            "workspace"
        } else {
            "workspace_path"
        };
        let todos_sql = format!(
            "UPDATE todos \
            SET {0} = '/tmp', workspace_id = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now','utc') \
            WHERE deleted_at IS NULL \
              AND ({0} IS NULL OR {0} = '' OR workspace_id IS NULL OR workspace_id = 0)",
            todos_workspace_col
        );
        db.conn.execute(Statement::from_sql_and_values(DbBackend::Sqlite, todos_sql, vec![temp_id.into()]))
            .await?;
        // 迁移孤儿 loops（workspace 为空）
        let loops_workspace_col = if table_has_column(db, "loops", "workspace").await? {
            "workspace"
        } else {
            "workspace_path"
        };
        db.exec(&format!(
            "UPDATE loops SET {0} = '/tmp', updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now','utc') \
            WHERE {0} IS NULL OR {0} = ''",
            loops_workspace_col
        ))
        .await?;

        // ---- V35: todos.workspace → workspace_path, loops.workspace → workspace_path, loops.workspace_id ----
        // todos.workspace → workspace_path
        if table_has_column(db, "todos", "workspace").await?
            && !table_has_column(db, "todos", "workspace_path").await?
        {
            db.exec("ALTER TABLE todos RENAME COLUMN workspace TO workspace_path").await?;
        }
        // loops.workspace → workspace_path
        if table_has_column(db, "loops", "workspace").await?
            && !table_has_column(db, "loops", "workspace_path").await?
        {
            db.exec("ALTER TABLE loops RENAME COLUMN workspace TO workspace_path").await?;
        }
        // loops.workspace_id 新增 + 回填
        if !table_has_column(db, "loops", "workspace_id").await? {
            db.exec("ALTER TABLE loops ADD COLUMN workspace_id INTEGER").await?;
        }
        db.exec(
            "UPDATE loops
             SET workspace_id = (
                 SELECT pd.id FROM project_directories pd
                 WHERE pd.path = loops.workspace_path
                 LIMIT 1
             )
             WHERE workspace_path IS NOT NULL AND workspace_path != ''",
        )
        .await?;
        // todos.workspace_id 回填（workspace_path 非空但 workspace_id 为 0/NULL）
        db.exec(
            "UPDATE todos
             SET workspace_id = (
                 SELECT pd.id FROM project_directories pd
                 WHERE pd.path = todos.workspace_path
                 LIMIT 1
             )
             WHERE workspace_path IS NOT NULL AND workspace_path != ''
               AND (workspace_id IS NULL OR workspace_id = 0)",
        )
        .await?;
        // V35 安全补丁：仍有 workspace_id 为 NULL 的行 → 设为 temp workspace id
        db.exec(
            format!("UPDATE todos SET workspace_id = {temp_id} WHERE workspace_id IS NULL").as_str(),
        )
        .await?;

        tracing::info!("V42 (consolidated_workspace_refactor) applied");
        Ok(())
    }
}

/// V43 合并迁移：最终功能补充
///
/// 合并了以下历史迁移：
/// V36  LoopExecutionsErrorMessage    - loop_executions.error_message
/// V37  SlashCommandLoopSupport       - workspace_slash_commands.command_type/loop_id
/// V38  DefaultResponseType           - workspace_settings.default_response_*
/// V39  FixFeishuMessagesWorkspaceId  - feishu_messages.workspace_id
/// V40  DropFeishuMessagesProcessedTodoId - 删除 processed_todo_id 列
pub(super) struct V43ConsolidatedFinalFeatures;

#[async_trait]
impl Migration for V43ConsolidatedFinalFeatures {
    fn version(&self) -> i64 { 43 }
    fn name(&self) -> &'static str { "consolidated_final_features" }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // V36: loop_executions.error_message
        add_column_if_missing(db, "loop_executions", "error_message",
            "ALTER TABLE loop_executions ADD COLUMN error_message TEXT DEFAULT NULL").await?;

        // V37: workspace_slash_commands.command_type + loop_id
        add_column_if_missing(db, "workspace_slash_commands", "command_type",
            "ALTER TABLE workspace_slash_commands ADD COLUMN command_type TEXT DEFAULT 'todo'").await?;
        add_column_if_missing(db, "workspace_slash_commands", "loop_id",
            "ALTER TABLE workspace_slash_commands ADD COLUMN loop_id INTEGER").await?;

        // V38: workspace_settings.default_response_*
        add_column_if_missing(db, "workspace_settings", "default_response_type",
            "ALTER TABLE workspace_settings ADD COLUMN default_response_type TEXT").await?;
        add_column_if_missing(db, "workspace_settings", "default_response_loop_id",
            "ALTER TABLE workspace_settings ADD COLUMN default_response_loop_id INTEGER").await?;
        add_column_if_missing(db, "workspace_settings", "default_response_executor",
            "ALTER TABLE workspace_settings ADD COLUMN default_response_executor TEXT").await?;

        // V39: feishu_messages.workspace_id
        add_column_if_missing(db, "feishu_messages", "workspace_id",
            "ALTER TABLE feishu_messages ADD COLUMN workspace_id INTEGER").await?;

        // V40: 删除 feishu_messages.processed_todo_id
        drop_column_if_exists(db, "feishu_messages", "processed_todo_id").await?;

        tracing::info!("V43 (consolidated_final_features) applied");
        Ok(())
    }
}

/// V44: 补加 feishu_messages.processed_id / processed_type 列。
///
/// V1 的 CREATE TABLE IF NOT EXISTS 当前代码虽包含这两列，但老数据库可能是在
/// 这两列加入 CREATE TABLE 之前创建的，IF NOT EXISTS 不会给已有表加列。
/// V43 合并迁移（V39）补了 workspace_id，但从未添加 processed_id / processed_type，
/// 导致 get_feishu_message_stats() 查询 "processed_id IS NOT NULL" 时报
/// "no such column: processed_id" 错误，handler 返回 500。
pub(super) struct V44AddFeishuMessagesProcessedId;

#[async_trait]
impl Migration for V44AddFeishuMessagesProcessedId {
    fn version(&self) -> i64 { 44 }
    fn name(&self) -> &'static str { "add_feishu_messages_processed_id" }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        add_column_if_missing(db, "feishu_messages", "processed_id",
            "ALTER TABLE feishu_messages ADD COLUMN processed_id INTEGER").await?;
        add_column_if_missing(db, "feishu_messages", "processed_type",
            "ALTER TABLE feishu_messages ADD COLUMN processed_type TEXT").await?;
        tracing::info!("V44: feishu_messages.processed_id/processed_type columns added");
        Ok(())
    }
}

/// V45: 补加 todos.action_type 列。
///
/// action_type 用于标记 todo 的用途分类（如 "rewrite_title"、"optimize_prompt"），
/// 供前端 ActionButton 组件做 UI 展示和筛选，不影响执行逻辑。
pub(super) struct V45AddTodosActionType;

#[async_trait]
impl Migration for V45AddTodosActionType {
    fn version(&self) -> i64 { 45 }
    fn name(&self) -> &'static str { "add_todos_action_type" }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        add_column_if_missing(db, "todos", "action_type",
            "ALTER TABLE todos ADD COLUMN action_type TEXT").await?;
        tracing::info!("V45: todos.action_type column added");
        Ok(())
    }
}

/// V46: 补加 todos.action_key 列，并为 (action_type, action_key) 创建唯一索引。
///
/// action_key 与 action_type 配合，唯一标识一个 action 模板 todo。
/// 前端传 action_type + action_key，后端查找或自动创建对应的 todo。
/// 唯一索引防止并发请求重复创建模板 todo。
pub(super) struct V46AddTodosActionKey;

#[async_trait]
impl Migration for V46AddTodosActionKey {
    fn version(&self) -> i64 { 46 }
    fn name(&self) -> &'static str { "add_todos_action_key" }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        add_column_if_missing(db, "todos", "action_key",
            "ALTER TABLE todos ADD COLUMN action_key TEXT").await?;

        // 创建唯一索引：(action_type, action_key) 组合唯一
        // 忽略错误：索引已存在时会报错，属于正常情况
        let _ = db
            .conn
            .execute(Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                "CREATE UNIQUE INDEX IF NOT EXISTS idx_todos_action_type_key ON todos (action_type, action_key) WHERE action_type IS NOT NULL AND action_key IS NOT NULL".to_string(),
            ))
            .await;

        tracing::info!("V46: todos.action_key column added with unique index");
        Ok(())
    }
}
