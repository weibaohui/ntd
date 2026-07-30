//! V79 迁移：`process_templates` 引入 `guid` 身份列，支撑需求 040「工艺模板 GUID 身份」。
//!
//! 背景：此前 `name` 是表内唯一键兼全局寻址键，导致两个结构性问题——
//! ① 复制系统模板只能同名覆盖（用户层 upsert 顶掉系统行，原模板消失）；
//! ② 同步"先删后插"使自增 id 每次重排，`loops.process_template_id`（ON DELETE SET NULL）被清空。
//! 040 把身份挪到 YAML 文件的 `process.guid` 字段（UUID v4，随文件走），DB 只存索引。
//!
//! 本迁移做的事：
//! 1. 显式解除 loops → 工艺模板的外键关联（老 id 体系作废，语义清晰，不依赖 DROP 触发 FK 行为）。
//! 2. 重建 `process_templates`：`guid TEXT NOT NULL` + 唯一索引；`name` 去掉 UNIQUE（保留普通索引）。
//!    SQLite 无法删除内联 UNIQUE 约束，只能 DROP + CREATE。
//! 3. 老数据行不迁移（用户拍板删老数据）：系统模板由下次 bundled 同步重新入库，
//!    用户层磁盘 YAML 保留，重扫时自动生成 guid 回写后重新入库。
//!
//! 幂等：新表已有 guid 列（如迁移成功后重放）时整体跳过。
use super::{table_has_column, Migration};
use crate::db::Database;

/// 重建 `process_templates`：guid 唯一身份，name 放开唯一约束，清空老行。
pub(super) struct V79ProcessTemplateGuid;

#[async_trait::async_trait]
impl Migration for V79ProcessTemplateGuid {
    fn version(&self) -> i64 {
        // 紧随 V78，单调递增；新迁移必须严格大于已有版本
        79
    }
    fn name(&self) -> &'static str {
        "V79ProcessTemplateGuid"
    }
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 幂等守卫：已有 guid 列说明迁移已执行过（或全新库直接建成新 schema），跳过重建。
        if table_has_column(db, "process_templates", "guid").await? {
            return Ok(());
        }

        // 老 id 体系作废：先显式解关联，避免 DROP 表时依赖 SQLite 对 ON DELETE 的隐式触发行为。
        db.exec("UPDATE loops SET process_template_id = NULL WHERE process_template_id IS NOT NULL")
            .await?;

        // SQLite 无法删除 v71 建表时内联的 name UNIQUE 约束，只能 DROP + CREATE 重建。
        // 表上的索引与触发器随 DROP 一并消失，重建后按新 schema 恢复。
        db.exec("DROP TABLE process_templates").await?;
        db.exec(
            "CREATE TABLE process_templates (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                guid TEXT NOT NULL,
                name TEXT NOT NULL,
                display_name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                category TEXT NOT NULL DEFAULT '',
                complexity TEXT NOT NULL DEFAULT 'standard',
                version TEXT NOT NULL DEFAULT '1.0.0',
                source_path TEXT,
                workspace_id INTEGER,
                is_system INTEGER NOT NULL DEFAULT 0,
                previous_version_id INTEGER,
                created_at TEXT,
                updated_at TEXT
            )",
        )
        .await?;

        // guid 是全局唯一身份；name 降级为普通索引（允许同名共存，如模板与它的用户副本）。
        db.exec("CREATE UNIQUE INDEX uk_process_templates_guid ON process_templates(guid)").await?;
        db.exec("CREATE INDEX idx_process_templates_name ON process_templates(name)").await?;
        db.exec("CREATE INDEX idx_process_templates_category ON process_templates(category)").await?;
        db.exec("CREATE INDEX idx_process_templates_workspace ON process_templates(workspace_id)").await?;

        // 恢复 v71 的时间戳触发器（重建表后丢失）。
        db.exec(
            "CREATE TRIGGER set_process_templates_created_at_utc AFTER INSERT ON process_templates
             WHEN new.created_at IS NULL OR new.created_at = ''
             BEGIN UPDATE process_templates SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid; END",
        )
        .await?;
        db.exec(
            "CREATE TRIGGER set_process_templates_updated_at_utc BEFORE UPDATE ON process_templates
             WHEN new.updated_at IS NULL OR new.updated_at = ''
             BEGIN UPDATE process_templates SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid; END",
        )
        .await?;
        Ok(())
    }
}
