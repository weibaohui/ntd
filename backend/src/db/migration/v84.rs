//! V84 迁移：`todos` 新增 `skills` 列（需求 055）。
//!
//! ## 背景
//! 工艺环节（link）允许配置 `skills`，但安装工艺创建的事项此前不携带技能——
//! `link.skills` 只落到 `loop_steps.skill_names` 供 AI 评审门禁引用，执行器
//! 执行事项时 prompt 中没有任何 skill 引用。需求 055 要求事项自身携带技能列表，
//! 执行时以 `/skill-name` 形式注入 prompt，由执行器 CLI 自行解析。
//!
//! ## 设计
//! 列类型为 JSON 数组串（默认 `"[]"`），与 `loop_steps.skill_names` /
//! `expected_artifacts` / `step_template_refs` 完全同构，保持全库 JSON 列口径一致。
//!
//! ## 幂等
//! 通过 `add_column_if_missing` 先用 `pragma_table_info` 探测列是否存在，
//! 已存在则跳过；重复执行不会报错。
use super::super::Database;
use super::{add_column_if_missing, Migration};

/// V84：给 `todos` 增加 `skills` 列。
pub(super) struct V84AddTodoSkills;

#[async_trait::async_trait]
impl Migration for V84AddTodoSkills {
    // 紧随 V83，单调递增；新迁移必须严格大于已有版本。
    fn version(&self) -> i64 {
        84
    }

    fn name(&self) -> &'static str {
        "V84AddTodoSkills"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 存事项级技能名 JSON 数组（["code-review", ...]），默认空数组。
        // 与 loop_steps.skill_names 同构，存量行自动获得默认空数组，零数据迁移成本。
        add_column_if_missing(
            db,
            "todos",
            "skills",
            "ALTER TABLE todos ADD COLUMN skills TEXT NOT NULL DEFAULT '[]'",
        )
        .await?;
        tracing::info!("V84: todos.skills 列已就绪（需求 055）");
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::super::{table_exists, table_has_column};
    use super::*;

    async fn fresh_db() -> Database {
        // Database::new 会先跑完全量迁移链；fresh 库上 v84 已把列加上。
        Database::new(":memory:")
            .await
            .expect(":memory: db must open")
    }

    #[tokio::test]
    async fn v84_adds_skills_column() {
        let db = fresh_db().await;
        // fresh 库跑完迁移链后，列必须存在。
        assert!(
            table_has_column(&db, "todos", "skills").await.unwrap(),
            "v84 应用后 todos 必须有 skills 列"
        );
        // todos 表本身存在（防误删）。
        assert!(table_exists(&db, "todos").await.unwrap(), "todos 表应存在");
    }

    #[tokio::test]
    async fn v84_is_idempotent() {
        let db = fresh_db().await;
        // 列已存在（fresh 库）时重复应用不得报错。
        V84AddTodoSkills.up(&db).await.unwrap();
        V84AddTodoSkills.up(&db).await.unwrap();
    }

    #[tokio::test]
    async fn v84_skills_defaults_to_empty_array() {
        use sea_orm::ConnectionTrait;
        let db = fresh_db().await;
        // 直接读 schema 列默认值，确认是 '[]'（与 loop_steps.skill_names 同构，避免脏/空数据误解）。
        let row = db
            .conn
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT dflt_value FROM pragma_table_info('todos') \
                 WHERE name='skills'",
            ))
            .await
            .unwrap()
            .unwrap();
        let dflt: String = row.try_get_by_index(0).unwrap();
        // SQLite 把 DEFAULT '[]' 原样存为带单引号的 '[]'。
        assert_eq!(dflt, "'[]'", "skills 列默认值应为 '[]'");
    }
}
