//! 数据库迁移 V61：为 blackboards 表新增 enabled 总开关。
//!
//! ## 背景
//! 黑板功能需要一个 per-workspace 的总开关：关闭后不执行任何黑板相关逻辑
//! （防抖入队、flush 刷新、Wiki 自动维护），避免不需要黑板的工作空间浪费资源。
//!
//! ## 幂等
//! `add_column_if_missing` 已存在则静默跳过。

use async_trait::async_trait;

use super::super::Database;
use super::{Migration, add_column_if_missing};

pub(super) struct V61AddBlackboardEnabled;

#[async_trait]
impl Migration for V61AddBlackboardEnabled {
    fn version(&self) -> i64 {
        61
    }

    fn name(&self) -> &'static str {
        "V61AddBlackboardEnabled"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // enabled 列：INTEGER NOT NULL DEFAULT 1，1=启用，0=禁用。
        // 默认启用保持向后兼容：已有工作空间无需手动开启。
        add_column_if_missing(
            db,
            "blackboards",
            "enabled",
            "ALTER TABLE blackboards ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1",
        )
        .await?;
        tracing::info!("V61: blackboards.enabled 列已添加");
        Ok(())
    }
}
