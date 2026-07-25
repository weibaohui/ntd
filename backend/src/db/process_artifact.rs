//! 工艺产物 & 门禁数据库访问层。
//!
//! 负责 `loop_step_artifacts` 与 `loop_step_execution_gates` 表的读写，
//! 供 `services/process/artifact_capture` 与 `services/process/gate_evaluator` 使用。

use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
};

use crate::db::entity::{loop_step_artifacts, loop_step_execution_gates};
use crate::db::Database;
use crate::models::utc_timestamp;

impl Database {
    // ── Artifacts ──────────────────────────────────────────────

    /// 创建一条环节产物记录。
    ///
    /// `captured_by` 通常为 execution_record_id 字符串，或 "manual" 表示人工补充。
    pub async fn create_loop_step_artifact(
        &self,
        loop_step_execution_id: i64,
        name: &str,
        artifact_type: &str,
        locator: &str,
        content_text: Option<&str>,
        captured_by: Option<&str>,
    ) -> Result<loop_step_artifacts::Model, sea_orm::DbErr> {
        let now = utc_timestamp();
        let am = loop_step_artifacts::ActiveModel {
            loop_step_execution_id: ActiveValue::Set(loop_step_execution_id),
            name: ActiveValue::Set(name.to_string()),
            artifact_type: ActiveValue::Set(artifact_type.to_string()),
            locator: ActiveValue::Set(locator.to_string()),
            content_text: ActiveValue::Set(content_text.map(|s| s.to_string())),
            captured_at: ActiveValue::Set(now),
            captured_by: ActiveValue::Set(captured_by.map(|s| s.to_string())),
            ..Default::default()
        };
        am.insert(&self.conn).await
    }

    /// 列出某次环节执行的所有产物，按捕获时间排序。
    pub async fn list_loop_step_artifacts(
        &self,
        loop_step_execution_id: i64,
    ) -> Result<Vec<loop_step_artifacts::Model>, sea_orm::DbErr> {
        loop_step_artifacts::Entity::find()
            .filter(loop_step_artifacts::Column::LoopStepExecutionId.eq(loop_step_execution_id))
            .order_by_asc(loop_step_artifacts::Column::CapturedAt)
            .all(&self.conn)
            .await
    }

    /// 按名称查找某次环节执行的产物。
    pub async fn get_loop_step_artifact_by_name(
        &self,
        loop_step_execution_id: i64,
        name: &str,
    ) -> Result<Option<loop_step_artifacts::Model>, sea_orm::DbErr> {
        loop_step_artifacts::Entity::find()
            .filter(loop_step_artifacts::Column::LoopStepExecutionId.eq(loop_step_execution_id))
            .filter(loop_step_artifacts::Column::Name.eq(name))
            .one(&self.conn)
            .await
    }

    // ── Gates ──────────────────────────────────────────────────

    /// 创建一条门禁评价记录，初始状态为 `pending`。
    pub async fn create_loop_step_execution_gate(
        &self,
        loop_step_execution_id: i64,
        gate_type: &str,
        gate_name: &str,
        config: &str,
    ) -> Result<loop_step_execution_gates::Model, sea_orm::DbErr> {
        let am = loop_step_execution_gates::ActiveModel {
            loop_step_execution_id: ActiveValue::Set(loop_step_execution_id),
            gate_type: ActiveValue::Set(gate_type.to_string()),
            gate_name: ActiveValue::Set(gate_name.to_string()),
            config: ActiveValue::Set(config.to_string()),
            status: ActiveValue::Set("pending".to_string()),
            ..Default::default()
        };
        am.insert(&self.conn).await
    }

    /// 更新门禁评价结果（状态、结果 JSON、评价时间、评价者）。
    pub async fn update_loop_step_execution_gate(
        &self,
        id: i64,
        status: &str,
        result: Option<&str>,
        evaluated_by: Option<&str>,
    ) -> Result<(), sea_orm::DbErr> {
        let existing = loop_step_execution_gates::Entity::find_by_id(id)
            .one(&self.conn)
            .await?;
        if let Some(c) = existing {
            let mut am: loop_step_execution_gates::ActiveModel = c.into();
            am.status = ActiveValue::Set(status.to_string());
            am.result = ActiveValue::Set(result.map(|s| s.to_string()));
            am.evaluated_at = ActiveValue::Set(Some(utc_timestamp()));
            am.evaluated_by = ActiveValue::Set(evaluated_by.map(|s| s.to_string()));
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    /// 列出某次环节执行的所有门禁记录。
    pub async fn list_loop_step_execution_gates(
        &self,
        loop_step_execution_id: i64,
    ) -> Result<Vec<loop_step_execution_gates::Model>, sea_orm::DbErr> {
        loop_step_execution_gates::Entity::find()
            .filter(
                loop_step_execution_gates::Column::LoopStepExecutionId.eq(loop_step_execution_id),
            )
            .order_by_asc(loop_step_execution_gates::Column::Id)
            .all(&self.conn)
            .await
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::bool_assert_comparison
)]
mod tests {
    use super::*;

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    #[tokio::test]
    async fn test_create_and_list_loop_step_artifacts() {
        let db = fresh_db().await;

        // 插入 FK 依赖行。
        db.exec("INSERT INTO todos (id, title, prompt, status) VALUES (1, 't', 'p', 'pending')").await.unwrap();
        db.exec("INSERT INTO loops (id, name) VALUES (1, 'l')").await.unwrap();
        db.exec("INSERT INTO loop_steps (id, loop_id, name, todo_id) VALUES (1, 1, 's', 1)").await.unwrap();
        db.exec("INSERT INTO loop_executions (id, loop_id, trigger_type, started_at, status) VALUES (1, 1, 'manual', '2024-01-01', 'running')").await.unwrap();
        db.exec("INSERT INTO loop_step_executions (id, loop_execution_id, step_id, todo_id, status) VALUES (1, 1, 1, 1, 'running')").await.unwrap();

        let a1 = db
            .create_loop_step_artifact(1, "PRD", "file", "docs/PRD.md", Some("content"), Some("100"))
            .await
            .unwrap();
        assert_eq!(a1.loop_step_execution_id, 1);
        assert_eq!(a1.name, "PRD");

        let a2 = db
            .create_loop_step_artifact(1, "Summary", "text", "## 结论", None, Some("manual"))
            .await
            .unwrap();
        assert_eq!(a2.captured_by.as_deref(), Some("manual"));

        let list = db.list_loop_step_artifacts(1).await.unwrap();
        assert_eq!(list.len(), 2);

        let found = db.get_loop_step_artifact_by_name(1, "PRD").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().artifact_type, "file");

        let missing = db.get_loop_step_artifact_by_name(1, "Missing").await.unwrap();
        assert!(missing.is_none());
    }
}
