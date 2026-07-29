//! V77 迁移：删除 `loop_phases.acceptance_criteria` 死列，支撑需求 036「验收标准归环节」。
//!
//! 背景：阶段级验收标准在后端运行时从未被消费——评审逐环节进行，依据来自环节的
//! `acceptance_criteria`（安装时灌进 todo，由 `compose_review_prompt` 读取）。
//! `loop_phases.acceptance_criteria` 仅在 installer 写入、全代码库无任何读取，是死列。
//! 需求 036 将验收标准收敛到只归环节，本迁移删除该死列。
//!
//! 幂等：列不存在则跳过（`drop_column_if_exists` 先用 `pragma_table_info` 检查）。
//! 删除的是无约束的普通 TEXT 列，SQLite `DROP COLUMN` 安全。
use super::{drop_column_if_exists, Migration};
use crate::db::Database;

/// 删除 `loop_phases.acceptance_criteria` 死列（验收标准只归环节）。
pub(super) struct V77DropLoopPhaseAcceptanceCriteria;

#[async_trait::async_trait]
impl Migration for V77DropLoopPhaseAcceptanceCriteria {
    fn version(&self) -> i64 {
        // 紧随 V76，单调递增；新迁移必须严格大于已有版本
        77
    }
    fn name(&self) -> &'static str {
        "V77DropLoopPhaseAcceptanceCriteria"
    }
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 幂等删列：旧库已无该列（如全新库或已升级）时跳过，避免重复 ALTER 报错
        drop_column_if_exists(db, "loop_phases", "acceptance_criteria").await
    }
}
