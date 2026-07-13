//! 数据库迁移 V64：为 todos 表新增 expert_name 字段
//!
//! ## 背景
//! 支持为 Todo 配置 WorkBuddy 格式的专家/团队，执行时注入专家身份和技能。
//!
//! ## 幂等
//! `add_column_if_missing` 已存在则静默跳过。

use async_trait::async_trait;

use super::super::Database;
use super::{Migration, add_column_if_missing};

pub(super) struct V64AddTodoExpertName;

#[async_trait]
impl Migration for V64AddTodoExpertName {
    fn version(&self) -> i64 {
        64
    }

    fn name(&self) -> &'static str {
        "V64AddTodoExpertName"
    }

    /// 为 todos 表添加 expert_name 列，TEXT 类型，可为 NULL。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        add_column_if_missing(
            db,
            "todos",
            "expert_name",
            "ALTER TABLE todos ADD COLUMN expert_name TEXT",
        )
        .await?;
        tracing::info!("V64: todos.expert_name 列已添加");
        Ok(())
    }
}
