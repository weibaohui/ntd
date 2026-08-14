//! 数据库迁移 V94：把「项目目录」实体重命名为「工作空间」。
//!
//! 背景：工作空间功能从「项目目录」演化而来（见 docs/design/104-工作空间命名统一重构-设计.md），
//! 表名 project_directories 与现行 workspace 命名并存。本迁移把表/索引/触发器/列名统一为 workspace。
//! 注意：V94 之前的旧迁移（v1..v93）引用的是当时的表名 project_directories，属不可变历史，
//! 本迁移在它们之后执行，SQL 里必须使用旧表名做 RENAME 的源表名。

use async_trait::async_trait;

use super::super::Database;
use super::Migration;

pub(super) struct V94RenameProjectDirectoriesToWorkspaces;

#[async_trait]
impl Migration for V94RenameProjectDirectoriesToWorkspaces {
    fn version(&self) -> i64 {
        94
    }
    fn name(&self) -> &'static str {
        "rename_project_directories_to_workspaces"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 先删旧触发器与旧名索引：SQLite 没有 RENAME INDEX / RENAME TRIGGER，
        // 必须显式 drop 后以新名重建，否则库里会残留带 project_directories 名字的对象。
        db.exec("DROP TRIGGER IF EXISTS set_project_directories_created_at_utc")
            .await?;
        db.exec("DROP INDEX IF EXISTS idx_project_directories_path").await?;

        // 表改名。SQLite ≥3.25 会自动把其它表 CREATE 语句里
        // REFERENCES project_directories(id) 改写为 REFERENCES workspaces(id)，
        // 无需手工重建外键表（漂移测试会校验这一点）。
        db.exec("ALTER TABLE project_directories RENAME TO workspaces")
            .await?;

        // 以新名重建 path 唯一索引与 created_at UTC 触发器，保持旧库既有行为不变。
        db.exec("CREATE INDEX IF NOT EXISTS idx_workspaces_path ON workspaces(path)")
            .await?;
        db.exec(
            "CREATE TRIGGER IF NOT EXISTS set_workspaces_created_at_utc AFTER INSERT ON workspaces
             BEGIN
                 UPDATE workspaces SET created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', 'utc') WHERE rowid = new.rowid;
             END",
        )
        .await?;

        // 飞书绑定表的 project_dir_id 列名一并统一为 workspace_id（API 可见字段），
        // 与系统内其它 workspace_id 列（agent_bots/todos/blackboards 等）对齐。
        db.exec("ALTER TABLE feishu_project_bindings RENAME COLUMN project_dir_id TO workspace_id")
            .await?;
        Ok(())
    }
}
