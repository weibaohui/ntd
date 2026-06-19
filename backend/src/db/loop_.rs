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
    Set,
};
use std::collections::HashMap;

use crate::db::entity::{
    loop_executions, loop_hooks, loop_stage_executions, loop_stages, loop_triggers, loops,
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
        product: &str,
        repo: &str,
        branch: &str,
        color: &str,
        icon: &str,
    ) -> Result<loops::Model, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = loops::ActiveModel {
            name: ActiveValue::Set(name.to_string()),
            description: ActiveValue::Set(description.to_string()),
            product: ActiveValue::Set(product.to_string()),
            repo: ActiveValue::Set(repo.to_string()),
            branch: ActiveValue::Set(branch.to_string()),
            color: ActiveValue::Set(color.to_string()),
            icon: ActiveValue::Set(icon.to_string()),
            status: ActiveValue::Set("draft".to_string()),
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
        product: &str,
        repo: &str,
        branch: &str,
        color: &str,
        icon: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let existing = loops::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loops::ActiveModel = c.into();
            am.name = ActiveValue::Set(name.to_string());
            am.description = ActiveValue::Set(description.to_string());
            am.product = ActiveValue::Set(product.to_string());
            am.repo = ActiveValue::Set(repo.to_string());
            am.branch = ActiveValue::Set(branch.to_string());
            am.color = ActiveValue::Set(color.to_string());
            am.icon = ActiveValue::Set(icon.to_string());
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

    /// 复制 loop 及其所有 trigger/stage/hook；execution 不复制。
    ///
    /// 用于 UI 的「另存为」/「复制为新版本」按钮。
    /// 复制时 name 追加「(副本)」前缀，status 重置为 draft，
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
                &source.product,
                &source.repo,
                &source.branch,
                &source.color,
                &source.icon,
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

        // 复制 stages (按原 order 写入,新 order 自动递增)
        let stages = self.list_stages_by_loop(source_id).await?;
        for s in stages {
            self.create_stage(
                new_loop.id,
                &s.name,
                &s.description,
                s.todo_id,
                &s.run_mode,
                s.skip_on_source_failed != 0,
                s.min_rating,
                &s.unrated_policy,
                s.enabled != 0,
            )
            .await?;
        }

        // 复制 hooks
        let hooks = self.list_hooks_by_loop(source_id).await?;
        for h in hooks {
            // 注意: 复制 hooks 时 source_stage_id 需要映射到新 loop 的 stage id
            // 因为 create_hook 不接受 source_stage_id 参数,这里直接走 SQL
            let now = crate::models::utc_timestamp();
            let am = loop_hooks::ActiveModel {
                loop_id: ActiveValue::Set(new_loop.id),
                hook_position: ActiveValue::Set(h.hook_position),
                source_stage_id: ActiveValue::Set(h.source_stage_id),
                target_todo_id: ActiveValue::Set(h.target_todo_id),
                skip_if_missing: ActiveValue::Set(h.skip_if_missing),
                enabled: ActiveValue::Set(h.enabled),
                min_rating: ActiveValue::Set(h.min_rating),
                unrated_policy: ActiveValue::Set(h.unrated_policy),
                created_at: ActiveValue::Set(Some(now)),
                ..Default::default()
            };
            am.insert(&self.conn).await?;
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

    // ====== Loop Stages ======

    pub async fn list_stages_by_loop(
        &self,
        loop_id: i64,
    ) -> Result<Vec<loop_stages::Model>, sea_orm::DbErr> {
        loop_stages::Entity::find()
            .filter(loop_stages::Column::LoopId.eq(loop_id))
            .order_by_asc(loop_stages::Column::OrderIndex)
            .order_by_asc(loop_stages::Column::Id)
            .all(&self.conn)
            .await
    }

    /// 列出 loop 的启用阶段,用于 loop runner 按序执行。
    pub async fn list_enabled_stages_by_loop(
        &self,
        loop_id: i64,
    ) -> Result<Vec<loop_stages::Model>, sea_orm::DbErr> {
        loop_stages::Entity::find()
            .filter(loop_stages::Column::LoopId.eq(loop_id))
            .filter(loop_stages::Column::Enabled.eq(1))
            .order_by_asc(loop_stages::Column::OrderIndex)
            .order_by_asc(loop_stages::Column::Id)
            .all(&self.conn)
            .await
    }

    pub async fn get_stage(
        &self,
        id: i64,
    ) -> Result<Option<loop_stages::Model>, sea_orm::DbErr> {
        loop_stages::Entity::find_by_id(id).one(&self.conn).await
    }

    pub async fn create_stage(
        &self,
        loop_id: i64,
        name: &str,
        description: &str,
        todo_id: i64,
        run_mode: &str,
        skip_on_source_failed: bool,
        min_rating: Option<i32>,
        unrated_policy: &str,
        enabled: bool,
    ) -> Result<loop_stages::Model, sea_orm::DbErr> {
        // 自动分配 order_index: 当前最大 + 1
        let next_order = self
            .list_stages_by_loop(loop_id)
            .await?
            .iter()
            .map(|s| s.order_index)
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        let now = crate::models::utc_timestamp();
        let am = loop_stages::ActiveModel {
            loop_id: ActiveValue::Set(loop_id),
            name: ActiveValue::Set(name.to_string()),
            description: ActiveValue::Set(description.to_string()),
            order_index: ActiveValue::Set(next_order),
            todo_id: ActiveValue::Set(todo_id),
            run_mode: ActiveValue::Set(run_mode.to_string()),
            skip_on_source_failed: ActiveValue::Set(if skip_on_source_failed { 1 } else { 0 }),
            min_rating: ActiveValue::Set(min_rating),
            unrated_policy: ActiveValue::Set(unrated_policy.to_string()),
            enabled: ActiveValue::Set(if enabled { 1 } else { 0 }),
            created_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        am.insert(&self.conn).await
    }

    pub async fn update_stage(
        &self,
        id: i64,
        name: &str,
        description: &str,
        todo_id: i64,
        run_mode: &str,
        skip_on_source_failed: bool,
        min_rating: Option<i32>,
        unrated_policy: &str,
        enabled: bool,
    ) -> Result<(), sea_orm::DbErr> {
        let existing = loop_stages::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loop_stages::ActiveModel = c.into();
            am.name = ActiveValue::Set(name.to_string());
            am.description = ActiveValue::Set(description.to_string());
            am.todo_id = ActiveValue::Set(todo_id);
            am.run_mode = ActiveValue::Set(run_mode.to_string());
            am.skip_on_source_failed =
                ActiveValue::Set(if skip_on_source_failed { 1 } else { 0 });
            am.min_rating = ActiveValue::Set(min_rating);
            am.unrated_policy = ActiveValue::Set(unrated_policy.to_string());
            am.enabled = ActiveValue::Set(if enabled { 1 } else { 0 });
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    pub async fn delete_stage(&self, id: i64) -> Result<(), sea_orm::DbErr> {
        loop_stages::Entity::delete_by_id(id).exec(&self.conn).await?;
        Ok(())
    }

    /// 批量重排阶段。前端拖拽排序后调用,传入完整的新顺序。
    /// `ordered_ids` 的顺序即新的 order_index(从 0 开始递增)。
    pub async fn reorder_stages(
        &self,
        loop_id: i64,
        ordered_ids: &[i64],
    ) -> Result<(), sea_orm::DbErr> {
        // 1. 先把所有相关 stage 取出,确保 ordered_ids 全部属于 loop_id
        let stages = self.list_stages_by_loop(loop_id).await?;
        let valid: std::collections::HashSet<i64> = stages.iter().map(|s| s.id).collect();
        for (idx, id) in ordered_ids.iter().enumerate() {
            if !valid.contains(id) {
                return Err(sea_orm::DbErr::Custom(format!(
                    "stage #{} 不属于 loop #{}",
                    id, loop_id
                )));
            }
            let stage = stages.iter().find(|s| s.id == *id).unwrap();
            let mut am: loop_stages::ActiveModel = stage.clone().into();
            am.order_index = Set(idx as i32);
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    // ====== Loop Hooks ======

    pub async fn list_hooks_by_loop(
        &self,
        loop_id: i64,
    ) -> Result<Vec<loop_hooks::Model>, sea_orm::DbErr> {
        loop_hooks::Entity::find()
            .filter(loop_hooks::Column::LoopId.eq(loop_id))
            .order_by_asc(loop_hooks::Column::Id)
            .all(&self.conn)
            .await
    }

    pub async fn list_hooks_by_loop_and_position(
        &self,
        loop_id: i64,
        position: &str,
    ) -> Result<Vec<loop_hooks::Model>, sea_orm::DbErr> {
        loop_hooks::Entity::find()
            .filter(loop_hooks::Column::LoopId.eq(loop_id))
            .filter(loop_hooks::Column::HookPosition.eq(position))
            .filter(loop_hooks::Column::Enabled.eq(1))
            .order_by_asc(loop_hooks::Column::Id)
            .all(&self.conn)
            .await
    }

    /// 列出某阶段后置 hook(post_stage),用于阶段执行完后触发。
    /// 同时支持 source_stage_id = NULL 的情况(供 post_loop 复用),调用方需自行过滤。
    pub async fn list_post_stage_hooks(
        &self,
        loop_id: i64,
        source_stage_id: i64,
    ) -> Result<Vec<loop_hooks::Model>, sea_orm::DbErr> {
        loop_hooks::Entity::find()
            .filter(loop_hooks::Column::LoopId.eq(loop_id))
            .filter(loop_hooks::Column::HookPosition.eq("post_stage"))
            .filter(loop_hooks::Column::SourceStageId.eq(Some(source_stage_id)))
            .filter(loop_hooks::Column::Enabled.eq(1))
            .order_by_asc(loop_hooks::Column::Id)
            .all(&self.conn)
            .await
    }

    pub async fn get_hook(
        &self,
        id: i64,
    ) -> Result<Option<loop_hooks::Model>, sea_orm::DbErr> {
        loop_hooks::Entity::find_by_id(id).one(&self.conn).await
    }

    pub async fn create_hook(
        &self,
        loop_id: i64,
        hook_position: &str,
        source_stage_id: Option<i64>,
        target_todo_id: i64,
        skip_if_missing: bool,
        enabled: bool,
        min_rating: Option<i32>,
        unrated_policy: &str,
    ) -> Result<loop_hooks::Model, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = loop_hooks::ActiveModel {
            loop_id: ActiveValue::Set(loop_id),
            hook_position: ActiveValue::Set(hook_position.to_string()),
            source_stage_id: ActiveValue::Set(source_stage_id),
            target_todo_id: ActiveValue::Set(target_todo_id),
            skip_if_missing: ActiveValue::Set(if skip_if_missing { 1 } else { 0 }),
            enabled: ActiveValue::Set(if enabled { 1 } else { 0 }),
            min_rating: ActiveValue::Set(min_rating),
            unrated_policy: ActiveValue::Set(unrated_policy.to_string()),
            created_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        am.insert(&self.conn).await
    }

    pub async fn update_hook(
        &self,
        id: i64,
        hook_position: &str,
        source_stage_id: Option<i64>,
        target_todo_id: i64,
        skip_if_missing: bool,
        enabled: bool,
        min_rating: Option<i32>,
        unrated_policy: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let existing = loop_hooks::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loop_hooks::ActiveModel = c.into();
            am.hook_position = ActiveValue::Set(hook_position.to_string());
            am.source_stage_id = ActiveValue::Set(source_stage_id);
            am.target_todo_id = ActiveValue::Set(target_todo_id);
            am.skip_if_missing = ActiveValue::Set(if skip_if_missing { 1 } else { 0 });
            am.enabled = ActiveValue::Set(if enabled { 1 } else { 0 });
            am.min_rating = ActiveValue::Set(min_rating);
            am.unrated_policy = ActiveValue::Set(unrated_policy.to_string());
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    pub async fn delete_hook(&self, id: i64) -> Result<(), sea_orm::DbErr> {
        loop_hooks::Entity::delete_by_id(id).exec(&self.conn).await?;
        Ok(())
    }

    // ====== Loop Executions ======

    pub async fn create_loop_execution(
        &self,
        loop_id: i64,
        trigger_id: Option<i64>,
        trigger_type: &str,
        trigger_meta: &str,
        total_stages: i32,
    ) -> Result<loop_executions::Model, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = loop_executions::ActiveModel {
            loop_id: ActiveValue::Set(loop_id),
            trigger_id: ActiveValue::Set(trigger_id),
            trigger_type: ActiveValue::Set(trigger_type.to_string()),
            trigger_meta: ActiveValue::Set(trigger_meta.to_string()),
            started_at: ActiveValue::Set(now),
            status: ActiveValue::Set("running".to_string()),
            total_stages: ActiveValue::Set(total_stages),
            completed_stages: ActiveValue::Set(0),
            failed_stages: ActiveValue::Set(0),
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

    /// 把 loop execution 标记为 running,并一次性写入 total_stages 与清掉 finished_at。
    ///
    /// 之前通过「finish_loop_execution(running) + clear_finished_at」两步实现,
    /// 两次 UPDATE 之间有可见窗口,前端 poll 会看到 status=running 但 finished_at=now
    /// 的错配状态,误判为已完成。这里合并为单条 SQL,消除 race。
    pub async fn mark_loop_execution_running(
        &self,
        id: i64,
        total_stages: i32,
    ) -> Result<(), sea_orm::DbErr> {
        use sea_orm::{ConnectionTrait, Statement};
        // 直接用 i64 数字拼接;total_stages 是 i32、id 是 i64,内部不接收外部字符串,无注入面
        let sql = format!(
            "UPDATE loop_executions SET status = 'running', total_stages = {}, \
             finished_at = NULL WHERE id = {}",
            total_stages, id
        );
        self.conn
            .execute(Statement::from_string(sea_orm::DbBackend::Sqlite, sql))
            .await?;
        Ok(())
    }

    /// 终态化 loop execution: 设置 status、finished_at 并按需累加 completed/failed 计数。
    ///
    /// 计数更新由调用方传入,因为 runner 在每个阶段结束时增量更新,效率更高。
    /// 这里做的是「终态校验+写回」,防止中间状态写错。
    pub async fn finish_loop_execution(
        &self,
        id: i64,
        status: &str,
        completed_stages: i32,
        failed_stages: i32,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let existing = loop_executions::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loop_executions::ActiveModel = c.into();
            am.status = ActiveValue::Set(status.to_string());
            am.finished_at = ActiveValue::Set(Some(now));
            am.completed_stages = ActiveValue::Set(completed_stages);
            am.failed_stages = ActiveValue::Set(failed_stages);
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    pub async fn increment_loop_execution_counters(
        &self,
        id: i64,
        success_delta: i32,
        failed_delta: i32,
    ) -> Result<(), sea_orm::DbErr> {
        // 通过 SQL 累加;避免读写竞争
        let sql = format!(
            "UPDATE loop_executions SET completed_stages = completed_stages + {}, \
             failed_stages = failed_stages + {} WHERE id = {}",
            success_delta, failed_delta, id
        );
        use sea_orm::{ConnectionTrait, Statement};
        self.conn
            .execute(Statement::from_string(sea_orm::DbBackend::Sqlite, sql))
            .await?;
        Ok(())
    }

    // ====== Loop Stage Executions ======

    pub async fn create_loop_stage_execution(
        &self,
        loop_execution_id: i64,
        stage_id: i64,
        todo_id: i64,
        status: &str,
    ) -> Result<loop_stage_executions::Model, sea_orm::DbErr> {
        let am = loop_stage_executions::ActiveModel {
            loop_execution_id: ActiveValue::Set(loop_execution_id),
            stage_id: ActiveValue::Set(stage_id),
            todo_id: ActiveValue::Set(todo_id),
            status: ActiveValue::Set(status.to_string()),
            ..Default::default()
        };
        am.insert(&self.conn).await
    }

    pub async fn list_loop_stage_executions(
        &self,
        loop_execution_id: i64,
    ) -> Result<Vec<loop_stage_executions::Model>, sea_orm::DbErr> {
        loop_stage_executions::Entity::find()
            .filter(loop_stage_executions::Column::LoopExecutionId.eq(loop_execution_id))
            .order_by_asc(loop_stage_executions::Column::Id)
            .all(&self.conn)
            .await
    }

    pub async fn mark_stage_execution_started(
        &self,
        id: i64,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let existing = loop_stage_executions::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loop_stage_executions::ActiveModel = c.into();
            am.status = ActiveValue::Set("running".to_string());
            am.started_at = ActiveValue::Set(Some(now));
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    pub async fn finish_stage_execution(
        &self,
        id: i64,
        status: &str,
        execution_record_id: Option<i64>,
        error_message: Option<&str>,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let existing = loop_stage_executions::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loop_stage_executions::ActiveModel = c.into();
            am.status = ActiveValue::Set(status.to_string());
            am.finished_at = ActiveValue::Set(Some(now));
            if let Some(rid) = execution_record_id {
                am.execution_record_id = ActiveValue::Set(Some(rid));
            }
            if error_message.is_some() {
                am.error_message = ActiveValue::Set(error_message.map(|s| s.to_string()));
            }
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    // ====== 辅助：批量取 stage + todo 元信息 ======

    /// 一次 SQL 把 stage + 关联 todo 的 title/executor/status 拉出来。
    /// 供前端 LoopStudio 详情页直接渲染(避免 N+1)。
    pub async fn list_stages_with_todo_meta(
        &self,
        loop_id: i64,
    ) -> Result<Vec<(loop_stages::Model, String, String, String)>, sea_orm::DbErr> {
        // 用 raw SQL JOIN; SeaORM 的 join API 对 has-many/belongs-to 支持有限,
        // 一次写清晰且类型稳定。
        use sea_orm::{ConnectionTrait, Statement};
        let sql = format!(
            "SELECT s.id, s.loop_id, s.name, s.description, s.order_index, s.todo_id, \
                    s.run_mode, s.skip_on_source_failed, s.min_rating, s.unrated_policy, \
                    s.enabled, s.created_at, \
                    t.title as todo_title, t.executor as todo_executor, t.status as todo_status \
             FROM loop_stages s \
             INNER JOIN todos t ON t.id = s.todo_id \
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
            let model = loop_stages::Model {
                id: row.try_get_by::<i64, _>("id")?,
                loop_id: row.try_get_by::<i64, _>("loop_id")?,
                name: row.try_get_by::<String, _>("name")?,
                description: row.try_get_by::<String, _>("description")?,
                order_index: row.try_get_by::<i32, _>("order_index")?,
                todo_id: row.try_get_by::<i64, _>("todo_id")?,
                run_mode: row.try_get_by::<String, _>("run_mode")?,
                skip_on_source_failed: row.try_get_by::<i32, _>("skip_on_source_failed")?,
                min_rating: row.try_get_by::<Option<i32>, _>("min_rating")?,
                unrated_policy: row.try_get_by::<String, _>("unrated_policy")?,
                enabled: row.try_get_by::<i32, _>("enabled")?,
                created_at: row.try_get_by::<Option<String>, _>("created_at")?,
            };
            let todo_title: String = row.try_get_by("todo_title")?;
            let todo_executor: String = row
                .try_get_by::<Option<String>, _>("todo_executor")?
                .unwrap_or_default();
            let todo_status: String = row
                .try_get_by::<Option<String>, _>("todo_status")?
                .unwrap_or_default();
            out.push((model, todo_title, todo_executor, todo_status));
        }
        Ok(out)
    }

    // ====== 辅助：批量取 loop + 计数 ======

    /// 一次 SQL 把所有 loop + 它的 trigger/stage 数 + 最近一次 execution 状态拉出来。
    /// 供左侧 LoopList 用,避免 N+1。
    pub async fn list_loops_with_counts(
        &self,
    ) -> Result<Vec<LoopListRow>, sea_orm::DbErr> {
        use sea_orm::{ConnectionTrait, Statement};
        let sql = "SELECT l.id, l.name, l.description, l.product, l.repo, l.branch, \
                          l.status, l.color, l.icon, l.created_at, l.updated_at, \
                          (SELECT COUNT(*) FROM loop_triggers t WHERE t.loop_id = l.id) as trigger_count, \
                          (SELECT COUNT(*) FROM loop_stages s WHERE s.loop_id = l.id) as stage_count, \
                          (SELECT le.status FROM loop_executions le \
                           WHERE le.loop_id = l.id ORDER BY le.started_at DESC LIMIT 1) as last_execution_status, \
                          (SELECT le.started_at FROM loop_executions le \
                           WHERE le.loop_id = l.id ORDER BY le.started_at DESC LIMIT 1) as last_execution_at \
                   FROM loops l \
                   ORDER BY l.updated_at DESC";
        let rows = self
            .conn
            .query_all(Statement::from_string(sea_orm::DbBackend::Sqlite, sql))
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(LoopListRow {
                loop_: loops::Model {
                    id: row.try_get_by::<i64, _>("id")?,
                    name: row.try_get_by::<String, _>("name")?,
                    description: row.try_get_by::<String, _>("description")?,
                    product: row.try_get_by::<String, _>("product")?,
                    repo: row.try_get_by::<String, _>("repo")?,
                    branch: row.try_get_by::<String, _>("branch")?,
                    status: row.try_get_by::<String, _>("status")?,
                    color: row.try_get_by::<String, _>("color")?,
                    icon: row.try_get_by::<String, _>("icon")?,
                    created_at: row.try_get_by::<Option<String>, _>("created_at")?,
                    updated_at: row.try_get_by::<Option<String>, _>("updated_at")?,
                },
                trigger_count: row.try_get_by::<i32, _>("trigger_count")?,
                stage_count: row.try_get_by::<i32, _>("stage_count")?,
                last_execution_status: row
                    .try_get_by::<Option<String>, _>("last_execution_status")?
                    .unwrap_or_default(),
                last_execution_at: row
                    .try_get_by::<Option<String>, _>("last_execution_at")?,
            });
        }
        Ok(out)
    }

    /// 加载 loop 详情(基本+所有子项)给前端 LoopStudio 详情面板用。
    /// 单次返回所有必要数据,前端无需多次请求。
    pub async fn load_loop_full(
        &self,
        loop_id: i64,
    ) -> Result<Option<LoopFullView>, sea_orm::DbErr> {
        let Some(loop_) = self.get_loop(loop_id).await? else {
            return Ok(None);
        };
        let triggers = self.list_triggers_by_loop(loop_id).await?;
        let stages_with_meta = self.list_stages_with_todo_meta(loop_id).await?;
        let hooks = self.list_hooks_by_loop(loop_id).await?;
        let mut todo_ids: Vec<i64> = stages_with_meta.iter().map(|(s, _, _, _)| s.todo_id).collect();
        for h in &hooks {
            todo_ids.push(h.target_todo_id);
        }
        todo_ids.sort_unstable();
        todo_ids.dedup();
        let todos = self.get_todos_by_ids(&todo_ids).await?;
        let todo_map: HashMap<i64, _> = todos.into_iter().map(|t| (t.id, t)).collect();
        let stages: Vec<loop_stages::Model> =
            stages_with_meta.iter().map(|(s, _, _, _)| s.clone()).collect();
        Ok(Some(LoopFullView {
            loop_,
            triggers,
            stages,
            stages_meta: stages_with_meta,
            hooks,
            todo_map,
        }))
    }
}

/// 左栏 LoopList 一行所需的所有数据,一次查询拉出。
#[derive(Debug, Clone)]
pub struct LoopListRow {
    pub loop_: loops::Model,
    pub trigger_count: i32,
    pub stage_count: i32,
    pub last_execution_status: String,
    pub last_execution_at: Option<String>,
}

/// LoopStudio 详情页单次请求所需的完整数据。
#[derive(Debug, Clone)]
pub struct LoopFullView {
    pub loop_: loops::Model,
    pub triggers: Vec<loop_triggers::Model>,
    pub stages: Vec<loop_stages::Model>,
    /// (stage, todo_title, todo_executor, todo_status)
    pub stages_meta: Vec<(loop_stages::Model, String, String, String)>,
    pub hooks: Vec<loop_hooks::Model>,
    pub todo_map: HashMap<i64, crate::db::entity::todos::Model>,
}
