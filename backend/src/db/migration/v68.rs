//! 数据库迁移 V68：为 executors 表新增 default_model，为 todos 表新增 model
//!
//! ## 背景
//! 当前 ntd 执行链路完全不参与模型选择——执行器二进制读自己的配置文件决定模型，
//! ntd 只在执行结束后从日志反解模型名做统计。本次新增两列，把「模型」提升为
//! 可在执行器级 / todo 级指定的执行参数：
//!   - `executors.default_model`：执行器级默认模型（「这个执行器平时用什么模型」）；
//!   - `todos.model`：任务级覆盖（「这条 todo 临时指定模型」，优先于执行器默认）。
//!
//! 执行时按三层优先级解析：`todo.model` > `executor.default_model` > 不传 `--model`
//! （最后一级保证向后兼容——未配置时行为与升级前完全一致）。
//!
//! ## 幂等
//! `add_column_if_missing` 先探测列是否存在，缺则 ALTER ADD，已存在则静默跳过，
//! 任意中间状态可重入。

use async_trait::async_trait;

use super::super::Database;
use super::{Migration, add_column_if_missing};

pub(super) struct V68AddModelColumns;

#[async_trait]
impl Migration for V68AddModelColumns {
    fn version(&self) -> i64 {
        68
    }

    fn name(&self) -> &'static str {
        "V68AddModelColumns"
    }

    /// 两列均为 TEXT、可空：NULL = 未指定模型，执行时不传 `--model`，由执行器配置文件决定。
    /// 与 executor / todo 已有的可空 TEXT 字段（session_dir、expert_name 等）保持同一约定。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 执行器级默认模型：所有未单独指定模型的 todo，使用该执行器时默认传此模型。
        add_column_if_missing(
            db,
            "executors",
            "default_model",
            "ALTER TABLE executors ADD COLUMN default_model TEXT",
        )
        .await?;
        // 任务级模型覆盖：优先级高于 executor.default_model。
        add_column_if_missing(
            db,
            "todos",
            "model",
            "ALTER TABLE todos ADD COLUMN model TEXT",
        )
        .await?;
        tracing::info!("V68: executors.default_model / todos.model 列已添加");
        Ok(())
    }
}
