//! V78 迁移：删除 `process_templates.definition` 列，支撑需求 038「工艺定义改为文件读取」。
//!
//! 背景：此前 `process_templates` 把工艺 YAML 正文（大 TEXT）冗余存进 DB，作为磁盘文件的镜像。
//! 仓库事实上的「唯一真源」是磁盘 YAML（`~/.ntd/processes/**` 用户层 + `~/.ntd/bundled/processes/**` 系统层），
//! DB 从不提供编辑入口，导入时只按 `name` 做 upsert，不保留历史版本，因此 DB 副本既非版本、也非缓存的权威。
//! 需求 038 改为「磁盘唯一真源」：DB 只保留 `source_path` 路径引用与小字段元数据，
//! 正文始终按 `source_path` 从磁盘读取，删掉 `definition` 列消除冗余与双写漂移风险。
//!
//! 保留：不删表。`process_templates.id` 仍是 `loops.process_template_id` 的外键锚点，
//! `name`/`display_name`/`description`/`category`/`complexity`/`version` 等轻量元数据继续留在 DB 供查询缓存。
//!
//! 幂等：列不存在则跳过（`drop_column_if_exists` 先用 `pragma_table_info` 检查）。
//! 删除的是无约束的普通 TEXT 列，SQLite `DROP COLUMN` 安全。
use super::{drop_column_if_exists, Migration};
use crate::db::Database;

/// 删除 `process_templates.definition` 列（正文改由 source_path 指定的磁盘文件读取）。
pub(super) struct V78ProcessDefinitionToFile;

#[async_trait::async_trait]
impl Migration for V78ProcessDefinitionToFile {
    fn version(&self) -> i64 {
        // 紧随 V77，单调递增；新迁移必须严格大于已有版本
        78
    }
    fn name(&self) -> &'static str {
        "V78ProcessDefinitionToFile"
    }
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 幂等删列：旧库已无该列（如全新库或已升级）时跳过，避免重复 ALTER 报错
        drop_column_if_exists(db, "process_templates", "definition").await
    }
}
