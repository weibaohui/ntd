//! V75 迁移：为 `loop_steps` 增加 `review_prompt` 列，支撑需求 033「环节评审模板」。
//!
//! 背景：环节级评审模板此前只能复用环路级/全局默认模板（`review_templates` 表），
//! 无法在工艺 YAML 中按环节定制。本列存储环节内联的完整评审模板正文（含占位符），
//! 评审时优先使用，空则回退现有机制。幂等：列已存在则跳过。
use super::{add_column_if_missing, Migration};
use crate::db::Database;

/// 为 `loop_steps` 增加 `review_prompt`（TEXT，可空），存储环节级内联评审模板正文。
pub(super) struct V75AddLoopStepReviewPrompt;

#[async_trait::async_trait]
impl Migration for V75AddLoopStepReviewPrompt {
    fn version(&self) -> i64 {
        // 紧随 V74，单调递增；新迁移必须严格大于已有版本
        75
    }
    fn name(&self) -> &'static str {
        "V75AddLoopStepReviewPrompt"
    }
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 幂等加列：旧库直接升级到本版本时若列已存在则跳过，避免重复 ALTER 报错
        add_column_if_missing(
            db,
            "loop_steps",
            "review_prompt",
            "ALTER TABLE loop_steps ADD COLUMN review_prompt TEXT",
        )
        .await
    }
}
