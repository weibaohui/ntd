//! 迁移 v87：残留态 DB 自愈（BUG-009 / Issue #973）。
//!
//! 背景：本地 DB 处于「中间版本残留态」（迁移半途中断、跨版本试跑后回退、手动改库等），
//! `schema_version` 表缺某些版本的行，导致 `run_migrations` 重跑时旧迁移 v1 的
//! `add_column_warn` 对「列已存在」只 warn 不阻断，但后续迁移里依赖该列的 SQL
//! （SELECT/INSERT `acceptance_criteria` 等）直接抛 `no such column` → 启动失败。
//!
//! 修复：在迁移链末端追加本「自愈」迁移，幂等探测并补齐历史缺失列。
//! - 正常库：列都在 → no-op → 记行 → 完成
//! - 残留态库：探测到缺列 → ALTER 补回 → 记行 → 完成，启动恢复正常
//!
//! 不动旧迁移 v1：迁移不可变约定（已应用到生产库的迁移不应改）。

use crate::db::{Database, migration::Migration};
use crate::db::migration::add_column_if_missing;
use async_trait::async_trait;
use tracing::info;

/// v87 自愈迁移：幂等补齐历史残留态缺失的关键列。
pub struct V87SelfHealResidual;

#[async_trait]
impl Migration for V87SelfHealResidual {
    fn version(&self) -> i64 { 87 }
    fn name(&self) -> &'static str { "self_heal_residual" }

    /// 幂等探测并补齐历史缺失列。
    ///
    /// 以 v1 `add_legacy_todos_columns` / `add_legacy_loops_columns` 的列清单为准，
    /// 逐表逐列用 `add_column_if_missing`（pragma_table_info 探测）补齐。
    /// 列已存在则跳过，不存在则 ALTER ADD。
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        info!("v87: 残留态自愈迁移开始（幂等补齐历史缺失列）");

        // todos 历史追加列（与 v1 add_legacy_todos_columns 对齐）
        // 每条：探测列存在性 → 缺则 ALTER ADD
        add_column_if_missing(
            db, "todos", "workspace",
            "ALTER TABLE todos ADD COLUMN workspace TEXT",
        ).await?;
        add_column_if_missing(
            db, "todos", "worktree_enabled",
            "ALTER TABLE todos ADD COLUMN worktree_enabled INTEGER DEFAULT 0",
        ).await?;
        add_column_if_missing(
            db, "todos", "scheduler_timezone",
            "ALTER TABLE todos ADD COLUMN scheduler_timezone TEXT",
        ).await?;
        add_column_if_missing(
            db, "todos", "hooks",
            "ALTER TABLE todos ADD COLUMN hooks TEXT",
        ).await?;
        // acceptance_criteria 是 issue #973 报的关键缺失列，后续迁移依赖它
        add_column_if_missing(
            db, "todos", "acceptance_criteria",
            "ALTER TABLE todos ADD COLUMN acceptance_criteria TEXT",
        ).await?;
        add_column_if_missing(
            db, "todos", "todo_type",
            "ALTER TABLE todos ADD COLUMN todo_type INTEGER DEFAULT 0",
        ).await?;
        add_column_if_missing(
            db, "todos", "parent_todo_id",
            "ALTER TABLE todos ADD COLUMN parent_todo_id INTEGER",
        ).await?;
        add_column_if_missing(
            db, "todos", "auto_review_enabled",
            "ALTER TABLE todos ADD COLUMN auto_review_enabled INTEGER DEFAULT 1",
        ).await?;

        // loops 历史追加列（与 v1 add_legacy_loops_columns 对齐）
        add_column_if_missing(
            db, "loops", "workspace",
            "ALTER TABLE loops ADD COLUMN workspace TEXT",
        ).await?;

        info!("v87: 残留态自愈迁移完成");
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark, clippy::single_match_else)]
mod tests {
    use super::*;
    use crate::db::Database;

    /// 构造一个全新内存 DB（跑完所有已注册迁移）。
    async fn fresh_db() -> Database {
        Database::new(":memory:")
            .await
            .expect(":memory: db must open")
    }

    /// AC-009-2：正常库跑 v87 为 no-op（不重复加列，schema_version 记行）。
    /// fresh_db 已含所有迁移，v87 单独再 up 一次应成功且不破坏结构。
    #[tokio::test]
    async fn test_v87_noop_on_fresh_db() {
        let db = fresh_db().await;
        // fresh_db 已跑过 v87（all_migrations 末位），这里再手动 up 验证幂等
        V87SelfHealResidual.up(&db).await.expect("v87 re-up must succeed");
        // todos.acceptance_criteria 列应仍存在（幂等 no-op）
        let has = crate::db::migration::table_has_column(&db, "todos", "acceptance_criteria")
            .await
            .expect("probe must succeed");
        assert!(has, "acceptance_criteria 列必须存在");
    }

    /// AC-009-3：残留态库（删 acceptance_criteria 列模拟）跑 v87 后列补回。
    /// SQLite 3.35+ 支持 ALTER TABLE DROP COLUMN，直接删列模拟残留态。
    #[tokio::test]
    async fn test_v87_heals_missing_acceptance_criteria() {
        let db = fresh_db().await;
        // 模拟残留态：直接 DROP COLUMN acceptance_criteria
        // 若 DROP COLUMN 旧 SQLite 不支持，test 会 panic 报错——可接受，CI 用新 SQLite
        db.exec("ALTER TABLE todos DROP COLUMN acceptance_criteria")
            .await
            .expect("DROP COLUMN must succeed on SQLite 3.35+");

        // 确认列确实缺失
        let has_before = crate::db::migration::table_has_column(&db, "todos", "acceptance_criteria")
            .await
            .expect("probe must succeed");
        assert!(!has_before, "模拟残留态：acceptance_criteria 列应缺失");

        // 跑 v87 自愈
        V87SelfHealResidual.up(&db).await.expect("v87 must heal");

        // 列应被补回
        let has_after = crate::db::migration::table_has_column(&db, "todos", "acceptance_criteria")
            .await
            .expect("probe must succeed");
        assert!(has_after, "v87 应补回 acceptance_criteria 列");
    }

    /// AC-009-3 补充：残留态库（删 loops.workspace 列模拟）跑 v87 后列补回。
    #[tokio::test]
    async fn test_v87_heals_missing_loops_workspace() {
        let db = fresh_db().await;
        db.exec("ALTER TABLE loops DROP COLUMN workspace")
            .await
            .expect("DROP COLUMN must succeed on SQLite 3.35+");

        let has_before = crate::db::migration::table_has_column(&db, "loops", "workspace")
            .await.expect("probe");
        assert!(!has_before, "模拟残留态：loops.workspace 列应缺失");

        V87SelfHealResidual.up(&db).await.expect("v87 must heal");

        let has_after = crate::db::migration::table_has_column(&db, "loops", "workspace")
            .await.expect("probe");
        assert!(has_after, "v87 应补回 loops.workspace 列");
    }
}
