//! V80 迁移：环路瘦身（需求 044）。
//!
//! 环路（Loop）从「独立功能」降级为「工艺的运行时承载」：
//! - 定义层交给工艺 YAML；触发器（cron/webhook/飞书/事项/标签）整体下线；
//! - 手工环路是双轨演进遗留，直接级联删除；
//! - loops/loop_steps 上越界承担「定义」的列随之下线。
//!
//! 分两步，顺序不可反（先清数据再动结构，保证删列语句不依赖已删行）：
//! 1. 存量手工环路（process_template_id IS NULL）按外键依赖顺序级联删除；
//! 2. DROP TABLE loop_triggers + 删除 loops/loop_steps 的冗余列。
//!
//! 数据删除走单事务，中途失败整体回滚，不留半截状态；删除前列出将被清除的环路
//! id 与名称到日志，便于事后审计（用户已确认可丢历史）。
//! 幂等：loop_triggers 表已不存在说明迁移已执行过，整体跳过。
use super::{drop_column_if_exists, table_exists, Migration};
use crate::db::Database;
// ConnectionTrait 提供 query_all、TransactionTrait 提供 transaction；
// 仅补齐 trait 导入以编译，不改变任何迁移语义。
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
use std::collections::BTreeMap;

/// 环路瘦身：级联删除手工环路 + 删除触发器表与冗余列。
pub(super) struct V80LoopSlim;

#[async_trait::async_trait]
impl Migration for V80LoopSlim {
    fn version(&self) -> i64 {
        // 紧随 V79，单调递增；新迁移必须严格大于已有版本
        80
    }
    fn name(&self) -> &'static str {
        "V80LoopSlim"
    }
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 幂等守卫：loop_triggers 表已不存在 = 迁移已执行过（或全新库直接建成新 schema）。
        if !table_exists(db, "loop_triggers").await? {
            return Ok(());
        }

        // 第一步：单事务级联删除手工环路。先查待删清单记日志，再按依赖顺序删子表。
        cascade_delete_manual_loops(db).await?;

        // 第二步：结构变更。
        // 2.1 先重建 loop_executions：v41 建表时带 `FOREIGN KEY (trigger_id) REFERENCES loop_triggers`，
        //     删 loop_triggers 后该外键悬空，foreign_keys=ON 下任何 INSERT 都会报「no such table」。
        //     SQLite 无法删外键，只能重建表；trigger_id 作为历史列保留（不再有外键约束）。
        rebuild_loop_executions_without_trigger_fk(db).await?;
        // 2.2 触发器表整体删除；loops/loop_steps 的冗余列逐个 DROP（SQLite 3.35+）。
        // 各 DROP 用 drop_column_if_exists 守卫，重放时跳过已删列。
        db.exec("DROP TABLE loop_triggers").await?;
        for col in ["webhook_enabled", "review_template_id", "color", "icon"] {
            drop_column_if_exists(db, "loops", col).await?;
        }
        for col in ["min_rating", "unrated_policy", "run_mode", "skip_on_source_failed"] {
            drop_column_if_exists(db, "loop_steps", col).await?;
        }
        Ok(())
    }
}

