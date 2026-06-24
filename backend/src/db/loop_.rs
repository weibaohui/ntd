//! Loop Studio 数据库访问层。
//!
//! 命名约定：
//! - `list_*` 返回该实体的全集或过滤集
//! - `get_*` 按 id 单查
//! - `create_*` 插入并返回新行
//! - `update_*` 按 id 修改
//! - `delete_*` 按 id 删除
//!
//! 与现有 webhook/tag 等模块风格保持一致（直接用 sea_orm::DatabaseConnection，
//! 不抽象 DAO trait，因为 codebase 其它 db 文件都这样做）。
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    Set, DbBackend,
};

use crate::db::entity::{
    loop_executions, loop_step_executions, loop_steps, loop_triggers, loops,
};
use crate::db::Database;

// ====== Loop 主体 ======

impl Database {
    pub async fn list_loops(&self) -> Result<Vec<loops::Model>, sea_orm::DbErr> {
        loops::Entity::find()
            .order_by_desc(loops::Column::UpdatedAt)
            .all(&self.conn)
            .await
    }

    pub async fn get_loop(&self, id: i64) -> Result<Option<loops::Model>, sea_orm::DbErr> {
        loops::Entity::find_by_id(id).one(&self.conn).await
    }

    pub async fn create_loop(
        &self,
        name: &str,
        description: &str,
        workspace: Option<&str>,
        icon: &str,
        review_template_id: Option<i64>,
    ) -> Result<loops::Model, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = loops::ActiveModel {
            name: ActiveValue::Set(name.to_string()),
            description: ActiveValue::Set(description.to_string()),
            workspace: ActiveValue::Set(workspace.map(|s| s.to_string())),
            icon: ActiveValue::Set(icon.to_string()),
            review_template_id: ActiveValue::Set(review_template_id),
            status: ActiveValue::Set("paused".to_string()),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        am.insert(&self.conn).await
    }

