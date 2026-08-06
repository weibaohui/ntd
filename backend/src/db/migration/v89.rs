//! 迁移 v89：execution_records 新增 workspace_id 列。
//!
//! 背景（BUG：讨论区执行明细完成后 404）：execution record 的 workspace 归属原本经
//! carrier todo 间接关联（`verify_execution_belongs_to_ws` → `record.todo_id` →
//! `todo.workspace_id`）。060 讨论区在执行完成时软删 carrier todo，软删后 `get_todo`
//! 过滤 `deleted_at IS NULL` 返回 None，导致按 recordId 查执行明细时归属校验 NotFound
//! （执行中能打开、完成后 404）。
//!
//! 修复：给 execution_records 加 workspace_id 列，record 直接归属 workspace；归属校验
//! 改用 record 自身字段，与 todo 是否被软删彻底解耦。
//!
//! 回填：历史 record 无 workspace_id，用关联 todo 的 workspace_id 补齐（todos 为软删，
//! 行仍在，SELECT 能查到）。
//!
//! 幂等：`add_column_if_missing` 探测列存在性；回填 UPDATE 带 `WHERE workspace_id IS NULL`，
//! 任意中间状态可重入。

use crate::db::{Database, migration::Migration};
use crate::db::migration::add_column_if_missing;
use async_trait::async_trait;
use tracing::info;

/// v89：execution_records 新增 workspace_id，record 直接归属 workspace。
pub struct V89AddExecutionRecordsWorkspaceId;

#[async_trait]
impl Migration for V89AddExecutionRecordsWorkspaceId {
    fn version(&self) -> i64 {
        89
    }

    fn name(&self) -> &'static str {
        // 与 v67（同表加列先例 V67AddExecutionRecordsAgentRuns）一致：struct 名即迁移名
        "V89AddExecutionRecordsWorkspaceId"
    }

    /// 加列 + 回填历史数据。
    ///
    /// 回填子查询 `SELECT workspace_id FROM todos WHERE id = execution_records.todo_id`：
    /// todos 软删行仍在能查到；todo 不存在或 todo.workspace_id 为 NULL 时返回 NULL，
    /// 该 record 保持 NULL（归属校验 Forbidden，与未归属数据现有契约一致）。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 加列：record 直接存 workspace_id，消除经 todo 的间接归属。
        add_column_if_missing(
            db,
            "execution_records",
            "workspace_id",
            "ALTER TABLE execution_records ADD COLUMN workspace_id INTEGER",
        )
        .await?;
        // 回填历史 record：从关联 todo 的 workspace_id 补齐。幂等（只填 IS NULL 的）。
        db.exec(
            "UPDATE execution_records \
             SET workspace_id = (SELECT workspace_id FROM todos WHERE id = execution_records.todo_id) \
             WHERE workspace_id IS NULL",
        )
        .await?;
        info!("v89: execution_records.workspace_id 列已添加并回填历史数据");
        Ok(())
    }
}