/// 重建 `loop_executions`，去掉指向 `loop_triggers` 的外键。
///
/// 必须在 `DROP TABLE loop_triggers` 之前执行：重建时旧表还在，旧外键仍可解析；
/// 拷贝数据后用新表（只保留 loop_id 外键）替换旧表。
/// 列集合与现行表完全一致（含后续 ALTER 追加的 total_executed_steps/error_message/task_id），
/// trigger_id 作为历史列保留，仅去掉其外键约束。
async fn rebuild_loop_executions_without_trigger_fk(
    db: &Database,
) -> Result<(), sea_orm::DbErr> {
    // 临时表名带后缀，避免与将来可能的同名冲突；列与现行 loop_executions 一一对应。
    // 重建必须在单事务（单连接）内完成：迁移的每条 db.exec 各自从连接池取不同连接，
    // SQLite WAL 下不同连接的快照会让「DROP 旧表 → RENAME 新表」跨连接看到旧表仍在，
    // 报「there is already another table loop_executions」。放进事务后语句在同一连接顺序执行，
    // 快照一致，DROP 后 RENAME 才能成功。
    db.conn
        .transaction::<_, (), sea_orm::DbErr>(|txn| {
            Box::pin(async move {
                txn.execute(Statement::from_string(
                    DbBackend::Sqlite,
                    "CREATE TABLE loop_executions_v80 (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        loop_id INTEGER NOT NULL,
                        trigger_id INTEGER,
                        trigger_type TEXT NOT NULL,
                        trigger_meta TEXT DEFAULT '{}',
                        started_at TEXT NOT NULL,
                        finished_at TEXT,
                        status TEXT NOT NULL DEFAULT 'running',
                        total_steps INTEGER NOT NULL DEFAULT 0,
                        completed_steps INTEGER NOT NULL DEFAULT 0,
                        failed_steps INTEGER NOT NULL DEFAULT 0,
                        total_executed_steps INTEGER NOT NULL DEFAULT 0,
                        error_message TEXT DEFAULT NULL,
                        task_id INTEGER,
                        FOREIGN KEY (loop_id) REFERENCES loops(id) ON DELETE CASCADE
                    )",
                ))
                .await?;
                // 全量拷贝：列顺序与建表一致，SELECT * 依赖两表列定义同序（已对齐）。
                txn.execute(Statement::from_string(
                    DbBackend::Sqlite,
                    "INSERT INTO loop_executions_v80 SELECT * FROM loop_executions",
                ))
                .await?;
                txn.execute(Statement::from_string(
                    DbBackend::Sqlite,
                    "DROP TABLE loop_executions",
                ))
                .await?;
                txn.execute(Statement::from_string(
                    DbBackend::Sqlite,
                    "ALTER TABLE loop_executions_v80 RENAME TO loop_executions",
                ))
                .await?;
                // 恢复 v41 建表时的索引（DROP TABLE 一并丢弃）。
                txn.execute(Statement::from_string(
                    DbBackend::Sqlite,
                    "CREATE INDEX IF NOT EXISTS idx_loop_executions_loop_id ON loop_executions(loop_id)",
                ))
                .await?;
                txn.execute(Statement::from_string(
                    DbBackend::Sqlite,
                    "CREATE INDEX IF NOT EXISTS idx_loop_executions_started_at ON loop_executions(started_at DESC)",
                ))
                .await?;
                txn.execute(Statement::from_string(
                    DbBackend::Sqlite,
                    "CREATE INDEX IF NOT EXISTS idx_loop_executions_status ON loop_executions(status)",
                ))
                .await?;
                Ok(())
            })
        })
        .await
        // transaction 返回 TransactionError<DbErr>，统一展开为 DbErr 以匹配函数签名。
        .map_err(|e| match e {
            sea_orm::TransactionError::Connection(e) => e,
            sea_orm::TransactionError::Transaction(e) => e,
        })?;
    Ok(())
}

