//! 数据库迁移 V56：补齐 blackboards 表缺失的演进字段。
//!
//! ## 背景
//! V47 合并迁移用 `if !table_exists` 做幂等判断，对早期已建过 blackboards 旧表
//! （只含 content / debounce_secs 等原初列）的用户，会跳过整段 CREATE TABLE，
//! 导致后续演进列（pending_record_ids / wiki_prompt / wiki_chat_executor /
//! wiki_chat_sessions）从未被追加。schema_version 已记成 47 已应用，再也不会重跑，
//! 该用户永远缺列，运行时报 `no such column: blackboards.xxx` 启动失败。
//!
//! 还存在更早期的旧表残留字段 wiki_index_prompt / wiki_page_prompt（被 wiki_prompt
//! 取代），本迁移不删它们（SQLite DROP COLUMN 自 v3.35 起支持但各发行版不一），
//! 仅追加缺失的列；旧列保留为 dead column 不影响新代码路径。
//!
//! ## 幂等设计
//! 用 `add_column_if_missing` 逐列追加，列已存在则静默跳过；新装 DB 也无害
//! （V47 已建齐 + V56 跑个 noop）。
//!
//! ## 默认值
//! - pending_record_ids: '[]'（空 JSON 数组字符串）
//! - wiki_prompt: ''（空字符串，业务层会回落到内置默认模板）
//! - wiki_chat_executor / wiki_chat_sessions: NULL（可空）
//!
//! 注意 SQLite 的 ADD COLUMN 不支持 NOT NULL 而无默认值，故对 nullable 列用纯 ADD，
//! 对业务默认值列用 ADD COLUMN ... DEFAULT ...，旧行自动填充默认值。

use async_trait::async_trait;

use super::super::Database;
use super::{Migration, add_column_if_missing};

pub(super) struct V56AddMissingBlackboardColumns;

#[async_trait]
impl Migration for V56AddMissingBlackboardColumns {
    fn version(&self) -> i64 {
        56
    }

