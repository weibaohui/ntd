//! 迁移 v92：tasks 与 workspace_settings 各加 `delegate_max_rounds` 列（需求 092 护栏配置化）。
//!
//! 背景：此前自动接力轮数上限写死在常量 `MAX_DELEGATE_ROUNDS = 10`（前后端各一处），
//! 运行时不可调、跨栈易漂移。092 把上限改为「任务覆盖 → 工作空间默认 → 兜底常量」三级可配，
//! 故需在两张表各加一列存放可配置的上限阈值。
//!
//! 两列均 nullable、无 DEFAULT：
//! - `tasks.delegate_max_rounds`：单任务覆盖，NULL 表示沿用工作空间默认。
//! - `workspace_settings.delegate_max_rounds`：工作空间默认，NULL 表示回退终极兜底常量。
//! SQLite 对 `ADD COLUMN` 的 nullable 列会自动给旧行填 NULL，无需回填语句（与 v91 的
//! assignee_kind 同口径）。
//!
//! 幂等：`add_column_if_missing` 用 `PRAGMA table_info` 探测列存在再 ADD，任意中间状态可重入。

use async_trait::async_trait;

use crate::db::migration::add_column_if_missing;
use crate::db::{Database, migration::Migration};

/// v92：tasks / workspace_settings 各加 `delegate_max_rounds` 列。
pub struct V92DelegateMaxRounds;

#[async_trait]
impl Migration for V92DelegateMaxRounds {
    fn version(&self) -> i64 {
        92
    }

    fn name(&self) -> &'static str {
        "V92DelegateMaxRounds"
    }

    /// 两表各追加一列。nullable 无 DEFAULT：旧行自动得 NULL（=沿用更下一级默认），
    /// 不破坏既有数据；新建库走 consolidated_schema 已内联同列。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 任务级覆盖：优先级最高，NULL 则回退工作空间默认。
        add_column_if_missing(
            db,
            "tasks",
            "delegate_max_rounds",
            "ALTER TABLE tasks ADD COLUMN delegate_max_rounds INTEGER",
        )
        .await?;
        // 工作空间级默认：NULL 则回退终极兜底常量 MAX_DELEGATE_ROUNDS。
        add_column_if_missing(
            db,
            "workspace_settings",
            "delegate_max_rounds",
            "ALTER TABLE workspace_settings ADD COLUMN delegate_max_rounds INTEGER",
        )
        .await?;
        tracing::info!("v92: tasks/workspace_settings 已添加 delegate_max_rounds 列");
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::migration::table_has_column;

    /// 幂等：重复执行 up 不报错（add_column_if_missing 探测列存在则跳过）。
    #[tokio::test]
    async fn test_v92_is_idempotent() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        V92DelegateMaxRounds.up(&db).await.expect("first run");
        V92DelegateMaxRounds
            .up(&db)
            .await
            .expect("second run (idempotent)");
    }

    /// 两表新列均可达：确保 schema 与迁移目标一致（tasks + workspace_settings 各一列）。
    #[tokio::test]
    async fn test_v92_columns_present() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        V92DelegateMaxRounds.up(&db).await.expect("up");
        assert!(
            table_has_column(&db, "tasks", "delegate_max_rounds")
                .await
                .unwrap(),
            "tasks.delegate_max_rounds 应存在"
        );
        assert!(
            table_has_column(&db, "workspace_settings", "delegate_max_rounds")
                .await
                .unwrap(),
            "workspace_settings.delegate_max_rounds 应存在"
        );
    }
}