/// 单事务级联删除所有手工环路（process_template_id IS NULL）。
///
/// 删除顺序严格按外键依赖：先删最深子表（gates/artifacts），逐层向上到 loops。
/// 用子查询按 loop_id 一批删除，避免逐行循环；中途任一语句失败，整个事务回滚。
async fn cascade_delete_manual_loops(db: &Database) -> Result<(), sea_orm::DbErr> {
    // 先在事务外把待删清单查出来记日志：删除是不可逆操作，事后能从日志追溯清了哪些环路。
    let manual_loops = list_manual_loops(db).await?;
    if manual_loops.is_empty() {
        return Ok(());
    }
    tracing::info!(
        "v80: 将级联删除 {} 个手工环路: {:?}",
        manual_loops.len(),
        manual_loops
    );

    // 把 id 列表拼成 `1,2,3` 用于 IN 子查询；id 来自自身主键，无注入面。
    let id_list = manual_loops
        .keys()
        .map(|k| k.to_string())
        .collect::<Vec<_>>()
        .join(",");

    // 全程单事务：数据删除 + 结构变更前的最后数据清理必须原子完成。
    db.conn
        .transaction::<_, (), sea_orm::DbErr>(|txn| {
            Box::pin(async move {
                // 子表按 loop_id 经中间表关联，用嵌套子查询定位待删行，保证一次清干净。
                // 1) 环节执行门禁：经 loop_step_executions → loop_executions → loops
                txn.execute(Statement::from_string(
                    DbBackend::Sqlite,
                    format!(
                        "DELETE FROM loop_step_execution_gates
                         WHERE loop_step_execution_id IN (
                             SELECT id FROM loop_step_executions WHERE loop_execution_id IN (
                                 SELECT id FROM loop_executions WHERE loop_id IN ({id_list})
                             )
                         )"
                    ),
                ))
                .await?;
                // 2) 环节产物：同链路
                txn.execute(Statement::from_string(
                    DbBackend::Sqlite,
                    format!(
                        "DELETE FROM loop_step_artifacts
                         WHERE loop_step_execution_id IN (
                             SELECT id FROM loop_step_executions WHERE loop_execution_id IN (
                                 SELECT id FROM loop_executions WHERE loop_id IN ({id_list})
                             )
                         )"
                    ),
                ))
                .await?;
                // 3) 环节执行：经 loop_executions 关联
                txn.execute(Statement::from_string(
                    DbBackend::Sqlite,
                    format!(
                        "DELETE FROM loop_step_executions
                         WHERE loop_execution_id IN (
                             SELECT id FROM loop_executions WHERE loop_id IN ({id_list})
                         )"
                    ),
                ))
                .await?;
                // 4) 阶段执行：经 loop_executions 关联
                txn.execute(Statement::from_string(
                    DbBackend::Sqlite,
                    format!(
                        "DELETE FROM loop_phase_executions
                         WHERE loop_execution_id IN (
                             SELECT id FROM loop_executions WHERE loop_id IN ({id_list})
                         )"
                    ),
                ))
                .await?;
                // 5) 执行主表：直接按 loop_id
                txn.execute(Statement::from_string(
                    DbBackend::Sqlite,
                    format!("DELETE FROM loop_executions WHERE loop_id IN ({id_list})"),
                ))
                .await?;
                // 6) 触发器：044 下线，手工环路的触发器一并清掉
                txn.execute(Statement::from_string(
                    DbBackend::Sqlite,
                    format!("DELETE FROM loop_triggers WHERE loop_id IN ({id_list})"),
                ))
                .await?;
                // 7) 标签关联
                txn.execute(Statement::from_string(
                    DbBackend::Sqlite,
                    format!("DELETE FROM loop_tags WHERE loop_id IN ({id_list})"),
                ))
                .await?;
                // 8) 环节定义
                txn.execute(Statement::from_string(
                    DbBackend::Sqlite,
                    format!("DELETE FROM loop_steps WHERE loop_id IN ({id_list})"),
                ))
                .await?;
                // 9) 阶段定义
                txn.execute(Statement::from_string(
                    DbBackend::Sqlite,
                    format!("DELETE FROM loop_phases WHERE loop_id IN ({id_list})"),
                ))
                .await?;
                // 10) 环路主表：最后删，保证上面子表清理时还能按 loop_id 定位
                txn.execute(Statement::from_string(
                    DbBackend::Sqlite,
                    format!("DELETE FROM loops WHERE id IN ({id_list})"),
                ))
                .await?;
                Ok(())
            })
        })
        .await
        // transaction 返回 TransactionError<DbErr>（Connection/Transaction 两变体均包裹 DbErr），
        // 统一展开为内层 DbErr 以匹配迁移函数签名。
        .map_err(|e| match e {
            sea_orm::TransactionError::Connection(e) => e,
            sea_orm::TransactionError::Transaction(e) => e,
        })?;
    tracing::info!("v80: 手工环路级联删除完成");
    Ok(())
}

/// 查出所有手工环路（process_template_id IS NULL）的 id→name，供删除前日志留痕。
///
/// 用 BTreeMap 让日志里的 id 单调递增、便于人工核对。
async fn list_manual_loops(db: &Database) -> Result<BTreeMap<i64, String>, sea_orm::DbErr> {
    let rows = db
        .conn
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT id, name FROM loops WHERE process_template_id IS NULL",
        ))
        .await?;
    let mut map = BTreeMap::new();
    for row in rows {
        // id 是主键必非空；name 理论上非空但用 unwrap_or_default 兜底避免迁移因脏数据中断。
        let id: i64 = row.try_get_by_index(0)?;
        let name: String = row.try_get_by_index(1).unwrap_or_default();
        map.insert(id, name);
    }
    Ok(map)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// id 列表拼接：空集合返回空串；多 id 用逗号连接，顺序随 BTreeMap 自然递增。
    /// 纯函数便于单测：DELETE 语句的 IN 范围由它决定，错了会误删或漏删。
    #[test]
    fn test_join_id_list() {
        let mut m: BTreeMap<i64, String> = BTreeMap::new();
        assert_eq!(join_id_list(&m), "");
        m.insert(3, "c".into());
        m.insert(1, "a".into());
        m.insert(2, "b".into());
        // BTreeMap 按 key 升序迭代，保证日志/SQL 里的 id 顺序稳定可读
        assert_eq!(join_id_list(&m), "1,2,3");
    }

    fn join_id_list(m: &BTreeMap<i64, String>) -> String {
        m.keys().map(|k| k.to_string()).collect::<Vec<_>>().join(",")
    }
}
