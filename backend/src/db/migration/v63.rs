//! 数据库迁移 V63：为 executors 表新增 is_default 字段。
//!
//! ## 背景
//! 用户希望能在执行器菜单中设置默认执行器，而不是写死 claudecode。
//! 在 executors 表增加 is_default 布尔字段，标记哪一个是系统默认执行器。
//!
//! ## 幂等
//! `add_column_if_missing` 已存在则静默跳过。

use async_trait::async_trait;

use super::super::Database;
use super::{Migration, add_column_if_missing};

pub(super) struct V63AddExecutorIsDefault;

#[async_trait]
impl Migration for V63AddExecutorIsDefault {
    fn version(&self) -> i64 {
        63
    }

    fn name(&self) -> &'static str {
        "V63AddExecutorIsDefault"
    }

    /// 为 executors 表添加 is_default 列，默认 0（非默认）。
    /// 首次迁移时，如果表中已有 claudecode 且无任何默认执行器，将其设为默认，
    /// 保持与历史行为一致。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // is_default 列：INTEGER NOT NULL DEFAULT 0，1=默认，0=非默认。
        // 默认 0 保持向后兼容：已有执行器都不是默认，由后续种子逻辑处理。
        add_column_if_missing(
            db,
            "executors",
            "is_default",
            "ALTER TABLE executors ADD COLUMN is_default INTEGER NOT NULL DEFAULT 0",
        )
        .await?;
        tracing::info!("V63: executors.is_default 列已添加");

        // 首次迁移后，如果没有任何执行器被标记为默认，
        // 则把 claudecode 设为默认（与历史默认值一致）。
        let has_default = Self::check_has_default(db).await?;
        if !has_default {
            Self::seed_default_executor(db).await?;
        }

        Ok(())
    }
}

impl V63AddExecutorIsDefault {
    /// 检查是否已有执行器被标记为默认。
    async fn check_has_default(db: &Database) -> Result<bool, sea_orm::DbErr> {
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        let row = db
            .conn
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) FROM executors WHERE is_default = 1",
            ))
            .await?;
        Ok(row
            .and_then(|r| r.try_get_by_index::<i64>(0).ok())
            .unwrap_or(0)
            > 0)
    }

    /// 将 claudecode 设为默认执行器（保持历史行为一致）。
    /// 仅在 claudecode 存在时设置；不存在则不操作，由用户后续手动设置。
    async fn seed_default_executor(db: &Database) -> Result<(), sea_orm::DbErr> {
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        let result = db
            .conn
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "UPDATE executors SET is_default = 1 WHERE name = 'claudecode'",
            ))
            .await?;
        if result.rows_affected() > 0 {
            tracing::info!("V63: claudecode 已设为默认执行器（种子值）");
        } else {
            tracing::warn!(
                "V63: 未找到 claudecode 执行器，未设置默认值；请用户手动配置"
            );
        }
        Ok(())
    }
}
