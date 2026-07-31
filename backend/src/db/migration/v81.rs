//! V81 迁移：删除 `loop_steps.review_type` 列（需求 048）。
//!
//! 背景：046 门禁统一后，评审/门禁语义收敛到 `gate_config`（ai_criteria_review /
//! human_approval）。`review_type` 与 gate 一一重复（review_type=ai ≈ ai_criteria_review，
//! review_type=human ≈ human_approval），loop_runner 已改由 `gate_config` 判定人工审批步骤、
//! installer 已改由 `gate_config` 推导 `auto_review_enabled`。`review_type` 成为死列，删除
//! 以免「两套配置打架」—— 留着会让用户误以为改 review_type 能影响评审，实际已被 gate 覆盖。
//!
//! 幂等：列不存在则跳过（`drop_column_if_exists` 先用 `pragma_table_info` 检查）。
//! 删除的是无外键约束的普通 TEXT 列，SQLite `DROP COLUMN` 安全。
use super::{drop_column_if_exists, Migration};
use crate::db::Database;

/// 删除 `loop_steps.review_type` 死列（评审/门禁统一由 gate_config 表达）。
pub(super) struct V81DropLoopStepReviewType;

#[async_trait::async_trait]
impl Migration for V81DropLoopStepReviewType {
    fn version(&self) -> i64 {
        // 紧随 V80，单调递增；新迁移必须严格大于已有版本
        81
    }
    fn name(&self) -> &'static str {
        "V81DropLoopStepReviewType"
    }
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 幂等删列：旧库已无该列（全新库或已升级）时跳过，避免重复 ALTER 报错
        drop_column_if_exists(db, "loop_steps", "review_type").await
    }
}
