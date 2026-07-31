//! V82 迁移：删除废弃的 `process_step_templates` 表（需求 052）。
//!
//! 背景：step_template 原型机制已废弃——安装工艺时执行配置全部内联在工艺 YAML 的 link 里，
//! 不再按 name 查原型表（见 `installer.rs::resolve_link_fields`）。该表只剩两类弱用途：
//! 1. bundled 同步时把 step-templates yaml「先删后插」的本地缓存（8 行系统数据）；
//! 2. `check_skill_warnings` 按 name 查它、查不到只 warn 且查的是错表。
//!
//! 核心运行链路（loops / todos / loop_steps）不依赖它，随表清理所有读写点。
//!
//! 幂等：`DROP TABLE IF EXISTS` 原生幂等；SQLite 会一并删除该表的索引与触发器。
use crate::db::Database;
use super::Migration;

/// 删除 `process_step_templates` 死表。
pub(super) struct V82DropProcessStepTemplates;

#[async_trait::async_trait]
impl Migration for V82DropProcessStepTemplates {
    fn version(&self) -> i64 {
        // 紧随 V81，单调递增；新迁移必须严格大于已有版本
        82
    }

    fn name(&self) -> &'static str {
        "V82DropProcessStepTemplates"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // DROP TABLE IF EXISTS 幂等；索引与触发器随表一并删除，无需单独清理。
        db.exec("DROP TABLE IF EXISTS process_step_templates").await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::super::table_exists;
    use super::*;

    async fn fresh_db() -> Database {
        // Database::new 会先跑完全量迁移链；fresh 库上 v82 已把表删掉。
        Database::new(":memory:")
            .await
            .expect(":memory: db must open")
    }

    #[tokio::test]
    async fn v82_drops_process_step_templates() {
        let db = fresh_db().await;
        // 显式重建表，模拟「旧库尚未升级」的状态，再应用 v82 验证删表语义。
        db.exec("CREATE TABLE IF NOT EXISTS process_step_templates (id INTEGER PRIMARY KEY, name TEXT)")
            .await
            .unwrap();
        assert!(
            table_exists(&db, "process_step_templates").await.unwrap(),
            "重建后表应存在"
        );

        V82DropProcessStepTemplates.up(&db).await.unwrap();

        assert!(
            !table_exists(&db, "process_step_templates").await.unwrap(),
            "v82 应用后表必须不存在"
        );
    }

    #[tokio::test]
    async fn v82_is_idempotent() {
        let db = fresh_db().await;
        // 表已不存在（fresh 库）时重复应用不得报错。
        V82DropProcessStepTemplates.up(&db).await.unwrap();
        V82DropProcessStepTemplates.up(&db).await.unwrap();
    }
}
