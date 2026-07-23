//! 数据库迁移 V70：workspace_settings 增加 system_prompt 列
//!
//! ## 背景
//! 需求 022「工作空间 Prompt」要求每个 workspace 拥有一份共享前置 prompt，
//! 内容包括产物目录、认证信息、基本文件路径等共识信息。
//! 该 workspace 下所有 todo 执行时，适配层把这段 prompt 拼到 message 最前面，
//! 达成 workspace 维度的共享、遵守、共识。
//!
//! ## 设计取舍
//! 复用既有 `workspace_settings` 表新增一列，而非新建 `workspace_prompts` 表：
//! - workspace 与 prompt 是 1:1 关系，独立表增加 JOIN 成本无收益
//! - upsert_workspace_settings 已有写入路径，扩列即用
//!
//! ## 幂等
//! `add_column_if_missing` 通过 `PRAGMA table_info` 判断列是否存在，
//! 重复执行不会报错，存量数据保持 NULL（读取时视为无 prompt，跳过拼接）。

use async_trait::async_trait;

use super::super::Database;
use super::{add_column_if_missing, Migration};

/// V70：给 workspace_settings 表追加 system_prompt 列。
///
/// 列类型 TEXT，默认 NULL；存储 workspace 级共识 prompt 的自由文本。
pub(super) struct V70AddWorkspaceSettingsSystemPrompt;

#[async_trait]
impl Migration for V70AddWorkspaceSettingsSystemPrompt {
    /// 严格递增的版本号，紧接 V69。
    fn version(&self) -> i64 {
        70
    }

    /// 日志与 schema_version.name 列使用的简短名字。
    fn name(&self) -> &'static str {
        "V70AddWorkspaceSettingsSystemPrompt"
    }

    /// 幂等追加 system_prompt 列；存在则跳过。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        add_column_if_missing(
            db,
            "workspace_settings",
            "system_prompt",
            "ALTER TABLE workspace_settings ADD COLUMN system_prompt TEXT",
        )
        .await?;
        tracing::info!("V70: workspace_settings.system_prompt 列已添加");
        Ok(())
    }
}
