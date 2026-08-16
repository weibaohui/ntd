//! 数据库迁移 V95：workspace_settings「默认响应」四列退役，「空间管家」两列上线（108 空间管家）。
//!
//! 背景：消息路由从「斜杠命令 + 默认响应兜底」改为「斜杠精确匹配 + 空间管家」
//! （docs/design/108-空间管家-设计.md）。default_response_type / default_response_todo_id /
//! default_response_loop_id / default_response_executor 四列失去所有读取方，本迁移删除之；
//! 新增 butler_expert_name / butler_executor 两列承载管家配置。
//! 升级时把 default_response_executor 的值承接到 butler_executor：
//! 已配置执行器的工作空间升级后管家沿用同一执行器，行为不断档。

use async_trait::async_trait;

use super::super::Database;
use super::{Migration, drop_column_if_exists, table_has_column};

/// 默认响应四列的名单：集中定义避免删列循环与注释各写一份漂移。
const LEGACY_DEFAULT_RESPONSE_COLUMNS: [&str; 4] = [
    "default_response_type",
    "default_response_todo_id",
    "default_response_loop_id",
    "default_response_executor",
];

pub(super) struct V95WorkspaceButler;

#[async_trait]
impl Migration for V95WorkspaceButler {
    fn version(&self) -> i64 {
        95
    }
    fn name(&self) -> &'static str {
        "workspace_butler_columns"
    }

    /// 升级步骤：加管家两列 → 承接旧执行器值 → 删默认响应四列。
    /// 每步都带列存在性守卫（与 V94 同一思路）：up() 内各 DDL 独立自动提交，
    /// 崩溃可能落在任意步骤之间，守卫保证半途残留态重跑时补完剩余步骤而非报错。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 步骤 1：加管家专家列。守卫：全新库（consolidated schema 已含该列）或
        // 「列已加、版本未记录」的残留态重跑时跳过，避免 duplicate column 报错。
        if !table_has_column(db, "workspace_settings", "butler_expert_name").await? {
            db.exec("ALTER TABLE workspace_settings ADD COLUMN butler_expert_name TEXT")
                .await?;
        }
        // 步骤 2：加管家执行器列，守卫同上。
        if !table_has_column(db, "workspace_settings", "butler_executor").await? {
            db.exec("ALTER TABLE workspace_settings ADD COLUMN butler_executor TEXT")
                .await?;
        }
        // 步骤 3：承接旧默认执行器 → 管家执行器。WHERE 双条件缺一不可：
        // butler_executor IS NULL 保证不覆盖新值（残留态/人工预写）；
        // default_response_executor IS NOT NULL 避免把 NULL 刷成 NULL 的无意义写。
        // 守卫：旧列已删的残留态重跑时整步跳过（列不存在 SQL 必报错）。
        if table_has_column(db, "workspace_settings", "default_response_executor").await? {
            db.exec(
                "UPDATE workspace_settings SET butler_executor = default_response_executor
                 WHERE butler_executor IS NULL AND default_response_executor IS NOT NULL",
            )
            .await?;
        }
        // 步骤 4：删默认响应四列。SQLite ≥3.35 支持 DROP COLUMN；四列均无索引/约束
        // 引用（见 v94 态 schema），可安全直删。helper 自带存在性守卫保证幂等。
        for col in LEGACY_DEFAULT_RESPONSE_COLUMNS {
            drop_column_if_exists(db, "workspace_settings", col).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
// 迁移测试断言用 expect/panic 直接失败即可，与 v94 等历史迁移测试的豁免口径一致。
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::super::super::Database;
    use super::super::table_has_column;

    use super::V95WorkspaceButler;
    use super::super::Migration;

    /// 直接查询某列值（单行字符串），测试断言用。
    /// None 表示行不存在或值为 NULL，由调用方区分断言语义。
    async fn query_text(db: &Database, sql: &str) -> Option<String> {
        use sea_orm::ConnectionTrait;
        db.conn
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                sql.to_string(),
            ))
            .await
            .expect("query must succeed")?
            .try_get_by_index::<String>(0)
            .ok()
    }

    /// 老库升级回归：v94 态库（默认响应四列齐全 + default_response_executor 有值）
    /// 跑 V95 后：butler_executor 承接旧值、两个管家列存在、四个默认响应列消失。
    /// 这是 108 的数据安全底线——承接保证已配置执行器的工作空间升级后行为不断档。
    #[tokio::test]
    async fn test_v95_old_db_upgrade_carries_executor_and_drops_legacy_columns() {
        let db = Database::connect_without_migrations(":memory:")
            .await
            .expect(":memory: db must open");
        // v94 是 V95 的直接前置版本：跑到 v94 即得到「升级前夜」的老库状态
        db.run_migrations_with(94).await.expect("build v94 state");

        // 在旧 schema 下铺真实数据：一行配了默认执行器的 workspace_settings。
        db.exec(
            "INSERT INTO workspace_settings
             (workspace_id, default_response_type, default_response_executor, system_prompt)
             VALUES (9, 'executor', 'pi', '共识保留')",
        )
        .await
        .expect("insert legacy settings");

        V95WorkspaceButler.up(&db).await.expect("V95 must succeed");

        // 管家两列就位
        assert!(
            table_has_column(&db, "workspace_settings", "butler_expert_name")
                .await
                .expect("table_has_column must succeed"),
            "butler_expert_name 列应已新增"
        );
        assert!(
            table_has_column(&db, "workspace_settings", "butler_executor")
                .await
                .expect("table_has_column must succeed"),
            "butler_executor 列应已新增"
        );
        // 旧默认执行器值承接到管家执行器
        assert_eq!(
            query_text(&db, "SELECT butler_executor FROM workspace_settings WHERE workspace_id = 9").await,
            Some("pi".to_string()),
            "default_response_executor 的值应承接到 butler_executor"
        );
        // 无关列数据完好（system_prompt 不属于本次迁移范围，不能被误伤）
        assert_eq!(
            query_text(&db, "SELECT system_prompt FROM workspace_settings WHERE workspace_id = 9").await,
            Some("共识保留".to_string()),
            "system_prompt 存量数据必须原样保留"
        );
        // 四个默认响应列全部消失
        for col in [
            "default_response_type",
            "default_response_todo_id",
            "default_response_loop_id",
            "default_response_executor",
        ] {
            assert!(
                !table_has_column(&db, "workspace_settings", col)
                    .await
                    .expect("table_has_column must succeed"),
                "旧列 {col} 应已删除"
            );
        }
    }

    /// 承接不覆盖新值：butler_executor 已有值时（如升级前人工写过），
    /// 承接 UPDATE 的 WHERE 条件必须跳过，不能回灌旧值。
    #[tokio::test]
    async fn test_v95_carry_does_not_overwrite_existing_butler_executor() {
        let db = Database::connect_without_migrations(":memory:")
            .await
            .expect(":memory: db must open");
        db.run_migrations_with(94).await.expect("build v94 state");
        db.exec(
            "INSERT INTO workspace_settings (workspace_id, default_response_executor)
             VALUES (9, 'pi')",
        )
        .await
        .expect("insert legacy settings");
        // 模拟「管家列已存在且有值」的中间态：先加列并写入新执行器
        db.exec("ALTER TABLE workspace_settings ADD COLUMN butler_executor TEXT")
            .await
            .expect("add butler_executor");
        db.exec("UPDATE workspace_settings SET butler_executor = 'claudecode' WHERE workspace_id = 9")
            .await
            .expect("preset butler_executor");

        V95WorkspaceButler.up(&db).await.expect("V95 must succeed");

        assert_eq!(
            query_text(&db, "SELECT butler_executor FROM workspace_settings WHERE workspace_id = 9").await,
            Some("claudecode".to_string()),
            "butler_executor 已有值时承接必须跳过，不能被旧值覆盖"
        );
    }

    /// 幂等：全新库（consolidated schema 已是最终态）重跑 up()，
    /// 所有守卫走「跳过」分支，不报「列已存在/不存在」类错误。
    #[tokio::test]
    async fn test_v95_is_idempotent_on_fresh_db() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        V95WorkspaceButler
            .up(&db)
            .await
            .expect("全新库重跑 V95 应被守卫跳过而非报错");
    }
}
