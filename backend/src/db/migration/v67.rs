//! 数据库迁移 V67：为 execution_records 表新增 agent_runs 字段
//!
//! ## 背景
//! 多 Agent 协作（如 Claude Code 的 Task 工具 spawn 子 agent、codewhale 的 agent 工具、
//! mimo 族的 task/actor 工具）目前没有结构化记录，前端无法展示"派生了哪些子 agent"。
//! 新增 agent_runs 列，与 todo_progress 平行：
//!   - 执行完成时一次性扫描 execution_logs，提取每个子 agent 的「元数据」
//!     （名称/角色/状态/启动时间），序列化为 JSON 存入本列；
//!   - **不存输入输出原文**——prompt 与 result 的完整文本已在 execution_logs 里，
//!     前端按 tool_name 扫日志按需展示，避免重复存储撑爆字段（pi 单条数百 KB）。
//!
//! ## 幂等
//! `add_column_if_missing` 探测列存在性，缺则 ALTER ADD，已存在则静默跳过，任意中间状态可重入。

use async_trait::async_trait;

use super::super::Database;
use super::{Migration, add_column_if_missing};

pub(super) struct V67AddExecutionRecordsAgentRuns;

#[async_trait]
impl Migration for V67AddExecutionRecordsAgentRuns {
    fn version(&self) -> i64 {
        67
    }

    fn name(&self) -> &'static str {
        "V67AddExecutionRecordsAgentRuns"
    }

    /// 为 execution_records 添加 agent_runs 列，TEXT 存 JSON 字符串（Vec<AgentRun> 序列化）。
    /// 与 todo_progress / execution_stats 同为 TEXT，保持现有 JSON-as-TEXT 约定（SQLite 无 JSONB）。
    /// NULL = 该记录未涉及子 agent，或尚未跑到完成态扫描。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        add_column_if_missing(
            db,
            "execution_records",
            "agent_runs",
            "ALTER TABLE execution_records ADD COLUMN agent_runs TEXT",
        )
        .await?;
        tracing::info!("V67: execution_records.agent_runs 列已添加");
        Ok(())
    }
}
