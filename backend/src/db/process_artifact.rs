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

    /// 按 ID 获取产物记录。
    pub async fn get_loop_step_artifact(
        &self,
        id: i64,
    ) -> Result<Option<loop_step_artifacts::Model>, sea_orm::DbErr> {
        loop_step_artifacts::Entity::find_by_id(id).one(&self.conn).await
    }

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
            // 初始态 pending 走 D6 枚举（LoopGateStatus），与 update 写入侧共享同一词汇表。
            status: ActiveValue::Set(crate::models::LoopGateStatus::Pending.as_str().to_string()),
            ..Default::default()
        };
        am.insert(&self.conn).await
    }

    /// 更新门禁评价结果（状态、结果 JSON、评价时间、评价者）。
    /// status 收 LoopGateStatus 枚举（D6 收口）——杜绝调用方传裸串拼写错误；
    /// DB 仍存 String（as_str 锁原字面量），行为逐字节不变。
    pub async fn update_loop_step_execution_gate(
        &self,
        id: i64,
        status: crate::models::LoopGateStatus,
        result: Option<&str>,
        evaluated_by: Option<&str>,
    ) -> Result<(), sea_orm::DbErr> {
        let existing = loop_step_execution_gates::Entity::find_by_id(id)
            .one(&self.conn)
            .await?;
        if let Some(c) = existing {
            let mut am: loop_step_execution_gates::ActiveModel = c.into();
            am.status = ActiveValue::Set(status.as_str().to_string());
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

    /// 批量取多个 step_execution 的门禁，按 loop_step_execution_id 分组返回。
    /// 执行历史批量 enrich 门禁摘要时调用，一次 IN 查询消除逐 step 的 N+1（091 性能优化）。
    /// 每组内按 id 升序，与单条版 `list_loop_step_execution_gates` 口径一致。
    pub async fn list_loop_step_execution_gates_by_step_ids(
        &self,
        loop_step_execution_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, Vec<loop_step_execution_gates::Model>>, sea_orm::DbErr> {
        use std::collections::HashMap;
        if loop_step_execution_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = loop_step_execution_gates::Entity::find()
            .filter(loop_step_execution_gates::Column::LoopStepExecutionId.is_in(loop_step_execution_ids.to_vec()))
            .order_by_asc(loop_step_execution_gates::Column::Id)
            .all(&self.conn)
            .await?;
        let mut map: HashMap<i64, Vec<_>> = HashMap::new();
        for row in rows {
            map.entry(row.loop_step_execution_id).or_default().push(row);
        }
        Ok(map)
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

    /// list_loop_step_execution_gates_by_step_ids：按 loop_step_execution_id 批量取闸门并分组，
    /// 组内按 id 升序；未挂闸门的 step_execution 不出现；空入参返空（091 批量化新增）。
    #[tokio::test]
    async fn test_list_loop_step_execution_gates_by_step_ids_groups_and_orders() {
        let db = fresh_db().await;
        // FK 依赖：todo / loop / loop_step / loop_execution / 两个 loop_step_execution。
        db.exec("INSERT INTO todos (id, title, prompt, status) VALUES (1, 't', 'p', 'pending')").await.unwrap();
        db.exec("INSERT INTO loops (id, name) VALUES (1, 'l')").await.unwrap();
        db.exec("INSERT INTO loop_steps (id, loop_id, name, todo_id) VALUES (1, 1, 's', 1)").await.unwrap();
        db.exec("INSERT INTO loop_executions (id, loop_id, trigger_type, started_at, status) VALUES (1, 1, 'manual', '2024-01-01', 'running')").await.unwrap();
        // se=10 挂闸门；se=20 不挂（验证未命中不出现在 map）。
        db.exec("INSERT INTO loop_step_executions (id, loop_execution_id, step_id, todo_id, status) VALUES (10, 1, 1, 1, 'running')").await.unwrap();
        db.exec("INSERT INTO loop_step_executions (id, loop_execution_id, step_id, todo_id, status) VALUES (20, 1, 1, 1, 'running')").await.unwrap();
        // se=10 两个闸门（自增 id 升序），验证组内按 id 升序返回。
        db.exec("INSERT INTO loop_step_execution_gates (loop_step_execution_id, gate_type, gate_name, config, status) VALUES (10, 'rating', 'g-older', '{}', 'pending')").await.unwrap();
        db.exec("INSERT INTO loop_step_execution_gates (loop_step_execution_id, gate_type, gate_name, config, status) VALUES (10, 'rating', 'g-newer', '{}', 'pending')").await.unwrap();
        let map = db.list_loop_step_execution_gates_by_step_ids(&[10, 20, 9999]).await.unwrap();
        let rows = map.get(&10).expect("se=10 应有闸门");
        assert_eq!(rows.len(), 2, "se=10 应有两个闸门");
        assert!(rows[0].id < rows[1].id, "组内按 id 升序");
        assert!(!map.contains_key(&20), "se=20 无闸门不应出现");
        assert!(db.list_loop_step_execution_gates_by_step_ids(&[]).await.unwrap().is_empty(), "空入参应返回空 map");
    }
}
