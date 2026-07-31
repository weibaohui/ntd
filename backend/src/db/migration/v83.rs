//! V83 迁移：`loop_steps` 新增 `step_template_refs` 列（需求 054）。
//!
//! ## 背景
//! 环节 spec 模板（`StepTemplateRef{name,path}`）此前只活在工艺 YAML 里，
//! 既未落库也未注入执行器提示词。需求 054 要求执行器执行时把环节引用的 spec
//! 模板注入 prompt「供其重点阅读」，因此需要把 `link.step_template` 持久化到
//! `loop_steps`，与 `expected_artifacts` 完全同构（JSON 数组串，默认 `"[]"`）。
//!
//! ## 幂等
//! 通过 `add_column_if_missing` 先用 `pragma_table_info` 探测列是否存在，
//! 已存在则跳过；重复执行不会报错。
use super::super::Database;
use super::{add_column_if_missing, Migration};

/// V83：给 `loop_steps` 增加 `step_template_refs` 列。
pub(super) struct V83AddLoopStepTemplateRefs;

#[async_trait::async_trait]
impl Migration for V83AddLoopStepTemplateRefs {
    // 紧随 V82，单调递增；新迁移必须严格大于已有版本。
    fn version(&self) -> i64 {
        83
    }

    fn name(&self) -> &'static str {
        "V83AddLoopStepTemplateRefs"
    }

    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        // 存环节 spec 模板引用 JSON 数组（[{name,path}, ...]），默认空数组。
        // 与 expected_artifacts / skill_names / gate_config 同构。
        add_column_if_missing(
            db,
            "loop_steps",
            "step_template_refs",
            "ALTER TABLE loop_steps ADD COLUMN step_template_refs TEXT NOT NULL DEFAULT '[]'",
        )
        .await?;
        tracing::info!("V83: loop_steps.step_template_refs 列已就绪（需求 054）");
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::super::{table_exists, table_has_column};
    use super::*;

    async fn fresh_db() -> Database {
        // Database::new 会先跑完全量迁移链；fresh 库上 v83 已把列加上。
        Database::new(":memory:")
            .await
            .expect(":memory: db must open")
    }

    #[tokio::test]
    async fn v83_adds_step_template_refs_column() {
        let db = fresh_db().await;
        // fresh 库跑完迁移链后，列必须存在。
        assert!(
            table_has_column(&db, "loop_steps", "step_template_refs")
                .await
                .unwrap(),
            "v83 应用后 loop_steps 必须有 step_template_refs 列"
        );
        // loop_steps 表本身存在（防误删）。
        assert!(
            table_exists(&db, "loop_steps").await.unwrap(),
            "loop_steps 表应存在"
        );
    }

    #[tokio::test]
    async fn v83_is_idempotent() {
        let db = fresh_db().await;
        // 列已存在（fresh 库）时重复应用不得报错。
        V83AddLoopStepTemplateRefs.up(&db).await.unwrap();
        V83AddLoopStepTemplateRefs.up(&db).await.unwrap();
    }

    #[tokio::test]
    async fn v83_step_template_refs_defaults_to_empty_array() {
        use sea_orm::ConnectionTrait;
        let db = fresh_db().await;
        // 直接读 schema 列默认值，确认是 '[]'（与 expected_artifacts 同构，避免脏/空数据误解）。
        let row = db
            .conn
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT dflt_value FROM pragma_table_info('loop_steps') \
                 WHERE name='step_template_refs'",
            ))
            .await
            .unwrap()
            .unwrap();
        let dflt: String = row.try_get_by_index(0).unwrap();
        // SQLite 把 DEFAULT '[]' 原样存为带单引号的 '[]'。
        assert_eq!(dflt, "'[]'", "step_template_refs 列默认值应为 '[]'");
    }
}
