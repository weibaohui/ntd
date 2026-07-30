//! V76 迁移：为 `loops` 增加 `abnormal_handler_prompt` 列，支撑需求 035「工艺驱动异常处理」。
//!
//! 背景：异常处理提示词此前只能挂在某个外部 Todo 的 prompt 上（用户在环路弹窗手选），
//! 与工艺定义脱节。本列存储工艺 YAML 中 `abnormal_handler.prompt` 的只读快照，
//! 让异常处理统一为工艺驱动。运行时仍读取 abnormal_handler_todo_id 指向的载体 Todo
//! （安装时按 prompt 自动创建）。
//!
//! 幂等：列已存在则跳过。未上线无用户，不做存量回填。
use super::{add_column_if_missing, Migration};
use crate::db::Database;

/// 为 `loops` 增加 `abnormal_handler_prompt`（TEXT，可空），存储工艺异常处理提示词快照。
pub(super) struct V76AddLoopAbnormalHandlerPrompt;

#[async_trait::async_trait]
impl Migration for V76AddLoopAbnormalHandlerPrompt {
    fn version(&self) -> i64 {
        // 紧随 V75，单调递增；新迁移必须严格大于已有版本
        76
    }
    fn name(&self) -> &'static str {
        "V76AddLoopAbnormalHandlerPrompt"
    }
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 幂等加列：旧库直接升级到本版本时若列已存在则跳过，避免重复 ALTER 报错
        add_column_if_missing(
            db,
            "loops",
            "abnormal_handler_prompt",
            "ALTER TABLE loops ADD COLUMN abnormal_handler_prompt TEXT",
        )
        .await
    }
}