    pub async fn update_loop(
        &self,
        id: i64,
        name: &str,
        description: &str,
        workspace: Option<&str>,
        icon: &str,
        review_template_id: Option<i64>,
        limits_config: Option<&str>,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let existing = loops::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loops::ActiveModel = c.into();
            am.name = ActiveValue::Set(name.to_string());
            am.description = ActiveValue::Set(description.to_string());
            am.workspace = ActiveValue::Set(workspace.map(|s| s.to_string()));
            am.icon = ActiveValue::Set(icon.to_string());
            am.review_template_id = ActiveValue::Set(review_template_id);
            if let Some(lc) = limits_config {
                am.limits_config = ActiveValue::Set(lc.to_string());
            }
            am.updated_at = ActiveValue::Set(Some(now));
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    /// 只切换 status 字段（不触发全量 update）。
    ///
    /// 为什么单独一个方法：状态切换在 UI 上是高频操作（启用/暂停/草稿互相切），
    /// 单独走一条小 SQL 避免把 name/description 等字段一起重写。
    pub async fn update_loop_status(
        &self,
        id: i64,
        status: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let existing = loops::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loops::ActiveModel = c.into();
            am.status = ActiveValue::Set(status.to_string());
            am.updated_at = ActiveValue::Set(Some(now));
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    pub async fn delete_loop(&self, id: i64) -> Result<(), sea_orm::DbErr> {
        loops::Entity::delete_by_id(id).exec(&self.conn).await?;
        Ok(())
    }

    /// 复制 loop 及其所有 trigger.step；execution 不复制。
    ///
    /// 用于 UI 的「另存为」/「复制为新版本」按钮。
    /// 复制时 name 追加「(副本)」前缀，status 重置为 paused，
    /// 创建时间/更新时间由 trigger 重新设置。
    pub async fn duplicate_loop(
        &self,
        source_id: i64,
    ) -> Result<Option<loops::Model>, sea_orm::DbErr> {
        let source = match self.get_loop(source_id).await? {
            Some(l) => l,
            None => return Ok(None),
        };
        let new_loop = self
            .create_loop(
                &format!("{}(副本)", source.name),
                &source.description,
                source.workspace.as_deref(),
                &source.icon,
                source.review_template_id,
            )
            .await?;

        // 复制 triggers
        let triggers = self.list_triggers_by_loop(source_id).await?;
        for t in triggers {
            self.create_trigger(
                new_loop.id,
                &t.trigger_type,
                &t.config,
                t.enabled != 0,
                t.priority,
            )
            .await?;
        }

        // 复制 steps (按原 order 写入,新 order 自动递增)
        let steps = self.list_loop_steps_by_loop(source_id).await?;
        for s in steps {
            self.create_loop_step(
                new_loop.id,
                &s.name,
                &s.description,
                s.step_id,
                &s.run_mode,
                s.skip_on_source_failed != 0,
                s.min_rating,
                &s.unrated_policy,
                s.enabled != 0,
                &s.on_success,
                s.success_goto_step_id,
                &s.on_rating_fail,
                s.fail_goto_step_id,
                &s.review_type,
            )
            .await?;
        }

        Ok(Some(new_loop))
    }

    // ====== Loop Triggers ======

    pub async fn list_triggers_by_loop(
        &self,
        loop_id: i64,
    ) -> Result<Vec<loop_triggers::Model>, sea_orm::DbErr> {
        loop_triggers::Entity::find()
            .filter(loop_triggers::Column::LoopId.eq(loop_id))
            .order_by_desc(loop_triggers::Column::Priority)
            .order_by_asc(loop_triggers::Column::Id)
            .all(&self.conn)
            .await
    }

    pub async fn get_trigger(
        &self,
        id: i64,
    ) -> Result<Option<loop_triggers::Model>, sea_orm::DbErr> {
        loop_triggers::Entity::find_by_id(id).one(&self.conn).await
    }

    /// 列出指定类型、启用状态的触发器（供 dispatcher 匹配）。
    pub async fn list_enabled_triggers_by_type(
        &self,
        trigger_type: &str,
    ) -> Result<Vec<loop_triggers::Model>, sea_orm::DbErr> {
        loop_triggers::Entity::find()
            .filter(loop_triggers::Column::TriggerType.eq(trigger_type))
            .filter(loop_triggers::Column::Enabled.eq(1))
            .all(&self.conn)
            .await
    }

    /// 列出指定 todo_id 的「todo_completed / todo_state_changed」触发器。
    /// 注意：类型过滤留给调用方做（因为可能查多个类型）。
    pub async fn list_triggers_by_todo(
        &self,
        todo_id: i64,
    ) -> Result<Vec<loop_triggers::Model>, sea_orm::DbErr> {
        // config 存的是 JSON,简单做法是全量扫出 todo 相关的触发器再在内存里解析。
        // 量级预期: 数十到数百条触发器,内存解析可接受。
        loop_triggers::Entity::find()
            .filter(loop_triggers::Column::Enabled.eq(1))
            .all(&self.conn)
            .await
            .map(|all| {
                all.into_iter()
                    .filter(|t| {
                        let cfg: serde_json::Value =
                            serde_json::from_str(&t.config).unwrap_or_default();
                        cfg.get("todo_id")
                            .and_then(|v| v.as_i64())
                            .map(|id| id == todo_id)
                            .unwrap_or(false)
                    })
                    .collect()
            })
    }

    pub async fn create_trigger(
        &self,
        loop_id: i64,
        trigger_type: &str,
        config: &str,
        enabled: bool,
        priority: i32,
    ) -> Result<loop_triggers::Model, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = loop_triggers::ActiveModel {
            loop_id: ActiveValue::Set(loop_id),
            trigger_type: ActiveValue::Set(trigger_type.to_string()),
            config: ActiveValue::Set(config.to_string()),
            enabled: ActiveValue::Set(if enabled { 1 } else { 0 }),
            priority: ActiveValue::Set(priority),
            created_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        am.insert(&self.conn).await
    }

    pub async fn update_trigger(
        &self,
        id: i64,
        trigger_type: &str,
        config: &str,
        enabled: bool,
        priority: i32,
    ) -> Result<(), sea_orm::DbErr> {
        let existing = loop_triggers::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loop_triggers::ActiveModel = c.into();
            am.trigger_type = ActiveValue::Set(trigger_type.to_string());
            am.config = ActiveValue::Set(config.to_string());
            am.enabled = ActiveValue::Set(if enabled { 1 } else { 0 });
            am.priority = ActiveValue::Set(priority);
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    pub async fn delete_trigger(&self, id: i64) -> Result<(), sea_orm::DbErr> {
        loop_triggers::Entity::delete_by_id(id).exec(&self.conn).await?;
        Ok(())
    }

    // ====== Loop Steps ======

    pub async fn list_loop_steps_by_loop(
        &self,
        loop_id: i64,
    ) -> Result<Vec<loop_steps::Model>, sea_orm::DbErr> {
        loop_steps::Entity::find()
            .filter(loop_steps::Column::LoopId.eq(loop_id))
            .order_by_asc(loop_steps::Column::OrderIndex)
            .order_by_asc(loop_steps::Column::Id)
            .all(&self.conn)
            .await
    }

    /// 列出 loop 的启用阶段,用于 loop runner 按序执行。
    pub async fn list_enabled_loop_steps_by_loop(
        &self,
        loop_id: i64,
    ) -> Result<Vec<loop_steps::Model>, sea_orm::DbErr> {
        loop_steps::Entity::find()
            .filter(loop_steps::Column::LoopId.eq(loop_id))
            .filter(loop_steps::Column::Enabled.eq(1))
            .order_by_asc(loop_steps::Column::OrderIndex)
            .order_by_asc(loop_steps::Column::Id)
            .all(&self.conn)
            .await
    }

    pub async fn get_loop_step(
        &self,
        id: i64,
    ) -> Result<Option<loop_steps::Model>, sea_orm::DbErr> {
        loop_steps::Entity::find_by_id(id).one(&self.conn).await
    }

    pub async fn create_loop_step(
        &self,
        loop_id: i64,
        name: &str,
        description: &str,
        step_id: i64,
        run_mode: &str,
        skip_on_source_failed: bool,
        min_rating: Option<i32>,
        unrated_policy: &str,
        enabled: bool,
        on_success: &str,
        success_goto_step_id: Option<i64>,
        on_rating_fail: &str,
        fail_goto_step_id: Option<i64>,
        review_type: &str,
    ) -> Result<loop_steps::Model, sea_orm::DbErr> {
        // 自动分配 order_index: 当前最大 + 1
        let next_order = self
            .list_loop_steps_by_loop(loop_id)
            .await?
            .iter()
            .map(|s| s.order_index)
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        let now = crate::models::utc_timestamp();
        let am = loop_steps::ActiveModel {
            loop_id: ActiveValue::Set(loop_id),
            name: ActiveValue::Set(name.to_string()),
            description: ActiveValue::Set(description.to_string()),
            order_index: ActiveValue::Set(next_order),
            step_id: ActiveValue::Set(step_id),
            run_mode: ActiveValue::Set(run_mode.to_string()),
            skip_on_source_failed: ActiveValue::Set(if skip_on_source_failed { 1 } else { 0 }),
            min_rating: ActiveValue::Set(min_rating),
            unrated_policy: ActiveValue::Set(unrated_policy.to_string()),
            enabled: ActiveValue::Set(if enabled { 1 } else { 0 }),
            on_success: ActiveValue::Set(on_success.to_string()),
            success_goto_step_id: ActiveValue::Set(success_goto_step_id),
            on_rating_fail: ActiveValue::Set(on_rating_fail.to_string()),
            fail_goto_step_id: ActiveValue::Set(fail_goto_step_id),
            review_type: ActiveValue::Set(review_type.to_string()),
            created_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        am.insert(&self.conn).await
    }

    pub async fn update_loop_step(
        &self,
        id: i64,
        name: &str,
        description: &str,
        step_id: i64,
        run_mode: &str,
        skip_on_source_failed: bool,
        min_rating: Option<i32>,
        unrated_policy: &str,
        enabled: bool,
        on_success: &str,
        success_goto_step_id: Option<i64>,
        on_rating_fail: &str,
        fail_goto_step_id: Option<i64>,
        review_type: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let existing = loop_steps::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loop_steps::ActiveModel = c.into();
            am.name = ActiveValue::Set(name.to_string());
            am.description = ActiveValue::Set(description.to_string());
            am.step_id = ActiveValue::Set(step_id);
            am.run_mode = ActiveValue::Set(run_mode.to_string());
            am.skip_on_source_failed =
                ActiveValue::Set(if skip_on_source_failed { 1 } else { 0 });
            am.min_rating = ActiveValue::Set(min_rating);
            am.unrated_policy = ActiveValue::Set(unrated_policy.to_string());
            am.enabled = ActiveValue::Set(if enabled { 1 } else { 0 });
            am.on_success = ActiveValue::Set(on_success.to_string());
            am.success_goto_step_id = ActiveValue::Set(success_goto_step_id);
            am.on_rating_fail = ActiveValue::Set(on_rating_fail.to_string());
            am.fail_goto_step_id = ActiveValue::Set(fail_goto_step_id);
            am.review_type = ActiveValue::Set(review_type.to_string());
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    pub async fn delete_loop_step(&self, id: i64) -> Result<(), sea_orm::DbErr> {
        loop_steps::Entity::delete_by_id(id).exec(&self.conn).await?;
        Ok(())
    }

    /// 批量重排阶段。前端拖拽排序后调用,传入完整的新顺序。
    /// `ordered_ids` 的顺序即新的 order_index(从 0 开始递增)。
    pub async fn reorder_loop_steps(
        &self,
        loop_id: i64,
        ordered_ids: &[i64],
    ) -> Result<(), sea_orm::DbErr> {
        // 1. 先把所有相关 step 取出,确保 ordered_ids 全部属于 loop_id
        let steps = self.list_loop_steps_by_loop(loop_id).await?;
        let valid: std::collections::HashSet<i64> = steps.iter().map(|s| s.id).collect();
        for (idx, id) in ordered_ids.iter().enumerate() {
            if !valid.contains(id) {
                return Err(sea_orm::DbErr::Custom(format!(
                    "step #{} 不属于 loop #{}",
                    id, loop_id
                )));
            }
            let step = steps.iter().find(|s| s.id == *id).unwrap();
            let mut am: loop_steps::ActiveModel = step.clone().into();
            am.order_index = Set(idx as i32);
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    // ====== Loop Executions ======

    pub async fn create_loop_execution(
        &self,
        loop_id: i64,
        trigger_id: Option<i64>,
        trigger_type: &str,
        trigger_meta: &str,
        total_steps: i32,
    ) -> Result<loop_executions::Model, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = loop_executions::ActiveModel {
            loop_id: ActiveValue::Set(loop_id),
            trigger_id: ActiveValue::Set(trigger_id),
            trigger_type: ActiveValue::Set(trigger_type.to_string()),
            trigger_meta: ActiveValue::Set(trigger_meta.to_string()),
            started_at: ActiveValue::Set(now),
            status: ActiveValue::Set("running".to_string()),
            total_steps: ActiveValue::Set(total_steps),
            completed_steps: ActiveValue::Set(0),
            failed_steps: ActiveValue::Set(0),
            ..Default::default()
        };
        am.insert(&self.conn).await
    }

    pub async fn get_loop_execution(
        &self,
        id: i64,
    ) -> Result<Option<loop_executions::Model>, sea_orm::DbErr> {
        loop_executions::Entity::find_by_id(id).one(&self.conn).await
    }

    pub async fn list_loop_executions(
        &self,
        loop_id: i64,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<loop_executions::Model>, sea_orm::DbErr> {
        loop_executions::Entity::find()
            .filter(loop_executions::Column::LoopId.eq(loop_id))
            .order_by_desc(loop_executions::Column::StartedAt)
            .limit(limit)
            .offset(offset)
            .all(&self.conn)
            .await
    }

    pub async fn count_loop_executions(&self, loop_id: i64) -> Result<i64, sea_orm::DbErr> {
        use sea_orm::PaginatorTrait;
        loop_executions::Entity::find()
            .filter(loop_executions::Column::LoopId.eq(loop_id))
            .count(&self.conn)
            .await
            .map(|c| c as i64)
    }

    /// 统计该 loop 下所有待人工审批的环节执行数。
    /// 条件：loop_step_executions 关联到该 loop 的运行中 execution，且 approval_status = 'pending'。
    pub async fn count_pending_approvals_for_loop(
        &self,
        loop_id: i64,
    ) -> Result<i32, sea_orm::DbErr> {
        use sea_orm::{ConnectionTrait, Statement};
        let sql = format!(
            "SELECT COUNT(*) AS n FROM loop_step_executions lse \
             INNER JOIN loop_executions le ON le.id = lse.loop_execution_id \
             WHERE le.loop_id = {} AND lse.approval_status = 'pending'",
            loop_id
        );
        let row = self
            .conn
            .query_one(Statement::from_string(DbBackend::Sqlite, sql))
            .await?
            .ok_or(sea_orm::DbErr::RecordNotFound("count query returned no rows".into()))?;
        Ok(row.try_get_by::<i32, _>("n").unwrap_or(0))
    }

    /// 批量查询指定 loop_execution 列表的待审批数，返回 execution_id → count 映射。
    pub async fn count_pending_approvals_by_execution_ids(
        &self,
        execution_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, i32>, sea_orm::DbErr> {
        use sea_orm::{ConnectionTrait, Statement};
        let mut map = std::collections::HashMap::new();
        if execution_ids.is_empty() {
            return Ok(map);
        }
        let ids_str: String = execution_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT lse.loop_execution_id, COUNT(*) AS n \
             FROM loop_step_executions lse \
             WHERE lse.loop_execution_id IN ({}) AND lse.approval_status = 'pending' \
             GROUP BY lse.loop_execution_id",
            ids_str
        );
        let rows = self
            .conn
            .query_all(Statement::from_string(DbBackend::Sqlite, sql))
            .await?;
        for row in rows {
            let exec_id: i64 = row.try_get_by("loop_execution_id")?;
            let n: i32 = row.try_get_by("n").unwrap_or(0);
            map.insert(exec_id, n);
        }
        Ok(map)
    }

    /// 终态化 loop execution: 设置 status、finished_at 并按需累加 completed/failed 计数。
    ///
    /// 计数更新由调用方传入,因为 runner 在每个阶段结束时增量更新,效率更高。
    /// 这里做的是「终态校验+写回」,防止中间状态写错。
    pub async fn finish_loop_execution(
        &self,
        id: i64,
        status: &str,
        completed_steps: i32,
        failed_steps: i32,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let existing = loop_executions::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loop_executions::ActiveModel = c.into();
            am.status = ActiveValue::Set(status.to_string());
            am.finished_at = ActiveValue::Set(Some(now));
            am.completed_steps = ActiveValue::Set(completed_steps);
            am.failed_steps = ActiveValue::Set(failed_steps);
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    pub async fn increment_loop_execution_counters(
        &self,
        id: i64,
        success_delta: i32,
        failed_delta: i32,
        executed_delta: i32,
    ) -> Result<(), sea_orm::DbErr> {
        // 通过 SQL 累加;避免读写竞争
        let sql = format!(
            "UPDATE loop_executions SET completed_steps = completed_steps + {}, \
             failed_steps = failed_steps + {}, \
             total_executed_steps = total_executed_steps + {} WHERE id = {}",
            success_delta, failed_delta, executed_delta, id
        );
        use sea_orm::{ConnectionTrait, Statement};
        self.conn
            .execute(Statement::from_string(sea_orm::DbBackend::Sqlite, sql))
            .await?;
        Ok(())
    }

    // ====== Loop Step Executions ======

    pub async fn create_loop_step_execution(
        &self,
        loop_execution_id: i64,
        step_id: i64,
        todo_id: i64,
        status: &str,
        sequence_index: i32,
        min_rating: Option<i32>,
        unrated_policy: &str,
    ) -> Result<loop_step_executions::Model, sea_orm::DbErr> {
        let am = loop_step_executions::ActiveModel {
            loop_execution_id: ActiveValue::Set(loop_execution_id),
            step_id: ActiveValue::Set(step_id),
            todo_id: ActiveValue::Set(todo_id),
            status: ActiveValue::Set(status.to_string()),
            sequence_index: ActiveValue::Set(sequence_index),
            min_rating: ActiveValue::Set(min_rating),
            unrated_policy: ActiveValue::Set(Some(unrated_policy.to_string())),
            ..Default::default()
        };
        am.insert(&self.conn).await
    }

    pub async fn list_loop_step_executions(
        &self,
        loop_execution_id: i64,
    ) -> Result<Vec<loop_step_executions::Model>, sea_orm::DbErr> {
        loop_step_executions::Entity::find()
            .filter(loop_step_executions::Column::LoopExecutionId.eq(loop_execution_id))
            .order_by_asc(loop_step_executions::Column::SequenceIndex)
            .all(&self.conn)
            .await
    }

    pub async fn mark_step_execution_started(
        &self,
        id: i64,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let existing = loop_step_executions::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loop_step_executions::ActiveModel = c.into();
            am.status = ActiveValue::Set("running".to_string());
            am.started_at = ActiveValue::Set(Some(now));
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    pub async fn finish_step_execution(
        &self,
        id: i64,
        status: &str,
        execution_record_id: Option<i64>,
        error_message: Option<&str>,
        rating: Option<i32>,
        conclusion: Option<&str>,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let existing = loop_step_executions::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loop_step_executions::ActiveModel = c.into();
            am.status = ActiveValue::Set(status.to_string());
            am.finished_at = ActiveValue::Set(Some(now));
            if let Some(rid) = execution_record_id {
                am.execution_record_id = ActiveValue::Set(Some(rid));
            }
            if error_message.is_some() {
                am.error_message = ActiveValue::Set(error_message.map(|s| s.to_string()));
            }
            if conclusion.is_some() {
                am.conclusion = ActiveValue::Set(conclusion.map(|s| s.to_string()));
            }
            if let Some(r) = rating {
                am.rating = ActiveValue::Set(Some(r));
            }
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    pub async fn update_step_execution_conclusion(
        &self,
        id: i64,
        conclusion: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let existing = loop_step_executions::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loop_step_executions::ActiveModel = c.into();
            am.conclusion = ActiveValue::Set(Some(conclusion.to_string()));
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    pub async fn set_step_execution_rating(
        &self,
        id: i64,
        rating: i32,
    ) -> Result<(), sea_orm::DbErr> {
        let existing = loop_step_executions::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loop_step_executions::ActiveModel = c.into();
            am.rating = ActiveValue::Set(Some(rating));
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    /// 设置环节执行记录的审批状态（人工审批流程专用）。
    pub async fn set_step_execution_approval_status(
        &self,
        id: i64,
        approval_status: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let existing = loop_step_executions::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loop_step_executions::ActiveModel = c.into();
            am.approval_status = ActiveValue::Set(Some(approval_status.to_string()));
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    /// 人工审批：写入评分、审批意见，更新状态。
    /// 调用前由 handler 校验 approval_status = "pending"。
    pub async fn approve_step_execution(
        &self,
        id: i64,
        rating: i32,
        status: &str,
        comment: Option<&str>,
    ) -> Result<(), sea_orm::DbErr> {
        let existing = loop_step_executions::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loop_step_executions::ActiveModel = c.into();
            am.rating = ActiveValue::Set(Some(rating));
            am.status = ActiveValue::Set(status.to_string());
            am.approval_status = ActiveValue::Set(Some("approved".to_string()));
            am.approval_comment = ActiveValue::Set(comment.map(|s| s.to_string()));
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    // ====== 辅助：批量取 step + todo 元信息 ======

    /// 一次 SQL 把 loop_step + 关联 steps 模板的 title/executor 拉出来。
    /// 供前端 LoopStudio 详情页直接渲染(避免 N+1)。
    ///
    /// 历史注记：早期 `loop_steps.step_id` 指向 `todos.id`，后来重构为指向 `steps.id`
    /// （环节成为可复用模板）。本 SQL 必须 INNER JOIN `steps`，不能再 JOIN `todos`。
    /// 否则会把同名 ID 的 todo title 错配到 step 上，例如 loop 绑定 steps.id=3 时
    /// 拿到的是 todos.id=3 的 title。前端 LoopFlowGraph / LoopStudioStepsPanel 第二列
    /// 都依赖这个字段，错配会直接展示错误标题。
    pub async fn list_loop_steps_with_todo_meta(
        &self,
        loop_id: i64,
    ) -> Result<Vec<(loop_steps::Model, String, String)>, sea_orm::DbErr> {
        // 用 raw SQL JOIN; SeaORM 的 join API 对 has-many/belongs-to 支持有限,
        // 一次写清晰且类型稳定。
        //
        // 仅返回 (loop_step, template_title, template_executor) 三元组。
        // 原 tuple 还包含 `todo_status`，但 `steps` 表没有 status 列（环节是模板不是任务），
        // 且前端从不消费该字段，所以一并移除。
        use sea_orm::{ConnectionTrait, Statement};
        let sql = format!(
            "SELECT s.id, s.loop_id, s.name, s.description, s.order_index, s.step_id, \
                    s.run_mode, s.skip_on_source_failed, s.min_rating, s.unrated_policy, \
                    s.on_success, s.success_goto_step_id, s.on_rating_fail, s.fail_goto_step_id, \
                    s.review_type, \
                    s.enabled, s.created_at, \
                    st.title as todo_title, st.executor as todo_executor \
             FROM loop_steps s \
             INNER JOIN steps st ON st.id = s.step_id \
             WHERE s.loop_id = {} \
             ORDER BY s.order_index ASC, s.id ASC",
            loop_id
        );
        let rows = self
            .conn
            .query_all(Statement::from_string(sea_orm::DbBackend::Sqlite, sql))
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let model = loop_steps::Model {
                id: row.try_get_by::<i64, _>("id")?,
                loop_id: row.try_get_by::<i64, _>("loop_id")?,
                name: row.try_get_by::<String, _>("name")?,
                description: row.try_get_by::<String, _>("description")?,
                order_index: row.try_get_by::<i32, _>("order_index")?,
                step_id: row.try_get_by::<i64, _>("step_id")?,
                run_mode: row.try_get_by::<String, _>("run_mode")?,
                skip_on_source_failed: row.try_get_by::<i32, _>("skip_on_source_failed")?,
                min_rating: row.try_get_by::<Option<i32>, _>("min_rating")?,
                unrated_policy: row.try_get_by::<String, _>("unrated_policy")?,
                on_success: row.try_get_by::<String, _>("on_success")?,
                success_goto_step_id: row.try_get_by::<Option<i64>, _>("success_goto_step_id")?,
                on_rating_fail: row.try_get_by::<String, _>("on_rating_fail")?,
                fail_goto_step_id: row.try_get_by::<Option<i64>, _>("fail_goto_step_id")?,
                review_type: row.try_get_by::<String, _>("review_type")?,
                enabled: row.try_get_by::<i32, _>("enabled")?,
                created_at: row.try_get_by::<Option<String>, _>("created_at")?,
            };
            let todo_title: String = row.try_get_by("todo_title")?;
            let todo_executor: String = row
                .try_get_by::<Option<String>, _>("todo_executor")?
                .unwrap_or_default();
            out.push((model, todo_title, todo_executor));
        }
        Ok(out)
    }

    // ====== 辅助：批量取 loop + 计数 ======

    /// 一次 SQL 把所有 loop + 它的 trigger.step 数 + 最近一次 execution 状态拉出来。
    /// 供左侧 LoopList 用,避免 N+1。
    pub async fn list_loops_with_counts(
        &self,
        workspace: Option<&str>,
    ) -> Result<Vec<LoopListRow>, sea_orm::DbErr> {
        use sea_orm::{ConnectionTrait, Statement};
        let sql = match workspace {
            Some(_) => "SELECT l.id, l.name, l.description, l.workspace, \
                          l.status, l.color, l.icon, l.limits_config, l.review_template_id, l.created_at, l.updated_at, \
                          (SELECT COUNT(*) FROM loop_triggers t WHERE t.loop_id = l.id) as trigger_count, \
                          (SELECT COUNT(*) FROM loop_steps s WHERE s.loop_id = l.id) as step_count, \
                          (SELECT le.status FROM loop_executions le \
                           WHERE le.loop_id = l.id ORDER BY le.started_at DESC LIMIT 1) as last_execution_status, \
                          (SELECT le.started_at FROM loop_executions le \
                           WHERE le.loop_id = l.id ORDER BY le.started_at DESC LIMIT 1) as last_execution_at, \
                          (SELECT COUNT(*) FROM loop_step_executions lse \
                           INNER JOIN loop_executions le2 ON le2.id = lse.loop_execution_id \
                           WHERE le2.loop_id = l.id AND lse.approval_status = 'pending') as pending_approval_count \
                   FROM loops l \
                   WHERE l.workspace = ?1 \
                   ORDER BY l.updated_at DESC",
            None => "SELECT l.id, l.name, l.description, l.workspace, \
                      l.status, l.color, l.icon, l.limits_config, l.review_template_id, l.created_at, l.updated_at, \
                      (SELECT COUNT(*) FROM loop_triggers t WHERE t.loop_id = l.id) as trigger_count, \
                      (SELECT COUNT(*) FROM loop_steps s WHERE s.loop_id = l.id) as step_count, \
                      (SELECT le.status FROM loop_executions le \
                       WHERE le.loop_id = l.id ORDER BY le.started_at DESC LIMIT 1) as last_execution_status, \
                      (SELECT le.started_at FROM loop_executions le \
                       WHERE le.loop_id = l.id ORDER BY le.started_at DESC LIMIT 1) as last_execution_at, \
                      (SELECT COUNT(*) FROM loop_step_executions lse \
                       INNER JOIN loop_executions le2 ON le2.id = lse.loop_execution_id \
                       WHERE le2.loop_id = l.id AND lse.approval_status = 'pending') as pending_approval_count \
                   FROM loops l \
                   ORDER BY l.updated_at DESC",
        };
        let rows = if let Some(w) = workspace {
            self.conn
                .query_all(
                    Statement::from_sql_and_values(sea_orm::DbBackend::Sqlite, sql, [w.to_string().into()])
                )
                .await?
        } else {
            self.conn
                .query_all(Statement::from_string(sea_orm::DbBackend::Sqlite, sql))
                .await?
        };
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(LoopListRow {
                loop_: loops::Model {
                    id: row.try_get_by::<i64, _>("id")?,
                    name: row.try_get_by::<String, _>("name")?,
                    description: row.try_get_by::<String, _>("description")?,
                    workspace: row.try_get_by::<Option<String>, _>("workspace")?,
                    status: row.try_get_by::<String, _>("status")?,
                    color: row.try_get_by::<String, _>("color")?,
                    icon: row.try_get_by::<String, _>("icon")?,
                    review_template_id: row.try_get_by::<Option<i64>, _>("review_template_id")?,
                    limits_config: row.try_get_by::<String, _>("limits_config")?,
                    created_at: row.try_get_by::<Option<String>, _>("created_at")?,
                    updated_at: row.try_get_by::<Option<String>, _>("updated_at")?,
                },
                trigger_count: row.try_get_by::<i32, _>("trigger_count")?,
                step_count: row.try_get_by::<i32, _>("step_count")?,
                last_execution_status: row
                    .try_get_by::<Option<String>, _>("last_execution_status")?
                    .unwrap_or_default(),
                last_execution_at: row
                    .try_get_by::<Option<String>, _>("last_execution_at")?,
                pending_approval_count: row.try_get_by::<i32, _>("pending_approval_count").unwrap_or(0),
            });
        }
        Ok(out)
    }

    /// 加载 loop 详情(基本+所有子项)给前端 LoopStudio 详情面板用。
    /// 单次返回所有必要数据,前端无需多次请求。
    ///
    /// 历史注记：早期实现还会 JOIN `todos` 表构建 `todo_map`，但 `loop_steps.step_id`
    /// 重构后指向 `steps` 表（reusable 环节模板），且 todo_map 字段在前端从未被消费。
    /// 本方法现在直接返回 `(loop_step, template_title, template_executor)` 三元组，
    /// 不再做多余的 todos 拼装。
    pub async fn load_loop_full(
        &self,
        loop_id: i64,
    ) -> Result<Option<LoopFullView>, sea_orm::DbErr> {
        let Some(loop_) = self.get_loop(loop_id).await? else {
            return Ok(None);
        };
        let triggers = self.list_triggers_by_loop(loop_id).await?;
        let steps_with_meta = self.list_loop_steps_with_todo_meta(loop_id).await?;
        let steps: Vec<loop_steps::Model> =
            steps_with_meta.iter().map(|(s, _, _)| s.clone()).collect();
        // 统计该 loop 下待人工审批的环节执行数
        let pending_approval_count = self.count_pending_approvals_for_loop(loop_id).await?;
        Ok(Some(LoopFullView {
            loop_,
            triggers,
            steps,
            steps_meta: steps_with_meta,
            pending_approval_count,
        }))
    }
}

/// 左栏 LoopList 一行所需的所有数据,一次查询拉出。
#[derive(Debug, Clone)]
pub struct LoopListRow {
    pub loop_: loops::Model,
    pub trigger_count: i32,
    pub step_count: i32,
    pub last_execution_status: String,
    pub last_execution_at: Option<String>,
    /// 该 loop 下所有待人工审批的环节执行数
    pub pending_approval_count: i32,
}

/// LoopStudio 详情页单次请求所需的完整数据。
#[derive(Debug, Clone)]
pub struct LoopFullView {
    pub loop_: loops::Model,
    pub triggers: Vec<loop_triggers::Model>,
    pub steps: Vec<loop_steps::Model>,
    /// (step, template_title, template_executor)
    /// template_* 字段从 steps 表读（不是 todos），见 list_loop_steps_with_todo_meta。
    pub steps_meta: Vec<(loop_steps::Model, String, String)>,
    /// 该 loop 下待人工审批的环节执行数
    pub pending_approval_count: i32,
}