    fn name(&self) -> &'static str {
        "add_missing_blackboard_columns"
    }

    /// 逐列追加 blackboards 表缺失字段。
    ///
    /// 顺序无所谓——`add_column_if_missing` 各自独立判断列存在性。
    /// 把每列单独成调用而非批量 ALTER，是因为 SQLite 的 ALTER TABLE ADD COLUMN
    /// 一次只能加一列，且失败一列不会自动回滚其他列。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // pending_record_ids：防抖队列 JSON 数组字符串，业务默认 '[]'（空队列）
        // 必须给 DEFAULT，否则旧行会写入 NULL，后续 JSON 解析会 panic
        add_column_if_missing(
            db,
            "blackboards",
            "pending_record_ids",
            // SQLite ADD COLUMN 带 DEFAULT 会回填旧行，避免存量数据变 NULL
            "ALTER TABLE blackboards ADD COLUMN pending_record_ids TEXT NOT NULL DEFAULT '[]'",
        )
        .await?;
        // wiki_prompt：单阶段 Wiki 维护提示词模板，空字符串触发业务层内置默认模板
        add_column_if_missing(
            db,
            "blackboards",
            "wiki_prompt",
            "ALTER TABLE blackboards ADD COLUMN wiki_prompt TEXT NOT NULL DEFAULT ''",
        )
        .await?;
        // wiki_chat_executor：Wiki 对话执行器名称，nullable（None 表示用默认 "claudecode"）
        add_column_if_missing(
            db,
            "blackboards",
            "wiki_chat_executor",
            "ALTER TABLE blackboards ADD COLUMN wiki_chat_executor TEXT",
        )
        .await?;
        // wiki_chat_sessions：per-executor session ID 的 JSON 对象，nullable
        add_column_if_missing(
            db,
            "blackboards",
            "wiki_chat_sessions",
            "ALTER TABLE blackboards ADD COLUMN wiki_chat_sessions TEXT",
        )
        .await?;
        tracing::info!("V56: blackboards 缺失列已补齐（幂等）");
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::db::migration::table_has_column;
    use crate::db::Database;

    /// 验证 V56 在全新库上跑是 no-op（V47 已建齐所有列）。
    /// 目的：确保新部署不会被 V56 误伤，V56 严格幂等。
    #[tokio::test]
    async fn test_v56_noop_on_fresh_db() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        // 先走完整迁移链到 V55，模拟新部署的稳态 schema
        let v47 = crate::db::migration::v47_v53::V47ConsolidatedBlackboardFeatures;
        v47.up(&db).await.expect("V47 must succeed");
        let v54 = crate::db::migration::v54::V54AddWikiChatExecutor;
        v54.up(&db).await.expect("V54 must succeed");
        let v55 = crate::db::migration::v55::V55AddWikiChatSessions;
        v55.up(&db).await.expect("V55 must succeed");

        // 跑 V56：应成功且不改变任何 schema
        let migration = V56AddMissingBlackboardColumns;
        migration.up(&db).await.expect("V56 must succeed on fresh db");

        // 关键断言：所有列仍然存在（幂等没破坏）
        for col in &[
            "pending_record_ids",
            "wiki_prompt",
            "wiki_chat_executor",
            "wiki_chat_sessions",
        ] {
            assert!(
                table_has_column(&db, "blackboards", col).await.unwrap(),
                "column {col} must still exist after V56 noop"
            );
        }
    }

    /// 验证 V56 能修复「早期 V47 已建表但只含原初列」的旧部署。
    /// 这是本迁移的核心场景：模拟旧表只有 content / debounce_secs / debounce_count，
    /// 跑 V56 后应补齐全部演进列。
    ///
    /// 实现思路：Database::new(":memory:") 会自动跑全套迁移把 blackboards 建齐，
    /// 所以这里先 DROP 掉完整表，手动重建一个「早期旧表」结构（缺演进列 + 多两个
    /// 旧残留列 wiki_index_prompt / wiki_page_prompt），再调 V56.up 验证补列行为。
    /// V56.up 内部用 add_column_if_missing 逐列探测 + ALTER，对新表会真实追加。
    #[tokio::test]
    async fn test_v56_adds_missing_columns_to_legacy_table() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        // 模拟早期 V47 的旧表结构：DROP 完整表重建一个只含原初列的旧版
        // 故意保留 wiki_index_prompt / wiki_page_prompt 两个旧残留列，
        // 验证 V56 不会去碰它们（只追加缺失的新列）
        db.exec("DROP TABLE blackboards").await.expect("must drop");
        db.exec(
            r#"CREATE TABLE blackboards (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                workspace_id INTEGER NOT NULL UNIQUE,
                content TEXT NOT NULL DEFAULT '',
                blackboard_debounce_secs INTEGER NOT NULL DEFAULT 600,
                blackboard_debounce_count INTEGER NOT NULL DEFAULT 10,
                wiki_index_prompt TEXT NOT NULL DEFAULT '',
                wiki_page_prompt TEXT NOT NULL DEFAULT '',
                updated_at TEXT,
                created_at TEXT
            )"#,
        )
        .await
        .expect("legacy table must be created");
        // 插一条旧行，验证 ADD COLUMN DEFAULT 能正确回填，不会变 NULL
        // workspace_id=1 对应的 project_directories 行在 V1 迁移里不存在，
        // 但 SQLite 默认不强制 FOREIGN KEY（除非 PRAGMA foreign_keys=ON），所以可以插
        db.exec(
            r#"INSERT INTO blackboards (workspace_id, content) VALUES (1, 'legacy')"#,
        )
        .await
        .expect("legacy row must be inserted");

        // 跑 V56：应补齐所有缺失列
        let migration = V56AddMissingBlackboardColumns;
        migration
            .up(&db)
            .await
            .expect("V56 must succeed on legacy table");

        // 关键断言：四个演进列全部到位
        for col in &[
            "pending_record_ids",
            "wiki_prompt",
            "wiki_chat_executor",
            "wiki_chat_sessions",
        ] {
            assert!(
                table_has_column(&db, "blackboards", col).await.unwrap(),
                "column {col} must exist after V56 repair"
            );
        }
        // 旧行的 pending_record_ids 应回填默认值 '[]'，而非 NULL
        // 这是 NOT NULL DEFAULT 子句的关键作用，避免后续 JSON 解析 panic
        use sea_orm::ConnectionTrait;
        let row = db
            .conn
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT pending_record_ids, wiki_prompt FROM blackboards WHERE workspace_id = 1"
                    .to_string(),
            ))
            .await
            .expect("row must be readable")
            .expect("legacy row must still exist");
        let pending: String = row.try_get_by_index(0).expect("pending_record_ids must be readable");
        let prompt: String = row.try_get_by_index(1).expect("wiki_prompt must be readable");
        assert_eq!(pending, "[]", "pending_record_ids must backfill to '[]'");
        assert_eq!(prompt, "", "wiki_prompt must backfill to empty string");
        // 旧残留列应仍然存在（V56 不删除任何列，只追加）
        assert!(
            table_has_column(&db, "blackboards", "wiki_index_prompt").await.unwrap(),
            "V56 must not drop legacy wiki_index_prompt column"
        );
        assert!(
            table_has_column(&db, "blackboards", "wiki_page_prompt").await.unwrap(),
            "V56 must not drop legacy wiki_page_prompt column"
        );
    }

    /// 验证 V56 是幂等的（重复执行不报错）。
    /// 防止 schema_version 与 m.up 不一致时下次启动重跑失败。
    #[tokio::test]
    async fn test_v56_is_idempotent() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        let v47 = crate::db::migration::v47_v53::V47ConsolidatedBlackboardFeatures;
        v47.up(&db).await.expect("V47 must succeed");

        let migration = V56AddMissingBlackboardColumns;
        migration.up(&db).await.expect("First run must succeed");
        migration.up(&db).await.expect("Second run must succeed (idempotent)");
    }
}
