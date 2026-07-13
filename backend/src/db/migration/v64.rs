//! 数据库迁移 V64：为 agent_bots 表新增 owner_open_id 字段，并做存量数据迁移。
//!
//! ## 背景
//! 推送目标机制简化：废弃「设为推送目标」(/sethome) 手动填 ID 的做法，
//! 改为在 agent_bots 上用 owner_open_id 记录「所有者（默认接收人）的 open_id」，
//! 扫码创建时写入、非扫码 bot 由首次私聊兜底写入。
//! owner_open_id 取代 feishu_push_targets.p2p_receive_id 成为推送目标的权威来源，
//! 让 receive_id_type 二选一开关的 bug 从模型层面消失。
//!
//! ## 存量迁移
//! 已有 bot 的推送目标存在 feishu_push_targets.p2p_receive_id（由 /sethome 写入），
//! 首次升级时把非空的 p2p_receive_id 一次性灌入 owner_open_id，避免存量推送中断。
//!
//! ## 幂等
//! `add_column_if_missing` 已存在则静默跳过；存量迁移用 `owner_open_id IS NULL` 守卫，
//! 重复执行不会覆盖已填值。

use async_trait::async_trait;

use super::super::Database;
use super::{Migration, add_column_if_missing};

pub(super) struct V64AddAgentBotOwnerOpenId;

#[async_trait]
impl Migration for V64AddAgentBotOwnerOpenId {
    fn version(&self) -> i64 {
        64
    }

    fn name(&self) -> &'static str {
        "V64AddAgentBotOwnerOpenId"
    }

    /// 1) 给 agent_bots 加 owner_open_id 列；2) 把存量 p2p_receive_id 灌进去。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // owner_open_id 列：可空，新建 bot 在写入前为 NULL。
        // 用 add_column_if_missing 保证旧库重复升级幂等。
        add_column_if_missing(
            db,
            "agent_bots",
            "owner_open_id",
            "ALTER TABLE agent_bots ADD COLUMN owner_open_id TEXT",
        )
        .await?;
        tracing::info!("V64: agent_bots.owner_open_id 列已添加");

        // 存量迁移：把 feishu_push_targets.p2p_receive_id 灌入 owner_open_id。
        // 仅灌 owner_open_id 仍为空、且对应 p2p_receive_id 非空的 bot，
        // 避免覆盖已通过扫码/首次私聊写入的值。
        Self::backfill_owner_open_id_from_push_targets(db).await?;

        Ok(())
    }
}

impl V64AddAgentBotOwnerOpenId {
    /// 把 feishu_push_targets.p2p_receive_id 一次性灌入 agent_bots.owner_open_id。
    /// 用相关子查询定位每个 bot 的 p2p_receive_id，EXISTS 守卫避免写成空串。
    async fn backfill_owner_open_id_from_push_targets(
        db: &Database,
    ) -> Result<(), sea_orm::DbErr> {
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        // p2p_receive_id 在 v1 建表时为 NOT NULL DEFAULT ''，故用 != '' 过滤掉未配置的 bot。
        // 无绑定参数，直接用 from_string（与 v63 seed 写法一致）。
        let result = db
            .conn
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "UPDATE agent_bots
                 SET owner_open_id = (
                     SELECT p2p_receive_id FROM feishu_push_targets
                     WHERE bot_id = agent_bots.id AND p2p_receive_id != ''
                 )
                 WHERE owner_open_id IS NULL
                   AND EXISTS (
                     SELECT 1 FROM feishu_push_targets
                     WHERE bot_id = agent_bots.id AND p2p_receive_id != ''
                   )",
            ))
            .await?;
        if result.rows_affected() > 0 {
            tracing::info!(
                "V64: 已把 {} 个 bot 的 p2p_receive_id 迁移到 owner_open_id",
                result.rows_affected()
            );
        } else {
            tracing::info!("V64: 无需迁移的存量推送目标");
        }
        Ok(())
    }
}
