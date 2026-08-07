//! 迁移 v91：tasks 表新增「委派执行」相关列（需求 092）。
//!
//! 背景：任务此前只能绑定工艺环路执行。092 新增第二种执行方式「委派」——把任务交给
//! 一个专家或执行器跑一次，可选开启「自动接力」让协调者型专家（管家）自主调度。
//! 这些语义需要持久化到 tasks 表，故加 5 列。
//!
//! 列清单（与 entity `db/entity/tasks.rs` 对应）：
//! - `execution_mode`：`loop`（默认）/ `delegate`，决定走工艺环路还是委派。
//! - `assignee_kind`：委派对象类型 `executor` / `expert`（仅 delegate 有值）。
//! - `assignee_name`：委派处理人名。
//! - `auto_continue`：自动接力开关（0/1），仅 expert 可为 1。
//! - `continue_rounds`：接力已执行轮数（护栏计数）。
//!
//! 幂等：每列都用 `add_column_if_missing`（`PRAGMA table_info` 探测列存在再 ADD），
//! 任意中间状态可重入；旧任务 execution_mode 默认 'loop'，行为与改动前一致。

use async_trait::async_trait;

use crate::db::migration::add_column_if_missing;
use crate::db::{Database, migration::Migration};

/// v91：tasks 表加 5 列支撑任务委派执行。
pub struct V91TaskDelegateExecution;

#[async_trait]
impl Migration for V91TaskDelegateExecution {
    fn version(&self) -> i64 {
        91
    }

    fn name(&self) -> &'static str {
        "V91TaskDelegateExecution"
    }

    /// 逐列探测追加。NOT NULL 列必须带 DEFAULT（SQLite 对 ADD COLUMN 的硬性要求），
    /// 否则旧行无法回填会报错；这里 execution_mode='loop'、auto_continue/continue_rounds=0
    /// 保证旧行直接获得合法默认值，无需额外回填语句。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 执行方式：默认 loop（工艺环路），旧任务由此自动归为环路、行为不变。
        add_column_if_missing(
            db,
            "tasks",
            "execution_mode",
            "ALTER TABLE tasks ADD COLUMN execution_mode TEXT NOT NULL DEFAULT 'loop'",
        )
        .await?;
        // 委派对象类型与名称：可空，仅 delegate 模式写入。
        add_column_if_missing(
            db,
            "tasks",
            "assignee_kind",
            "ALTER TABLE tasks ADD COLUMN assignee_kind TEXT",
        )
        .await?;
        add_column_if_missing(
            db,
            "tasks",
            "assignee_name",
            "ALTER TABLE tasks ADD COLUMN assignee_name TEXT",
        )
        .await?;
        // 自动接力开关与轮数计数：默认 0（关闭 / 未开始），旧任务不受影响。
        add_column_if_missing(
            db,
            "tasks",
            "auto_continue",
            "ALTER TABLE tasks ADD COLUMN auto_continue INTEGER NOT NULL DEFAULT 0",
        )
        .await?;
        add_column_if_missing(
            db,
            "tasks",
            "continue_rounds",
            "ALTER TABLE tasks ADD COLUMN continue_rounds INTEGER NOT NULL DEFAULT 0",
        )
        .await?;
        tracing::info!("v91: tasks 表已添加任务委派执行相关 5 列");
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
    /// 内存库经 consolidated_schema 初始化已含新列，up 应全部走「跳过」分支。
    #[tokio::test]
    async fn test_v91_is_idempotent() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        V91TaskDelegateExecution.up(&db).await.expect("first run");
        V91TaskDelegateExecution
            .up(&db)
            .await
            .expect("second run (idempotent)");
    }

    /// 5 列均可达：在已初始化的库上验证列存在，确保 schema 与迁移目标一致。
    #[tokio::test]
    async fn test_v91_columns_present() {
        let db = Database::new(":memory:")
            .await
            .expect(":memory: db must open");
        V91TaskDelegateExecution.up(&db).await.expect("up");
        for col in [
            "execution_mode",
            "assignee_kind",
            "assignee_name",
            "auto_continue",
            "continue_rounds",
        ] {
            assert!(
                table_has_column(&db, "tasks", col).await.unwrap(),
                "tasks.{col} 应存在"
            );
        }
    }
}
