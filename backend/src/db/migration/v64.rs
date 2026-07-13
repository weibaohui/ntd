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
        // 兜底：扫码创建但未跑过 /sethome 的 bot，其 owner open_id 错位存在 bot_open_id，
        // 这里补灌，避免升级后定时推送静默丢失（否则要等 owner 私聊一次才捕获）。
        Self::backfill_owner_open_id_from_bot_open_id(db).await?;

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

    /// 兜底回填：从 agent_bots.bot_open_id 灌入 owner_open_id。
    /// bot_open_id 对扫码创建的 bot 存的是扫码人 open_id（即所有者；历史字段语义错位为「bot 自己」）。
    /// 仅灌 ou_ 开头的值，避免误灌空串或非 open_id 内容；owner_open_id 已有值则不覆盖。
    async fn backfill_owner_open_id_from_bot_open_id(
        db: &Database,
    ) -> Result<(), sea_orm::DbErr> {
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        // LIKE 'ou_%' 过滤：只有扫码人 open_id（ou_ 前缀）才灌，空串/异常值跳过
        let result = db
            .conn
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "UPDATE agent_bots
                 SET owner_open_id = bot_open_id
                 WHERE owner_open_id IS NULL
                   AND bot_open_id LIKE 'ou_%'",
            ))
            .await?;
        if result.rows_affected() > 0 {
            tracing::info!(
                "V64: 已把 {} 个 bot 的 bot_open_id 兜底迁移到 owner_open_id",
                result.rows_affected()
            );
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod v64_tests {
    use super::*;
    use crate::db::Database;
    use sea_orm::{ConnectionTrait, DbBackend, Statement};

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect(":memory: db must open")
    }

    #[tokio::test]
    async fn test_backfill_p2p_first_then_bot_open_id() {
        // 双源 backfill：p2p_receive_id 优先，bot_open_id 兜底
        let db = fresh_db().await;
        // bot A：扫码(bot_open_id) + 跑过 /sethome(p2p_receive_id)
        let a = db.create_agent_bot("feishu", "a", "app", "secret", Some("ou_scan_a".to_string()), None, 1).await.unwrap();
        // bot B：只扫码(bot_open_id)，没 /sethome
        let b = db.create_agent_bot("feishu", "b", "app2", "secret", Some("ou_scan_b".to_string()), None, 1).await.unwrap();
        // create_agent_bot 新逻辑会把 bot_open_id 同步写进 owner_open_id，清空以模拟升级前
        db.conn.execute(Statement::from_string(DbBackend::Sqlite, "UPDATE agent_bots SET owner_open_id = NULL")).await.unwrap();
        // 给 bot A 造 p2p_receive_id（update_feishu_push_level 建行 + 补字段）
        db.update_feishu_push_level(a, "result_only").await.unwrap();
        db.conn.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE feishu_push_targets SET p2p_receive_id = 'ou_sethome_a' WHERE bot_id = ?",
            [a.into()],
        )).await.unwrap();

        V64AddAgentBotOwnerOpenId::backfill_owner_open_id_from_push_targets(&db).await.unwrap();
        V64AddAgentBotOwnerOpenId::backfill_owner_open_id_from_bot_open_id(&db).await.unwrap();

        // A：p2p_receive_id 优先灌入，兜底不再覆盖
        assert_eq!(db.get_owner_open_id(a).await.unwrap(), Some("ou_sethome_a".to_string()));
        // B：无 p2p_receive_id，兜底从 bot_open_id 灌入
        assert_eq!(db.get_owner_open_id(b).await.unwrap(), Some("ou_scan_b".to_string()));
    }
}
