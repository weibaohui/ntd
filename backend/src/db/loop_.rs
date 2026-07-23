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
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, Set, DbBackend,
};

use crate::db::entity::{
    loop_executions, loop_step_executions, loop_steps, loop_triggers, loops,
};
use crate::db::Database;

// ====== Loop 主体 ======

/// 把 `SELECT todo_id, loop_id, loop_name` 的结果行按 todo_id 分组成 LoopRefSummary 列表。
/// 抽出以让 get_referencing_loops_for_todos 低于 30 行。
fn group_loop_refs_by_todo(
    rows: Vec<sea_orm::QueryResult>,
) -> std::collections::HashMap<i64, Vec<crate::models::LoopRefSummary>> {
    let mut map: std::collections::HashMap<i64, Vec<crate::models::LoopRefSummary>> =
        std::collections::HashMap::new();
    for row in rows {
        let todo_id: i64 = match row.try_get_by("todo_id") {
            Ok(v) => v,
            Err(_) => continue,
        };
        let loop_id: i64 = match row.try_get_by("loop_id") {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Ok(loop_name) = row.try_get_by::<String, _>("loop_name") {
            map.entry(todo_id).or_default().push(crate::models::LoopRefSummary {
                loop_id,
                loop_name,
            });
        }
    }
    map
}

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

    /// 参数数量由 loops 表 schema 决定，无法进一步合并
    #[allow(clippy::too_many_arguments)]
    pub async fn create_loop(
        &self,
        name: &str,
        description: &str,
        workspace_id: Option<i64>,
        workspace_path: Option<&str>,
        webhook_enabled: bool,
        icon: &str,
        review_template_id: Option<i64>,
        limits_config: Option<&str>,
        abnormal_handler_todo_id: Option<i64>,
        abnormal_handler_trigger_on: &str,
    ) -> Result<loops::Model, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = loops::ActiveModel {
            name: ActiveValue::Set(name.to_string()),
            description: ActiveValue::Set(description.to_string()),
            // 双字段同源写入：handler 必须保证 id 解析得到的 path 与 workspace_path 一致，
            // 任何不一致都意味着上游解析有 bug——DAO 不再单独接受「只传 path」。
            workspace_id: ActiveValue::Set(workspace_id),
            workspace_path: ActiveValue::Set(workspace_path.map(|s| s.to_string())),
            webhook_enabled: ActiveValue::Set(webhook_enabled),
            icon: ActiveValue::Set(icon.to_string()),
            review_template_id: ActiveValue::Set(review_template_id),
            limits_config: ActiveValue::Set(limits_config.unwrap_or("{}").to_string()),
            abnormal_handler_todo_id: ActiveValue::Set(abnormal_handler_todo_id),
            abnormal_handler_trigger_on: ActiveValue::Set(abnormal_handler_trigger_on.to_string()),
            status: ActiveValue::Set("paused".to_string()),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        am.insert(&self.conn).await
    }

    /// 参数数量由 loops 表 schema 决定
    #[allow(clippy::too_many_arguments)]
    pub async fn update_loop(
        &self,
        id: i64,
        name: &str,
        description: &str,
        workspace_id: Option<i64>,
        workspace_path: Option<&str>,
        webhook_enabled: bool,
        icon: &str,
        review_template_id: Option<i64>,
        limits_config: Option<&str>,
        abnormal_handler_todo_id: Option<i64>,
        abnormal_handler_trigger_on: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let existing = loops::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loops::ActiveModel = c.into();
            am.name = ActiveValue::Set(name.to_string());
            am.description = ActiveValue::Set(description.to_string());
            // workspace 同步更新：handler 在传 id 时也会同时给 path；
            // 只传 id 不传 path 视为「只更新筛选键，cwd 保持不变」，反之亦然。
            if let Some(wid) = workspace_id {
                am.workspace_id = ActiveValue::Set(Some(wid));
            }
            if let Some(wpath) = workspace_path {
                am.workspace_path = ActiveValue::Set(Some(wpath.to_string()));
            }
            am.webhook_enabled = ActiveValue::Set(webhook_enabled);
            am.icon = ActiveValue::Set(icon.to_string());
            am.review_template_id = ActiveValue::Set(review_template_id);
            if let Some(lc) = limits_config {
                am.limits_config = ActiveValue::Set(lc.to_string());
            }
            am.abnormal_handler_todo_id = ActiveValue::Set(abnormal_handler_todo_id);
            am.abnormal_handler_trigger_on = ActiveValue::Set(abnormal_handler_trigger_on.to_string());
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

    /// 批量更新环路工作空间（移动到其他工作空间）。
    /// 连带移动步骤关联的所有 todo 到同一目标工作空间。
    ///
    /// 入参是 `project_directories.id`（唯一键）；handler 负责把 id 解析为 path 后传进来，
    /// DAO 仅按 (workspace_id, workspace_path) 双写以保证 cwd 字段与筛选字段同步。
    pub async fn batch_update_loops_workspace(
        &self,
        ids: &[i64],
        workspace_id: i64,
        workspace_path: &str,
    ) -> Result<u64, sea_orm::DbErr> {
        if ids.is_empty() || workspace_path.trim().is_empty() {
            return Ok(0);
        }
        let now = crate::models::utc_timestamp();
        let ws = workspace_path.trim();

        // 1. 更新 loops 表：按 id 筛选的回路同时写 workspace_id 与 workspace_path，
        //    保证「筛选用 id / cwd 用 path」两条路在批量迁移后保持一致。
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{}", i)).collect();
        let in_clause = placeholders.join(",");
        let ws_id_idx = ids.len() + 1;
        let ws_path_idx = ids.len() + 2;
        let now_idx = ids.len() + 3;
        let sql = format!(
            "UPDATE loops SET workspace_id = ?{ws_id_idx}, workspace_path = ?{ws_path_idx}, updated_at = ?{now_idx} WHERE id IN ({in_clause})"
        );
        let mut vals: Vec<sea_orm::Value> = ids.iter().map(|id| (*id).into()).collect();
        vals.push(workspace_id.into());
        vals.push(ws.to_string().into());
        vals.push(now.clone().into());
        let stmt = sea_orm::Statement::from_sql_and_values(sea_orm::DbBackend::Sqlite, sql, vals);
        self.conn.execute(stmt).await?.rows_affected();

        // 2. 收集所有步骤关联的 todo_id 并批量迁移
        let todo_ids = self.collect_todo_ids_from_loops(ids).await?;
        if !todo_ids.is_empty() {
            let t_placeholders: Vec<String> = (1..=todo_ids.len()).map(|i| format!("?{}", i)).collect();
            let t_in_clause = t_placeholders.join(",");
            let t_ws_id_idx = todo_ids.len() + 1;
            let t_ws_path_idx = todo_ids.len() + 2;
            let t_now_idx = todo_ids.len() + 3;
            let t_sql = format!(
                "UPDATE todos SET workspace_id = ?{t_ws_id_idx}, workspace_path = ?{t_ws_path_idx}, updated_at = ?{t_now_idx} WHERE id IN ({t_in_clause})"
            );
            let mut t_vals: Vec<sea_orm::Value> = todo_ids.iter().map(|id| (*id).into()).collect();
            t_vals.push(workspace_id.into());
            t_vals.push(ws.to_string().into());
            t_vals.push(now.into());
            let t_stmt = sea_orm::Statement::from_sql_and_values(sea_orm::DbBackend::Sqlite, t_sql, t_vals);
            self.conn.execute(t_stmt).await?;
        }

        Ok(ids.len() as u64)
    }

    /// 批量复制环路到目标工作空间。
    /// 连带复制步骤关联的 todo 到目标工作空间，并让复制后的 steps 指向新 todo。
    ///
    /// 入参是 `project_directories.id` + `workspace_path`：handler 已经把 id 解析为 path 传进来，
    /// DAO 仅做写入；拆分参数是为了让 SQL 一次完成 id + path 双写，避免再次回查。
    pub async fn batch_copy_loops_to_workspace(
        &self,
        ids: &[i64],
        target_workspace_id: i64,
        target_workspace_path: &str,
    ) -> Result<Vec<i64>, sea_orm::DbErr> {
        if ids.is_empty() || target_workspace_path.trim().is_empty() {
            return Ok(vec![]);
        }
        let ws = target_workspace_path.trim().to_string();
        let mut created_ids = Vec::new();

        // 用于记录已复制的 todo_id → new_todo_id 映射，避免同一 todo 被多个 step 重复复制
        let mut todo_copy_map: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();

        for &id in ids {
            let source = match self.get_loop(id).await? {
                Some(l) => l,
                None => continue,
            };
            let new_loop = self
                .create_loop(
                    &format!("{}(副本)", source.name),
                    &source.description,
                    Some(target_workspace_id),
                    Some(ws.as_str()),
                    source.webhook_enabled,
                    &source.icon,
                    source.review_template_id,
                    Some(source.limits_config.as_str()),
                    source.abnormal_handler_todo_id,
                    &source.abnormal_handler_trigger_on,
                )
                .await?;

            // 复制 triggers
            let triggers = self.list_triggers_by_loop(id).await?;
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

            // 复制 steps：每个 step 关联的 todo 也要复制到目标工作空间
            // 先分两遍走：第一遍创建所有步骤并建立新/旧 id 映射，
            // 第二遍修复 success_goto_step_id / fail_goto_step_id 的引用。
            let steps = self.list_loop_steps_by_loop(id).await?;
            // old_step_id → (new_step_id, old_success_goto, old_fail_goto)
            let mut step_map: Vec<(i64, i64, Option<i64>, Option<i64>)> = Vec::new();

            for s in &steps {
                let new_todo_id = if let Some(&cached) = todo_copy_map.get(&s.todo_id) {
                    cached
                } else {
                    // 复制 source todo 到目标工作空间
                    match self.copy_todo_to_workspace(s.todo_id, target_workspace_id, &ws).await? {
                        Some(copied_id) => {
                            todo_copy_map.insert(s.todo_id, copied_id);
                            copied_id
                        }
                        None => s.todo_id, // 回退：继续使用原始 todo_id
                    }
                };

                let new_step = self.create_loop_step(
                    new_loop.id,
                    &s.name,
                    &s.description,
                    new_todo_id,
                    &s.run_mode,
                    s.skip_on_source_failed != 0,
                    s.min_rating,
                    &s.unrated_policy,
                    s.enabled != 0,
                    &s.on_success,
                    None, // success_goto 第二遍再补
                    &s.on_rating_fail,
                    None, // fail_goto 第二遍再补
                    &s.review_type,
                )
                .await?;

                step_map.push((s.id, new_step.id, s.success_goto_step_id, s.fail_goto_step_id));
            }

            // 第二遍：更新有 goto 引用的步骤，把旧 step_id 换成新 step_id
            let old_to_new: std::collections::HashMap<i64, i64> = step_map.iter().map(|(old, new, _, _)| (*old, *new)).collect();
            for (_old_id, new_id, old_success_goto, old_fail_goto) in &step_map {
                let new_success_goto = old_success_goto.and_then(|g| old_to_new.get(&g).copied());
                let new_fail_goto = old_fail_goto.and_then(|g| old_to_new.get(&g).copied());

                if new_success_goto.is_some() || new_fail_goto.is_some() {
                    let _now = crate::models::utc_timestamp();
                    let existing = loop_steps::Entity::find_by_id(*new_id).one(&self.conn).await?;
                    if let Some(c) = existing {
                        let mut am: loop_steps::ActiveModel = c.into();
                        if let Some(goto) = new_success_goto {
                            am.success_goto_step_id = ActiveValue::Set(Some(goto));
                        }
                        if let Some(goto) = new_fail_goto {
                            am.fail_goto_step_id = ActiveValue::Set(Some(goto));
                        }
                        am.update(&self.conn).await?;
                    }
                }
            }

            created_ids.push(new_loop.id);
        }

        Ok(created_ids)
    }

    // ─── 辅助方法 ──────────────────────────────────────────────

    /// 从指定 loop_ids 的所有步骤中收集去重的 todo_id 列表。
    async fn collect_todo_ids_from_loops(&self, loop_ids: &[i64]) -> Result<Vec<i64>, sea_orm::DbErr> {
        use sea_orm::ColumnTrait;
        let mut seen = std::collections::HashSet::new();
        for &lid in loop_ids {
            let steps = loop_steps::Entity::find()
                .filter(loop_steps::Column::LoopId.eq(lid))
                .all(&self.conn)
                .await?;
            for s in steps {
                seen.insert(s.todo_id);
            }
        }
        Ok(seen.into_iter().collect())
    }

    /// 复制单个 todo 到目标工作空间并返回新 todo_id。
    /// 同时复制 tag 关联。
    async fn copy_todo_to_workspace(&self, todo_id: i64, target_workspace_id: i64, target_workspace: &str) -> Result<Option<i64>, sea_orm::DbErr> {
        use crate::db::entity::todos;
        use sea_orm::ColumnTrait;

        let source_model = todos::Entity::find_by_id(todo_id)
            .filter(todos::Column::DeletedAt.is_null())
            .one(&self.conn)
            .await?;
        let model = match source_model {
            Some(m) => m,
            None => return Ok(None),
        };

        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            title: ActiveValue::Set(model.title),
            prompt: ActiveValue::Set(model.prompt),
            status: ActiveValue::Set(model.status),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now.clone())),
            executor: ActiveValue::Set(model.executor),
            scheduler_enabled: ActiveValue::Set(model.scheduler_enabled),
            scheduler_config: ActiveValue::Set(model.scheduler_config),
            scheduler_timezone: ActiveValue::Set(model.scheduler_timezone),
            workspace_id: ActiveValue::Set(Some(target_workspace_id)),
            workspace_path: ActiveValue::Set(Some(target_workspace.to_string())),
            webhook_enabled: ActiveValue::Set(model.webhook_enabled),
            acceptance_criteria: ActiveValue::Set(model.acceptance_criteria),
            auto_review_enabled: ActiveValue::Set(model.auto_review_enabled),
            todo_type: ActiveValue::Set(model.todo_type),
            task_id: ActiveValue::Set(None),
            parent_todo_id: ActiveValue::Set(model.parent_todo_id),
            review_template_id: ActiveValue::Set(model.review_template_id),
            kind: ActiveValue::Set(model.kind),
            ..Default::default()
        };
        let inserted = am.insert(&self.conn).await?;
        let new_id = inserted.id;

        // 复制 tag 关联
        use crate::db::entity::todo_tags;
        let old_tags = todo_tags::Entity::find()
            .filter(todo_tags::Column::TodoId.eq(todo_id))
            .all(&self.conn)
            .await?;
        for t in old_tags {
            let tag_am = todo_tags::ActiveModel {
                todo_id: ActiveValue::Set(new_id),
                tag_id: ActiveValue::Set(t.tag_id),
            };
            tag_am.insert(&self.conn).await?;
        }

        Ok(Some(new_id))
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
                source.workspace_id,
                source.workspace_path.as_deref(),
                source.webhook_enabled,
                &source.icon,
                source.review_template_id,
                Some(source.limits_config.as_str()),
                source.abnormal_handler_todo_id,
                &source.abnormal_handler_trigger_on,
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

        // 复制 steps：两遍走（与 batch_copy_loops_to_workspace 一致）。
        // 第一遍创建所有 step 并建立 旧id→新id 映射，goto 字段暂置 None；
        // 第二遍把 success_goto_step_id / fail_goto_step_id 从「源 step id」重映射到「新 step id」。
        // 旧实现直接写源 step id，新 step 拿到的是全新自增 id，导致副本的 goto 指向源 loop 的
        // 步骤（悬空或错误），运行到 goto 分支时报 step not found / 跳错分支。
        let steps = self.list_loop_steps_by_loop(source_id).await?;
        // (old_step_id, new_step_id, old_success_goto, old_fail_goto)
        let mut step_map: Vec<(i64, i64, Option<i64>, Option<i64>)> = Vec::new();
        for s in &steps {
            let new_step = self
                .create_loop_step(
                    new_loop.id,
                    &s.name,
                    &s.description,
                    s.todo_id,
                    &s.run_mode,
                    s.skip_on_source_failed != 0,
                    s.min_rating,
                    &s.unrated_policy,
                    s.enabled != 0,
                    &s.on_success,
                    None, // success_goto 第二遍再补
                    &s.on_rating_fail,
                    None, // fail_goto 第二遍再补
                    &s.review_type,
                )
                .await?;
            step_map.push((s.id, new_step.id, s.success_goto_step_id, s.fail_goto_step_id));
        }

        // 第二遍：更新有 goto 引用的 step，把旧 step_id 换成新 step_id
        let old_to_new: std::collections::HashMap<i64, i64> =
            step_map.iter().map(|(old, new, _, _)| (*old, *new)).collect();
        for (_old_id, new_id, old_success_goto, old_fail_goto) in &step_map {
            let new_success_goto = old_success_goto.and_then(|g| old_to_new.get(&g).copied());
            let new_fail_goto = old_fail_goto.and_then(|g| old_to_new.get(&g).copied());
            if new_success_goto.is_some() || new_fail_goto.is_some() {
                let existing = loop_steps::Entity::find_by_id(*new_id).one(&self.conn).await?;
                if let Some(c) = existing {
                    let mut am: loop_steps::ActiveModel = c.into();
                    if let Some(goto) = new_success_goto {
                        am.success_goto_step_id = ActiveValue::Set(Some(goto));
                    }
                    if let Some(goto) = new_fail_goto {
                        am.fail_goto_step_id = ActiveValue::Set(Some(goto));
                    }
                    am.update(&self.conn).await?;
                }
            }
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

    /// 批量统计每个 todo 被启用中的 loop_steps 引用次数（用于事项中心 Loop 驱动分桶）。
    ///
    /// 只统计 `enabled=1` 的步骤：禁用步骤不参与 Loop 执行，不计入 Loop 驱动
    /// （设计文档明确要求）。`GROUP BY todo_id` 一次性聚合，避免列表场景 N+1。
    /// 返回 `todo_id -> count`，未出现的 todo 视为 0（调用方用 `unwrap_or(0)`）。
    pub async fn count_enabled_loop_steps_by_todos(
        &self,
        todo_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, i64>, sea_orm::DbErr> {
        // 空切片直接返回空 map，避免生成非法的 `IN ()` SQL
        if todo_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        // 手写 GROUP BY 聚合：sea_orm 的 find_also_related 不便表达 COUNT(*) GROUP BY，
        // 用原生 SQL 更直观，且与 list_loops_with_counts 里的子查询风格一致。
        let values: Vec<sea_orm::Value> = todo_ids.iter().map(|&id| id.into()).collect();
        // 占位符数量必须与值数量一致：构造 "?,?,?" 串
        let placeholders = std::iter::repeat("?").take(todo_ids.len()).collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT todo_id, COUNT(*) AS cnt FROM loop_steps \
             WHERE enabled = 1 AND todo_id IN ({placeholders}) \
             GROUP BY todo_id"
        );
        let rows = self
            .conn
            .query_all(sea_orm::Statement::from_sql_and_values(
                DbBackend::Sqlite,
                sql,
                values,
            ))
            .await?;
        // 逐行收集到 map；try_get 按 column 名取值，读不到记 0 不致命
        let mut map = std::collections::HashMap::new();
        for row in rows {
            let todo_id: i64 = row.try_get_by("todo_id")?;
            let cnt: i64 = row.try_get_by("cnt")?;
            map.insert(todo_id, cnt);
        }
        Ok(map)
    }

    /// 单个 todo 被启用中的 loop_steps 引用次数。
    ///
    /// 删除 todo 前的引用校验用：被启用环节引用的 todo 不应直接软删，
    /// 否则 Loop 执行时会指向已删除事项（设计文档风险三指出的现状缺陷）。
    /// 复用批量实现，避免又写一份 SQL。
    pub async fn count_enabled_loop_steps_by_todo(
        &self,
        todo_id: i64,
    ) -> Result<i64, sea_orm::DbErr> {
        let map = self.count_enabled_loop_steps_by_todos(&[todo_id]).await?;
        Ok(map.get(&todo_id).copied().unwrap_or(0))
    }

    /// 批量取每个 todo 被哪些启用的 Loop 引用（loop_id + loop_name）。
    ///
    /// 事项中心 Loop 驱动卡片用：展示「所属 Loop」并支持跳转 Loop 详情。
    /// 只统计 enabled=1 的 step（与计数口径一致）；JOIN loops 取 name。
    /// 返回 `todo_id -> Vec<LoopRefSummary>`，未出现的 todo 视为空 vec。
    pub async fn get_referencing_loops_for_todos(
        &self,
        todo_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, Vec<crate::models::LoopRefSummary>>, sea_orm::DbErr> {
        if todo_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let (placeholders, values) = Database::in_clause(todo_ids);
        // JOIN loops 取 name；ORDER BY todo_id, loop_id 保证输出稳定可测
        let sql = format!(
            "SELECT ls.todo_id, l.id as loop_id, l.name as loop_name FROM loop_steps ls \
             INNER JOIN loops l ON l.id = ls.loop_id \
             WHERE ls.enabled = 1 AND ls.todo_id IN ({placeholders}) \
             ORDER BY ls.todo_id ASC, l.id ASC"
        );
        let rows = self.query_all_sql(sql, values).await?;
        Ok(group_loop_refs_by_todo(rows))
    }

    /// 单个 todo 被多少条 loop_steps 引用（**不区分 enabled**）。
    ///
    /// 删除校验专用：设计文档风险三要求删除前查 `loop_steps.todo_id` 引用，
    /// 关注的是数据完整性（避免悬空 FK），而非是否参与执行。被禁用环节引用也算引用——
    /// 否则删后该 step 被重新启用时会指向已删除事项。
    pub async fn count_loop_steps_by_todo(
        &self,
        todo_id: i64,
    ) -> Result<i64, sea_orm::DbErr> {
        let (placeholders, values) = Database::in_clause(&[todo_id]);
        let sql =
            format!("SELECT COUNT(*) AS cnt FROM loop_steps WHERE todo_id IN ({placeholders})");
        let row = self
            .conn
            .query_one(sea_orm::Statement::from_sql_and_values(
                DbBackend::Sqlite,
                sql,
                values,
            ))
            .await?
            .ok_or(sea_orm::DbErr::RecordNotFound("count row missing".into()))?;
        row.try_get_by("cnt")
    }

    /// 参数数量由 loop_steps 表 schema 决定
    #[allow(clippy::too_many_arguments)]
    pub async fn create_loop_step(
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
            todo_id: ActiveValue::Set(todo_id),
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

    /// 参数数量由 loop_steps 表 schema 决定
    #[allow(clippy::too_many_arguments)]
    pub async fn update_loop_step(
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
            am.todo_id = ActiveValue::Set(todo_id);
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

    /// 仅更新步骤的 goto 跳转目标（用于导入时的伪ID解析）
    pub async fn update_loop_step_goto(
        &self,
        id: i64,
        success_goto_step_id: Option<i64>,
        fail_goto_step_id: Option<i64>,
    ) -> Result<(), sea_orm::DbErr> {
        let existing = loop_steps::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loop_steps::ActiveModel = c.into();
            if success_goto_step_id.is_some() {
                am.success_goto_step_id = ActiveValue::Set(success_goto_step_id);
            }
            if fail_goto_step_id.is_some() {
                am.fail_goto_step_id = ActiveValue::Set(fail_goto_step_id);
            }
            am.update(&self.conn).await?;
        }
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
            // valid 已确认 id 存在，find 必定返回 Some
            let step = steps.iter().find(|s| s.id == *id)
                .ok_or_else(|| sea_orm::DbErr::Custom(format!("step {} not found in loop {}", id, loop_id)))?;
            let mut am: loop_steps::ActiveModel = step.clone().into();
            am.order_index = Set(idx as i32);
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    // ====== Loop Executions ======

    /// 检查是否存在正在运行的 loop execution（status = "running"）。
    /// 用于自动更新前判断是否可以安全执行升级。
    pub async fn has_running_loop_executions(&self) -> Result<bool, sea_orm::DbErr> {
        use sea_orm::{PaginatorTrait};
        let count = loop_executions::Entity::find()
            .filter(loop_executions::Column::Status.eq("running"))
            .count(&self.conn)
            .await?;
        Ok(count > 0)
    }

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
        hours: Option<u32>,
    ) -> Result<Vec<loop_executions::Model>, sea_orm::DbErr> {
        let mut query = loop_executions::Entity::find()
            .filter(loop_executions::Column::LoopId.eq(loop_id))
            .order_by_desc(loop_executions::Column::StartedAt);
        if let Some(h) = hours.filter(|&h| h > 0) {
            // hours 已验证 > 0，format! 是构建 SQL 字面量的唯一途径
            let time_expr = sea_orm::sea_query::Expr::cust(format!(
                "REPLACE(REPLACE(started_at, 'T', ' '), 'Z', '') >= datetime('now', '-{} hours')", h
            ));
            query = query.filter(time_expr);
        }
        query.limit(limit).offset(offset).all(&self.conn).await
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
        error_message: Option<&str>,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let existing = loop_executions::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loop_executions::ActiveModel = c.into();
            am.status = ActiveValue::Set(status.to_string());
            am.finished_at = ActiveValue::Set(Some(now));
            am.completed_steps = ActiveValue::Set(completed_steps);
            am.failed_steps = ActiveValue::Set(failed_steps);
            // error_message 有值时写入 / 为 None 时保持数据库已有值不变。
            // 这意味着后续可以覆盖但不主动清空。如果将来需要显式清空，
            // 可改为 am.error_message = ActiveValue::Set(error_message.map(|s| s.to_string()));
            if let Some(msg) = error_message {
                am.error_message = ActiveValue::Set(Some(msg.to_string()));
            }
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

    /// 参数数量由 loop_step_executions 表 schema 决定
    #[allow(clippy::too_many_arguments)]
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

    /// 为异常处理步骤创建 loop_step_execution 记录。
    ///
    /// 使用原始 SQL 绕过外键约束，因为 abnormal handler 使用特殊 step_id=-1
    ///（该 ID 在 loop_steps 表中不存在，直接用 SeaORM insert 会触发 FK 校验失败）。
    pub async fn create_abnormal_handler_step_execution(
        &self,
        loop_execution_id: i64,
        todo_id: i64,
        sequence_index: i32,
    ) -> Result<i64, sea_orm::DbErr> {
        use sea_orm::{ConnectionTrait, Statement};
        let sql = r#"
            INSERT INTO loop_step_executions
                (loop_execution_id, step_id, todo_id, status, sequence_index, started_at)
            VALUES (?1, -1, ?2, 'running', ?3, datetime('now'))
        "#;
        let result = self
            .conn
            .execute(Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                sql,
                [loop_execution_id.into(), todo_id.into(), sequence_index.into()],
            ))
            .await?;
        Ok(result.last_insert_id() as i64)
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

    /// 一次 SQL 把 loop_step + 关联 todo 的 title/executor 拉出来。
    /// 供前端 LoopStudio 详情页直接渲染(避免 N+1)。
    ///
    /// `loop_steps.todo_id` 直接 JOIN `todos` 表读取 title 和 executor。
    pub async fn list_loop_steps_with_todo_meta(
        &self,
        loop_id: i64,
    ) -> Result<Vec<(loop_steps::Model, String, String, Option<String>)>, sea_orm::DbErr> {
        // 用 raw SQL JOIN; SeaORM 的 join API 对 has-many/belongs-to 支持有限,
        // 一次写清晰且类型稳定。
        //
        // 返回 (loop_step, todo_title, todo_executor, todo_archived_at) 四元组。
        // archived_at 用于 Loop 详情图上标记「已归档」环节，避免用户在 Loop 里误用已隐藏事项。
        use sea_orm::{ConnectionTrait, Statement};
        let sql = format!(
            "SELECT s.id, s.loop_id, s.name, s.description, s.order_index, s.todo_id, \
                    s.run_mode, s.skip_on_source_failed, s.min_rating, s.unrated_policy, \
                    s.on_success, s.success_goto_step_id, s.on_rating_fail, s.fail_goto_step_id, \
                    s.review_type, \
                    s.enabled, s.created_at, \
                    st.title as todo_title, st.executor as todo_executor, \
                    st.archived_at as todo_archived_at \
             FROM loop_steps s \
             INNER JOIN todos st ON st.id = s.todo_id \
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
                todo_id: row.try_get_by::<i64, _>("todo_id")?,
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
            let todo_archived_at: Option<String> =
                row.try_get_by::<Option<String>, _>("todo_archived_at")?;
            out.push((model, todo_title, todo_executor, todo_archived_at));
        }
        Ok(out)
    }

    // ====== 辅助：批量取 loop + 计数 ======

    /// 一次 SQL 把所有 loop + 它的 trigger.step 数 + 最近一次 execution 状态拉出来。
    /// 供左侧 LoopList 用,避免 N+1。按 workspace_id 过滤（唯一键，符合"筛选必须用 id"约定）。
        pub async fn list_loops_with_counts(
            &self,
            workspace_id: Option<i64>,
        ) -> Result<Vec<LoopListRow>, sea_orm::DbErr> {
            use sea_orm::{ConnectionTrait, Statement};
            let sql = match workspace_id {
                Some(_) => "SELECT l.id, l.name, l.description, l.workspace_path, \
                              l.status, l.color, l.icon, l.limits_config, l.review_template_id, \
                              l.webhook_enabled, \
                              l.abnormal_handler_todo_id, l.abnormal_handler_trigger_on, \
                              l.created_at, l.updated_at, \
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
                       WHERE l.workspace_id = ?1 \
                       ORDER BY l.updated_at DESC",
                None => "SELECT l.id, l.name, l.description, l.workspace_path, \
                          l.status, l.color, l.icon, l.limits_config, l.review_template_id, \
                          l.webhook_enabled, \
                          l.abnormal_handler_todo_id, l.abnormal_handler_trigger_on, \
                          l.created_at, l.updated_at, \
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
            let rows = if let Some(wid) = workspace_id {
                self.conn
                    .query_all(
                        Statement::from_sql_and_values(sea_orm::DbBackend::Sqlite, sql, [wid.into()])
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
                    workspace_path: row.try_get_by::<Option<String>, _>("workspace_path")?,
                    workspace_id: row.try_get_by::<Option<i64>, _>("workspace_id")?,
                    webhook_enabled: row.try_get_by::<bool, _>("webhook_enabled")?,
                    status: row.try_get_by::<String, _>("status")?,
                    color: row.try_get_by::<String, _>("color")?,
                    icon: row.try_get_by::<String, _>("icon")?,
                    review_template_id: row.try_get_by::<Option<i64>, _>("review_template_id")?,
                    limits_config: row.try_get_by::<String, _>("limits_config")?,
                    abnormal_handler_todo_id: row.try_get_by::<Option<i64>, _>("abnormal_handler_todo_id")?,
                    abnormal_handler_trigger_on: row.try_get_by::<String, _>("abnormal_handler_trigger_on")?,
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

    // ====== Loop 聚合统计(dashboard「自动化」Tab)======
    // 设计参照 db/dashboard.rs:原生 SQL + json_extract(usage) + SUM,
    // 4 条独立查询用 tokio::try_join! 并行后组装 LoopStats。
    // token 在 execution_records.usage JSON 里,必须经 loop_step_executions JOIN,
    // 前端若聚合会是 N²+N(逐 loop 拉 executions 再逐条取 token),故下沉到后端一条 SQL。

    /// GET /api/loops/stats 的数据来源:聚合所有 loop 的规模/执行/触发器/Token。
    /// hours=None 或 0 表示全时段;否则按 loop_executions.started_at 过滤执行类指标。
    pub async fn get_loop_stats(
        &self,
        hours: Option<u32>,
    ) -> Result<crate::models::LoopStats, sea_orm::DbErr> {
        self.get_loop_stats_for_workspace(None, hours).await
    }

    /// GET /api/v1/workspaces/{ws}/loops/stats 的数据来源:按 workspace 聚合 loop 统计。
    /// workspace_id=None 时退化为全库聚合,与 `get_loop_stats` 等价。
    pub async fn get_loop_stats_for_workspace(
        &self,
        workspace_id: Option<i64>,
        hours: Option<u32>,
    ) -> Result<crate::models::LoopStats, sea_orm::DbErr> {
        // 4 条查询互不依赖,并行执行;counts 查 loops 表(无时间窗),其余按 hours 过滤。
        let (counts, exec_summary, trigger_dist, token_totals) = tokio::try_join!(
            self.fetch_loop_counts(workspace_id),
            self.fetch_loop_execution_summary(workspace_id, hours),
            self.fetch_loop_trigger_distribution(workspace_id, hours),
            self.fetch_loop_token_totals(workspace_id, hours),
        )?;
        Ok(crate::models::LoopStats {
            total_loops: counts.0,
            active_loops: counts.1,
            total_executions: exec_summary.0,
            success_executions: exec_summary.1,
            failed_executions: exec_summary.2,
            total_input_tokens: token_totals.0,
            total_output_tokens: token_totals.1,
            total_cost_usd: token_totals.2,
            trigger_type_distribution: trigger_dist,
        })
    }

    /// loop 总数与活跃数(来自 loops 配置表,不受时间窗影响)。active = status='enabled'。
    async fn fetch_loop_counts(
        &self,
        workspace_id: Option<i64>,
    ) -> Result<(i64, i64), sea_orm::DbErr> {
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        let ws_filter = workspace_id
            .map(|id| format!("WHERE workspace_id = {}", id))
            .unwrap_or_default();
        let sql = format!(
            "SELECT \
            COUNT(*) AS total, \
            COALESCE(SUM(CASE WHEN status='enabled' THEN 1 ELSE 0 END), 0) AS active \
            FROM loops {}",
            ws_filter
        );
        let row = self
            .conn
            .query_one(Statement::from_string(DbBackend::Sqlite, sql))
            .await?
            .ok_or_else(|| sea_orm::DbErr::RecordNotFound("loop counts returned no rows".into()))?;
        Ok((
            row.try_get_by::<i64, _>("total").unwrap_or(0),
            row.try_get_by::<i64, _>("active").unwrap_or(0),
        ))
    }

    /// loop_executions 的总数/成功/失败(按 hours 过滤 started_at)。
    async fn fetch_loop_execution_summary(
        &self,
        workspace_id: Option<i64>,
        hours: Option<u32>,
    ) -> Result<(i64, i64, i64), sea_orm::DbErr> {
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        let ws_filter = workspace_id
            .map(|id| format!("AND l.workspace_id = {}", id))
            .unwrap_or_default();
        let sql = format!(
            "SELECT \
            COUNT(*) AS total, \
            COALESCE(SUM(CASE WHEN le.status='success' THEN 1 ELSE 0 END), 0) AS success, \
            COALESCE(SUM(CASE WHEN le.status='failed' THEN 1 ELSE 0 END), 0) AS failed \
            FROM loop_executions le \
            JOIN loops l ON l.id = le.loop_id \
            WHERE {} {}",
            Self::loop_exec_time_filter(hours, "le.started_at"),
            ws_filter
        );
        let row = self
            .conn
            .query_one(Statement::from_string(DbBackend::Sqlite, sql))
            .await?
            .ok_or_else(|| sea_orm::DbErr::RecordNotFound("loop exec summary returned no rows".into()))?;
        Ok((
            row.try_get_by::<i64, _>("total").unwrap_or(0),
            row.try_get_by::<i64, _>("success").unwrap_or(0),
            row.try_get_by::<i64, _>("failed").unwrap_or(0),
        ))
    }

    /// 触发类型分布(按 loop_executions.trigger_type GROUP BY)。
    async fn fetch_loop_trigger_distribution(
        &self,
        workspace_id: Option<i64>,
        hours: Option<u32>,
    ) -> Result<Vec<crate::models::LoopTriggerTypeCount>, sea_orm::DbErr> {
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        let ws_filter = workspace_id
            .map(|id| format!("AND l.workspace_id = {}", id))
            .unwrap_or_default();
        let sql = format!(
            "SELECT \
            COALESCE(le.trigger_type, 'manual') AS trigger_type, \
            COUNT(*) AS count, \
            COALESCE(SUM(CASE WHEN le.status='success' THEN 1 ELSE 0 END), 0) AS success_count, \
            COALESCE(SUM(CASE WHEN le.status='failed' THEN 1 ELSE 0 END), 0) AS failed_count \
            FROM loop_executions le \
            JOIN loops l ON l.id = le.loop_id \
            WHERE {} {} \
            GROUP BY COALESCE(le.trigger_type, 'manual') \
            ORDER BY count DESC",
            Self::loop_exec_time_filter(hours, "le.started_at"),
            ws_filter
        );
        let rows = self
            .conn
            .query_all(Statement::from_string(DbBackend::Sqlite, sql))
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(crate::models::LoopTriggerTypeCount {
                trigger_type: row
                    .try_get_by::<String, _>("trigger_type")
                    .unwrap_or_else(|_| "manual".to_string()),
                count: row.try_get_by::<i64, _>("count").unwrap_or(0),
                success_count: row.try_get_by::<i64, _>("success_count").unwrap_or(0),
                failed_count: row.try_get_by::<i64, _>("failed_count").unwrap_or(0),
            });
        }
        Ok(out)
    }

    /// Token 总量(经 loop_step_executions JOIN execution_records,SUM usage JSON)。
    async fn fetch_loop_token_totals(
        &self,
        workspace_id: Option<i64>,
        hours: Option<u32>,
    ) -> Result<(u64, u64, f64), sea_orm::DbErr> {
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        // le.started_at 用于时间过滤;LEFT JOIN 保证无 step/record 的 execution 行不丢失,
        // 其 token 经 COALESCE 兜底为 0。
        let ws_filter = workspace_id
            .map(|id| format!("AND l.workspace_id = {}", id))
            .unwrap_or_default();
        let sql = format!(
            "SELECT \
            COALESCE(SUM(COALESCE(json_extract(er.usage, '$.input_tokens'), 0)), 0) AS input_tokens, \
            COALESCE(SUM(COALESCE(json_extract(er.usage, '$.output_tokens'), 0)), 0) AS output_tokens, \
            COALESCE(SUM(COALESCE(json_extract(er.usage, '$.total_cost_usd'), 0.0)), 0.0) AS cost \
            FROM loop_executions le \
            JOIN loops l ON l.id = le.loop_id \
            LEFT JOIN loop_step_executions lse ON lse.loop_execution_id = le.id \
            LEFT JOIN execution_records er ON er.id = lse.execution_record_id \
            WHERE {} {}",
            Self::loop_exec_time_filter(hours, "le.started_at"),
            ws_filter
        );
        let row = self
            .conn
            .query_one(Statement::from_string(DbBackend::Sqlite, sql))
            .await?
            .ok_or_else(|| sea_orm::DbErr::RecordNotFound("loop token totals returned no rows".into()))?;
        // 用 i64 中转再 as u64:token 量级远低于 i64 上限,SQLite 整数返回 i64 最稳妥。
        let input: i64 = row.try_get_by::<i64, _>("input_tokens").unwrap_or(0);
        let output: i64 = row.try_get_by::<i64, _>("output_tokens").unwrap_or(0);
        let cost: f64 = row.try_get_by::<f64, _>("cost").unwrap_or(0.0);
        Ok((input as u64, output as u64, cost))
    }

    /// 构建 loop_executions 时间过滤 SQL 片段(impl 关联函数,无 self)。
    /// hours=None/0 → 全时段("1=1");否则按 started_at 文本列回退 N 小时。
    /// col 允许传别名前缀(如 "le.started_at"),适配 JOIN 查询的表别名。
    fn loop_exec_time_filter(hours: Option<u32>, col: &str) -> String {
        match hours.filter(|&h| h > 0) {
            Some(h) => format!(
                "REPLACE(REPLACE({col}, 'T', ' '), 'Z', '') >= datetime('now', '-{h} hours')"
            ),
            None => "1=1".to_string(),
        }
    }

    /// 加载 loop 详情(基本+所有子项)给前端 LoopStudio 详情面板用。
    /// 单次返回所有必要数据,前端无需多次请求。
    ///
    /// `loop_steps.todo_id` 直接指向 `todos` 表，不再经过 `steps` 中间层。
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
            steps_with_meta.iter().map(|(s, _, _, _)| s.clone()).collect();
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
    /// (step, todo_title, todo_executor, todo_archived_at)
    /// todo_* 字段从 todos 表 JOIN 读，见 list_loop_steps_with_todo_meta。
    pub steps_meta: Vec<(loop_steps::Model, String, String, Option<String>)>,
    /// 该 loop 下待人工审批的环节执行数
    pub pending_approval_count: i32,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod loop_step_count_tests {
    use super::*;
    use crate::db::Database;

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    /// 插一条 todo，返回 id。
    async fn seed_todo(db: &Database, title: &str) -> i64 {
        db.exec(&format!(
            "INSERT INTO todos (title, prompt, status) VALUES ('{title}', 'p', 'pending')"
        ))
        .await
        .expect("insert todo");
        let row = db
            .conn
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                format!("SELECT id FROM todos WHERE title = '{title}'"),
            ))
            .await
            .expect("query id")
            .expect("row exists");
        row.try_get_by_index::<i64>(0).expect("id readable")
    }

    /// 插一条 loop 行（loop_steps.loop_id 有 FK 约束），返回其 id。
    async fn seed_loop(db: &Database, name: &str) -> i64 {
        db.exec(&format!("INSERT INTO loops (name) VALUES ('{name}')"))
            .await
            .expect("insert loop");
        let row = db
            .conn
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                format!("SELECT id FROM loops WHERE name = '{name}'"),
            ))
            .await
            .expect("query loop id")
            .expect("loop row exists");
        row.try_get_by_index::<i64>(0).expect("loop id readable")
    }

    /// 被 1 个启用环节引用 → 计数 1；未引用的 todo → 计数 0。
    /// 这是 delete_todo 引用校验的依据：>0 即拒绝删除。
    #[tokio::test]
    async fn test_count_enabled_loop_steps_by_todo_single() {
        let db = fresh_db().await;
        let referenced = seed_todo(&db, "被引用").await;
        let free_todo = seed_todo(&db, "自由").await;
        let loop_id = seed_loop(&db, "L1").await;

        // 插一条启用的 step 引用 referenced
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) VALUES ({loop_id}, 's1', {referenced}, 1)"
        ))
        .await
        .expect("insert step");

        assert_eq!(
            db.count_enabled_loop_steps_by_todo(referenced).await.unwrap(),
            1,
            "被启用环节引用应计数 1"
        );
        assert_eq!(
            db.count_enabled_loop_steps_by_todo(free_todo).await.unwrap(),
            0,
            "未被引用应计数 0"
        );
    }

    /// 禁用环节不计入：enabled=0 的 step 不参与 Loop 执行，count 应为 0。
    /// 与批量版语义一致（设计文档：只统计 enabled=1）。
    #[tokio::test]
    async fn test_count_enabled_loop_steps_by_todo_excludes_disabled_steps() {
        let db = fresh_db().await;
        let todo_id = seed_todo(&db, "仅被禁用环节引用").await;
        let loop_id = seed_loop(&db, "L2").await;
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) VALUES ({loop_id}, 's_disabled', {todo_id}, 0)"
        ))
        .await
        .expect("insert disabled step");
        assert_eq!(
            db.count_enabled_loop_steps_by_todo(todo_id).await.unwrap(),
            0,
            "禁用环节不应计入"
        );
    }

    /// count_enabled_loop_steps_by_todos（批量）：多 todo 一次聚合，禁用不计。
    #[tokio::test]
    async fn test_count_enabled_loop_steps_by_todos_batch() {
        let db = fresh_db().await;
        let t1 = seed_todo(&db, "T1").await;
        let t2 = seed_todo(&db, "T2").await;
        let lp = seed_loop(&db, "L").await;
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) VALUES ({lp}, 'a', {t1}, 1)"
        ))
        .await
        .expect("insert a");
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) VALUES ({lp}, 'b', {t1}, 1)"
        ))
        .await
        .expect("insert b");
        // t2 仅被禁用环节引用
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) VALUES ({lp}, 'c', {t2}, 0)"
        ))
        .await
        .expect("insert c");
        let map = db.count_enabled_loop_steps_by_todos(&[t1, t2]).await.unwrap();
        assert_eq!(map.get(&t1).copied().unwrap_or(0), 2, "t1 应计数 2 条启用");
        assert_eq!(map.get(&t2).copied().unwrap_or(0), 0, "t2 仅禁用环节");
    }

    /// count_loop_steps_by_todo（删除校验用，不区分 enabled）：禁用环节也算引用。
    /// 否则删后该 step 被重新启用会指向已删除事项（设计文档风险三）。
    #[tokio::test]
    async fn test_count_loop_steps_by_todo_includes_disabled() {
        let db = fresh_db().await;
        let todo_id = seed_todo(&db, "被禁用环节引用").await;
        let loop_id = seed_loop(&db, "L").await;
        // 只插一条禁用 step
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) VALUES ({loop_id}, 's', {todo_id}, 0)"
        ))
        .await
        .expect("insert disabled step");
        // 删除校验口径：禁用也算 → 计数 1（应拒绝删除）
        assert_eq!(
            db.count_loop_steps_by_todo(todo_id).await.unwrap(),
            1,
            "删除校验应计入禁用环节"
        );
        // 对照：enabled 口径为 0
        assert_eq!(
            db.count_enabled_loop_steps_by_todo(todo_id).await.unwrap(),
            0,
            "分桶口径不计禁用环节"
        );
    }

    /// get_referencing_loops_for_todos：按 todo_id 返回引用 Loop 摘要（loop_id + name），
    /// 只含启用环节，禁用环节的 Loop 不出现。事项中心 Loop 驱动卡片「所属 Loop」用。
    #[tokio::test]
    async fn test_get_referencing_loops_for_todos() {
        let db = fresh_db().await;
        let todo_a = seed_todo(&db, "A").await;
        let todo_b = seed_todo(&db, "B").await;
        let loop1 = seed_loop(&db, "Loop1").await;
        let loop2 = seed_loop(&db, "Loop2").await;

        // A 被 loop1(启用) + loop2(禁用) 引用 → 只应返回 loop1
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) VALUES ({loop1}, 's1', {todo_a}, 1)"
        ))
        .await
        .expect("insert s1");
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) VALUES ({loop2}, 's2', {todo_a}, 0)"
        ))
        .await
        .expect("insert s2");
        // B 无引用

        let map = db.get_referencing_loops_for_todos(&[todo_a, todo_b]).await.unwrap();
        let refs_a = map.get(&todo_a).expect("A 应有引用");
        assert_eq!(refs_a.len(), 1, "禁用环节的 Loop 不应出现");
        assert_eq!(refs_a[0].loop_id, loop1);
        assert_eq!(refs_a[0].loop_name, "Loop1");
        // B 未引用 → 不在 map 中（调用方按 unwrap_or_default 取空 vec）
        assert!(!map.contains_key(&todo_b));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod loop_stats_tests {
    use crate::db::Database;
    use sea_orm::{ConnectionTrait, DbBackend, Statement};

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    /// 取某表当前最大 id。测试单线程顺序插入,等价于「刚插入那行的 id」;
    /// 用 MAX 而非 last_insert_rowid,因为连接池不保证两次查询落在同一连接。
    async fn max_id(db: &Database, table: &str) -> i64 {
        let sql = format!("SELECT MAX(id) AS m FROM {table}");
        let row = db
            .conn
            .query_one(Statement::from_string(DbBackend::Sqlite, sql))
            .await
            .expect("query max id")
            .expect("max id row exists");
        row.try_get_by::<i64, _>("m").unwrap_or(0)
    }

    async fn seed_todo(db: &Database, title: &str) -> i64 {
        db.exec(&format!(
            "INSERT INTO todos (title, prompt, status) VALUES ('{title}', 'p', 'pending')"
        ))
        .await
        .expect("insert todo");
        max_id(db, "todos").await
    }

    /// 插 loop 并显式指定 status(enabled/paused),供 active_loops 统计测试。
    async fn seed_loop_status(db: &Database, name: &str, status: &str) -> i64 {
        db.exec(&format!(
            "INSERT INTO loops (name, status) VALUES ('{name}', '{status}')"
        ))
        .await
        .expect("insert loop");
        max_id(db, "loops").await
    }

    async fn seed_loop_step(db: &Database, loop_id: i64, todo_id: i64, name: &str) -> i64 {
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) VALUES ({loop_id}, '{name}', {todo_id}, 1)"
        ))
        .await
        .expect("insert step");
        max_id(db, "loop_steps").await
    }

    /// 插一条 loop 执行记录。time_expr 是受控的 SQL 时间字面量(如 datetime('now','-100 days')),
    /// 非用户输入,直接拼接无注入风险。
    async fn seed_loop_execution(
        db: &Database,
        loop_id: i64,
        trigger_type: &str,
        status: &str,
        time_expr: &str,
    ) -> i64 {
        db.exec(&format!(
            "INSERT INTO loop_executions (loop_id, trigger_type, status, started_at) \
             VALUES ({loop_id}, '{trigger_type}', '{status}', {time_expr})"
        ))
        .await
        .expect("insert loop_execution");
        max_id(db, "loop_executions").await
    }

    /// 插一条 execution_record,usage 为 JSON 文本(含 token/cost 字段)。
    async fn seed_execution_record(db: &Database, usage: &str) -> i64 {
        db.exec(&format!("INSERT INTO execution_records (usage) VALUES ('{usage}')"))
            .await
            .expect("insert execution_record");
        max_id(db, "execution_records").await
    }

    /// 关联 loop_step_executions 到 execution_record,建立 token 聚合的 JOIN 桥梁。
    async fn link_step_execution(
        db: &Database,
        loop_execution_id: i64,
        step_id: i64,
        todo_id: i64,
        execution_record_id: i64,
    ) {
        db.exec(&format!(
            "INSERT INTO loop_step_executions (loop_execution_id, step_id, todo_id, execution_record_id, status) \
             VALUES ({loop_execution_id}, {step_id}, {todo_id}, {execution_record_id}, 'success')"
        ))
        .await
        .expect("insert step_execution");
    }

    /// 全时段聚合:loop 规模、执行成功/失败、触发器分布、Token 都应正确汇总。
    #[tokio::test]
    async fn test_get_loop_stats_aggregates_all_fields() {
        let db = fresh_db().await;
        let todo = seed_todo(&db, "T").await;
        let l_active = seed_loop_status(&db, "active", "enabled").await;
        let _l_paused = seed_loop_status(&db, "paused", "paused").await;
        let step = seed_loop_step(&db, l_active, todo, "s1").await;

        // 3 次执行:2 success(cron + manual)、1 failed(cron)
        let le_success_cron = seed_loop_execution(&db, l_active, "cron", "success", "datetime('now')").await;
        let _le_success_manual = seed_loop_execution(&db, l_active, "manual", "success", "datetime('now')").await;
        let _le_failed_cron = seed_loop_execution(&db, l_active, "cron", "failed", "datetime('now')").await;

        // 给其中一次成功执行挂一个带 token 的 execution_record
        let er = seed_execution_record(&db, r#"{"input_tokens":100,"output_tokens":200,"total_cost_usd":0.5}"#).await;
        link_step_execution(&db, le_success_cron, step, todo, er).await;

        let stats = db.get_loop_stats(None).await.expect("stats");
        assert_eq!(stats.total_loops, 2, "共 2 个 loop");
        assert_eq!(stats.active_loops, 1, "仅 1 个 enabled");
        assert_eq!(stats.total_executions, 3);
        assert_eq!(stats.success_executions, 2);
        assert_eq!(stats.failed_executions, 1);
        assert_eq!(stats.total_input_tokens, 100);
        assert_eq!(stats.total_output_tokens, 200);
        assert_eq!(stats.total_cost_usd, 0.5);

        // 触发器分布断言抽到独立函数,让本测试体保持在 30 行以内(CLAUDE.md 函数长度限制)。
        assert_trigger_distribution(&stats);
    }

    /// 校验 trigger_type_distribution:cron 2 次(1 成功+1 失败)、manual 1 次(成功)。
    /// 从主测试抽出以控制函数行数;断言逻辑与主测试共享同一份 stats 结果。
    fn assert_trigger_distribution(stats: &crate::models::LoopStats) {
        let cron = stats
            .trigger_type_distribution
            .iter()
            .find(|t| t.trigger_type == "cron")
            .expect("cron 行");
        assert_eq!(cron.count, 2);
        assert_eq!(cron.success_count, 1);
        assert_eq!(cron.failed_count, 1);
        let manual = stats
            .trigger_type_distribution
            .iter()
            .find(|t| t.trigger_type == "manual")
            .expect("manual 行");
        assert_eq!(manual.count, 1);
        assert_eq!(manual.success_count, 1);
    }

    /// hours 过滤:窗口外的执行不计入执行类指标,但 total_loops 不受影响。
    #[tokio::test]
    async fn test_get_loop_stats_hours_filter_excludes_old() {
        let db = fresh_db().await;
        let lp = seed_loop_status(&db, "L", "enabled").await;
        // 近期(成功)+ 100 天前(失败)
        let _le_recent = seed_loop_execution(&db, lp, "cron", "success", "datetime('now')").await;
        let _le_old = seed_loop_execution(&db, lp, "cron", "failed", "datetime('now','-100 days')").await;

        let stats = db.get_loop_stats(Some(720)).await.expect("stats");
        // 720h = 30 天,100 天前的失败不计入
        assert_eq!(stats.total_executions, 1, "窗口外的不计入");
        assert_eq!(stats.success_executions, 1);
        assert_eq!(stats.failed_executions, 0);
        // total_loops 来自 loops 表,不受时间窗影响
        assert_eq!(stats.total_loops, 1);
        assert_eq!(stats.active_loops, 1);
    }

    /// 空库:所有计数为 0、触发器分布为空、不报错(防 NULL/空集 panic)。
    #[tokio::test]
    async fn test_get_loop_stats_empty_db() {
        let db = fresh_db().await;
        let stats = db.get_loop_stats(None).await.expect("stats");
        assert_eq!(stats.total_loops, 0);
        assert_eq!(stats.total_executions, 0);
        assert!(stats.trigger_type_distribution.is_empty());
        assert_eq!(stats.total_input_tokens, 0);
    }
}
