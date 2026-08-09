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
    QueryOrder, QuerySelect, DbBackend,
};

use crate::db::entity::{
    loop_executions, loop_phases, loop_step_executions, loop_steps, loops,
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
            // 工艺模板字段可为 NULL（环路未绑定模板），try_get_by 失败时回落 None
            let process_template_id: Option<i64> = row.try_get_by("process_template_id").ok();
            let process_template_name: Option<String> =
                row.try_get_by::<String, _>("process_template_name").ok();
            // 版本同样允许 NULL：手工环路或历史数据没有快照时由前端显示占位符。
            let process_template_version: Option<String> =
                row.try_get_by::<String, _>("process_template_version").ok();
            map.entry(todo_id).or_default().push(crate::models::LoopRefSummary {
                loop_id,
                loop_name,
                process_template_id,
                process_template_name,
                process_template_version,
            });
        }
    }
    map
}

/// 把 `SELECT loop_id, COUNT(*) AS cnt` 的聚合行组装成 `loop_id -> count` 映射。
/// 抽出以让 count_loop_executions_by_loop_ids 低于 30 行；单行解析失败跳过而非整体报错，
/// 与 group_loop_refs_by_todo 的容错口径一致。
fn group_count_by_loop_id(
    rows: Vec<sea_orm::QueryResult>,
) -> std::collections::HashMap<i64, i64> {
    let mut map = std::collections::HashMap::new();
    for row in rows {
        let loop_id: i64 = match row.try_get_by("loop_id") {
            Ok(v) => v,
            Err(_) => continue,
        };
        let cnt: i64 = match row.try_get_by("cnt") {
            Ok(v) => v,
            Err(_) => continue,
        };
        map.insert(loop_id, cnt);
    }
    map
}

impl Database {

    pub async fn get_loop(&self, id: i64) -> Result<Option<loops::Model>, sea_orm::DbErr> {
        loops::Entity::find_by_id(id).one(&self.conn).await
    }

    /// 按 ID 批量查找环路。
    ///
    /// 任务列表注入「环路工艺版本快照」用——一条 SQL 取回本次列表涉及的全部环路，
    /// 避免逐任务调 get_loop 的 N+1（列表行数越多放大越明显）。
    /// 空入参直接返回空 Vec：filter is_in(空) 在某些后端会生成非法 SQL，提前短路更稳妥。
    pub async fn get_loops_by_ids(
        &self,
        ids: &[i64],
    ) -> Result<Vec<loops::Model>, sea_orm::DbErr> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        loops::Entity::find()
            .filter(loops::Column::Id.is_in(ids.to_vec()))
            .all(&self.conn)
            .await
    }

    /// 按来源工艺模板列出实例环路（按创建时间倒序，id 倒序兜底保证稳定）。
    ///
    /// 工艺详情「实例环路」Tab 用：让用户从模板下钻到由它实例化的所有 Loop。
    pub async fn list_loops_by_process_template(
        &self,
        template_id: i64,
    ) -> Result<Vec<loops::Model>, sea_orm::DbErr> {
        loops::Entity::find()
            .filter(loops::Column::ProcessTemplateId.eq(template_id))
            .order_by_desc(loops::Column::CreatedAt)
            .order_by_desc(loops::Column::Id)
            .all(&self.conn)
            .await
    }

    /// 批量统计每个环路的执行次数（loop_executions 行数），返回 `loop_id -> count`。
    ///
    /// 工艺实例列表要避免 N+1：逐个调 count_loop_executions 会在环路多时放大查询，
    /// 这里用一条 GROUP BY 聚合；未出现的 loop_id 视为 0，由调用方兜底。
    pub async fn count_loop_executions_by_loop_ids(
        &self,
        loop_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, i64>, sea_orm::DbErr> {
        if loop_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let (placeholders, values) = Database::in_clause(loop_ids);
        // GROUP BY 一次聚合出全部环路的执行数；ORDER BY 无意义，结果进 HashMap
        let sql = format!(
            "SELECT loop_id, COUNT(*) AS cnt FROM loop_executions \
             WHERE loop_id IN ({placeholders}) GROUP BY loop_id"
        );
        let rows = self.query_all_sql(sql, values).await?;
        Ok(group_count_by_loop_id(rows))
    }

    /// 参数数量由 loops 表 schema 决定，无法进一步合并
    #[allow(clippy::too_many_arguments)]
    pub async fn create_loop(
        &self,
        name: &str,
        description: &str,
        workspace_id: Option<i64>,
        workspace_path: Option<&str>,
        limits_config: Option<&str>,
        abnormal_handler_todo_id: Option<i64>,
        abnormal_handler_trigger_on: &str,
    ) -> Result<loops::Model, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        // 044 环路瘦身：webhook_enabled/icon/review_template_id/color 等列已下线，
        // 创建时不再接受这些参数；status 固定 paused，由后续 update_loop_status 切换。
        let am = loops::ActiveModel {
            name: ActiveValue::Set(name.to_string()),
            description: ActiveValue::Set(description.to_string()),
            // 双字段同源写入：handler 必须保证 id 解析得到的 path 与 workspace_path 一致，
            // 任何不一致都意味着上游解析有 bug——DAO 不再单独接受「只传 path」。
            workspace_id: ActiveValue::Set(workspace_id),
            workspace_path: ActiveValue::Set(workspace_path.map(|s| s.to_string())),
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
            // 允许显式清空：前端传 null → 写入 "{}"（无限制），传字符串 → 写入对应值。
            am.limits_config = ActiveValue::Set(limits_config.unwrap_or("{}").to_string());
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

    /// 批量删除环路（CASCADE 删 triggers/steps/phase_executions）。
    pub async fn batch_delete_loops(&self, ids: &[i64]) -> Result<u64, sea_orm::DbErr> {
        if ids.is_empty() { return Ok(0); }
        let res = loops::Entity::delete_many()
            .filter(loops::Column::Id.is_in(ids.to_vec()))
            .exec(&self.conn)
            .await?;
        Ok(res.rows_affected)
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
                    Some(source.limits_config.as_str()),
                    source.abnormal_handler_todo_id,
                    &source.abnormal_handler_trigger_on,
                )
                .await?;

            // 044：loop_triggers 表已下线，批量复制不再复制触发器。

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
                    s.enabled != 0,
                    &s.on_success,
                    None, // success_goto 第二遍再补
                    &s.on_rating_fail,
                    None, // fail_goto 第二遍再补
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

    /// 列出 loop 下所有 phase，按 order_index 排序。
    pub async fn list_loop_phases_by_loop(
        &self,
        loop_id: i64,
    ) -> Result<Vec<loop_phases::Model>, sea_orm::DbErr> {
        loop_phases::Entity::find()
            .filter(loop_phases::Column::LoopId.eq(loop_id))
            .order_by_asc(loop_phases::Column::OrderIndex)
            .order_by_asc(loop_phases::Column::Id)
            .all(&self.conn)
            .await
    }

    pub async fn get_loop_step(
        &self,
        id: i64,
    ) -> Result<Option<loop_steps::Model>, sea_orm::DbErr> {
        loop_steps::Entity::find_by_id(id).one(&self.conn).await
    }

    /// 按 todo_id 反查关联的 loop_step。
    /// 用于 todo 级 auto_review 判定是否由环路闸门接管评审：
    /// 若该 step 设了 min_rating，loop_runner.apply_rating_gate 会同步打分（其分支
    /// 依赖 min_rating.is_some()），todo 级 auto_review 应跳过以避免同一 record 被评审两次。
    /// 一个 todo 至多被一个启用中的 step 引用，取 one 即可。
    pub async fn find_loop_step_by_todo_id(
        &self,
        todo_id: i64,
    ) -> Result<Option<loop_steps::Model>, sea_orm::DbErr> {
        loop_steps::Entity::find()
            .filter(loop_steps::Column::TodoId.eq(todo_id))
            .one(&self.conn)
            .await
    }

    /// 按 todo_id 查启用的 loop_step 内联 review_prompt（设计 033）。
    /// 044：loops.review_template_id 列已下线（评审模板归环节），环路级模板回退取消，
    /// 统一评审路径只取环节内联 review_prompt，走「环节内联 → 默认」。
    ///
    /// step 未找到或不启用时返回 Ok(None)。
    pub async fn find_loop_step_review_prompt_by_todo(
        &self,
        todo_id: i64,
    ) -> Result<Option<String>, sea_orm::DbErr> {
        use sea_orm::{ConnectionTrait, Statement, DbBackend, Value};
        let sql = "SELECT review_prompt FROM loop_steps \
                   WHERE todo_id = ? AND enabled = 1 \
                   ORDER BY id ASC \
                   LIMIT 1";
        let rows = self
            .conn
            .query_all(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                sql,
                [Value::BigInt(Some(todo_id))],
            ))
            .await?;
        Ok(rows
            .into_iter()
            .next()
            .and_then(|r| r.try_get_by("review_prompt").ok().flatten()))
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
        // JOIN loops 取 name；LEFT JOIN process_templates 取工艺模板（供「工艺」列展示）。
        // 用 LEFT JOIN：环路可能未绑定工艺模板（process_template_id 为空），此时模板字段为 NULL。
        // ORDER BY todo_id, loop_id 保证输出稳定可测
        let sql = format!(
            "SELECT ls.todo_id, l.id as loop_id, l.name as loop_name, \
                    pt.id as process_template_id, pt.display_name as process_template_name, \
                    COALESCE(l.process_template_version, pt.version) as process_template_version \
             FROM loop_steps ls \
             INNER JOIN loops l ON l.id = ls.loop_id \
             LEFT JOIN process_templates pt ON pt.id = l.process_template_id \
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

    /// 批量查多个 todo 各自被 loop_steps 引用的次数，按 todo_id 分组返回。
    /// 批量删除事项前的可删除性校验用：一次 GROUP BY 查询替代逐 id 的 `count_loop_steps_by_todo`（消除 N+1，091 性能优化）。
    /// 仅返回存在引用的 todo；调用方对未出现的 todo 视作引用数 0。
    pub async fn count_loop_steps_by_todos(
        &self,
        todo_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, i64>, sea_orm::DbErr> {
        use sea_orm::Statement;
        use std::collections::HashMap;
        if todo_ids.is_empty() {
            return Ok(HashMap::new());
        }
        // 参数化 IN：in_clause 按 ids 数量生成 ? 占位符，避免 SQL 拼接（与单条版一致）。
        let (placeholders, values) = Database::in_clause(todo_ids);
        let sql = format!(
            "SELECT todo_id, COUNT(*) AS cnt FROM loop_steps WHERE todo_id IN ({placeholders}) GROUP BY todo_id"
        );
        let rows = self
            .conn
            .query_all(Statement::from_sql_and_values(DbBackend::Sqlite, sql, values))
            .await?;
        let mut map: HashMap<i64, i64> = HashMap::new();
        for row in rows {
            // 行级取值失败按整行跳过（理论上 COUNT 结果列一定可读）。
            let todo_id: i64 = row.try_get_by("todo_id")?;
            let cnt: i64 = row.try_get_by("cnt")?;
            map.insert(todo_id, cnt);
        }
        Ok(map)
    }

    /// 参数数量由 loop_steps 表 schema 决定
    #[allow(clippy::too_many_arguments)]
    pub async fn create_loop_step(
        &self,
        loop_id: i64,
        name: &str,
        description: &str,
        todo_id: i64,
        enabled: bool,
        on_success: &str,
        success_goto_step_id: Option<i64>,
        on_rating_fail: &str,
        fail_goto_step_id: Option<i64>,
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
        // 044：run_mode/skip_on_source_failed/min_rating/unrated_policy 已下线，
        // 流转与评审改由 gate_config + on_success/on_rating_fail 表达。
        let am = loop_steps::ActiveModel {
            loop_id: ActiveValue::Set(loop_id),
            name: ActiveValue::Set(name.to_string()),
            description: ActiveValue::Set(description.to_string()),
            order_index: ActiveValue::Set(next_order),
            todo_id: ActiveValue::Set(todo_id),
            enabled: ActiveValue::Set(if enabled { 1 } else { 0 }),
            on_success: ActiveValue::Set(on_success.to_string()),
            success_goto_step_id: ActiveValue::Set(success_goto_step_id),
            on_rating_fail: ActiveValue::Set(on_rating_fail.to_string()),
            fail_goto_step_id: ActiveValue::Set(fail_goto_step_id),
            created_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        am.insert(&self.conn).await
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

    /// 回填 execution 的 task_id（dispatcher 创建后调用）。
    pub async fn update_loop_execution_task_id(&self, exec_id: i64, task_id: i64) -> Result<(), sea_orm::DbErr> {
        let existing = loop_executions::Entity::find_by_id(exec_id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loop_executions::ActiveModel = c.into();
            am.task_id = ActiveValue::Set(Some(task_id));
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    pub async fn get_loop_execution(
        &self,
        id: i64,
    ) -> Result<Option<loop_executions::Model>, sea_orm::DbErr> {
        loop_executions::Entity::find_by_id(id).one(&self.conn).await
    }

    /// 093-B5：任务详情页用的「最近 N 条 loop 执行记录」（从 handlers/tasks.rs 下沉）。
    /// 原 handler 侧是 `format!("... WHERE task_id={}")` 拼接 SQL（违反禁止清单 #4），
    /// 下沉同时改为 SeaORM 参数绑定查询。
    pub async fn list_recent_loop_executions_for_task(
        &self,
        task_id: i64,
        limit: u64,
    ) -> Result<Vec<loop_executions::Model>, sea_orm::DbErr> {
        loop_executions::Entity::find()
            .filter(loop_executions::Column::TaskId.eq(task_id))
            .order_by_desc(loop_executions::Column::StartedAt)
            .limit(limit)
            .all(&self.conn)
            .await
    }

    /// 093-B5：由 artifact 反查所属工作空间路径（从 handlers/tasks.rs 下沉的三级跳查询）。
    /// artifact → step_execution → loop_execution → loop 取 workspace_path；
    /// 纯搬移不改逻辑（保持逐跳 find 与原 NotFound 语义），JOIN 优化留待后续。
    pub async fn get_artifact_workspace_path(
        &self,
        step_execution_id: i64,
    ) -> Result<Option<String>, sea_orm::DbErr> {
        let se = loop_step_executions::Entity::find_by_id(step_execution_id)
            .one(&self.conn)
            .await?
            .ok_or(sea_orm::DbErr::RecordNotFound("step_exec not found".into()))?;
        let le = loop_executions::Entity::find_by_id(se.loop_execution_id)
            .one(&self.conn)
            .await?
            .ok_or(sea_orm::DbErr::RecordNotFound("loop_exec not found".into()))?;
        let lp = loops::Entity::find_by_id(le.loop_id)
            .one(&self.conn)
            .await?
            .ok_or(sea_orm::DbErr::RecordNotFound("loop not found".into()))?;
        Ok(lp.workspace_path)
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
    ///
    /// 「待审批」覆盖两条暂停路径（NTD-004）：
    /// - 旧评分路径：暂停时写 `approval_status='pending'`；
    /// - 工艺（phase_driver）路径：暂停时只写 `status='pending_approval'`，不写 approval_status。
    ///
    /// 两种写法互斥（不同路径产生），OR 条件不会重复计数。
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
             WHERE lse.loop_execution_id IN ({}) \
               AND (lse.approval_status = 'pending' OR lse.status = 'pending_approval') \
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

    /// 按 task 批量统计待审批环节数，返回 task_id → count 映射（063 任务待审批透出）。
    ///
    /// 与 count_pending_approvals_by_execution_ids 的差别仅在分组维度（task vs execution）：
    /// 任务列表需要「该任务是否还有要我处理的审批」，因此统计范围是该 task **所有执行**
    /// 的未处理审批总数（与 Loop 列表 count_pending_approvals_for_loop 同口径）——
    /// 若只统计最近一次执行，旧执行滞留的审批会被新执行掩盖，用户永远收不到提醒。
    ///
    /// 待审批口径沿用 NTD-004：approval_status='pending' OR status='pending_approval'，
    /// 两条暂停路径互斥产生，OR 条件不会重复计数。
    pub async fn count_pending_approvals_by_task_ids(
        &self,
        task_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, i32>, sea_orm::DbErr> {
        use sea_orm::{ConnectionTrait, Statement};
        let mut map = std::collections::HashMap::new();
        // 空入参短路：避免拼出 IN () 的非法 SQL，与兄弟方法的防御模式一致。
        if task_ids.is_empty() {
            return Ok(map);
        }
        // 任务 id 来自内部调用方（list_tasks 收集的内存 id），非用户输入，直接拼接无注入面。
        let ids_str: String = task_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT le.task_id, COUNT(*) AS n \
             FROM loop_step_executions lse \
             INNER JOIN loop_executions le ON le.id = lse.loop_execution_id \
             WHERE le.task_id IN ({}) \
               AND (lse.approval_status = 'pending' OR lse.status = 'pending_approval') \
             GROUP BY le.task_id",
            ids_str
        );
        let rows = self
            .conn
            .query_all(Statement::from_string(DbBackend::Sqlite, sql))
            .await?;
        for row in rows {
            // task_id 列可空（历史数据）：WHERE IN 已排除 NULL，但防御性跳过保持健壮。
            if let Ok(Some(task_id)) = row.try_get_by::<Option<i64>, _>("task_id") {
                let n: i32 = row.try_get_by("n").unwrap_or(0);
                map.insert(task_id, n);
            }
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

    /// 终态化所有 running phase：loop 终态时调用，确保 phase 执行记录与 loop 状态一致。
    ///
    /// BUG-004：挂起时 phase 不会被终态化（保持 running），loop 成功后需要把剩余 running
    /// phase 全部标为 success；loop 失败时标为 failed。
    pub async fn finalize_phase_executions(
        &self,
        loop_execution_id: i64,
        status: &str,
    ) -> Result<(), sea_orm::DbErr> {
        use sea_orm::{ConnectionTrait, Statement};
        let phase_status = if status == "success" {
            "success"
        } else {
            "failed"
        };
        let sql = format!(
            "UPDATE loop_phase_executions SET status = '{}', finished_at = ?1 \
             WHERE loop_execution_id = {} AND status = 'running'",
            phase_status, loop_execution_id
        );
        let now = crate::models::utc_timestamp();
        self.conn
            .execute(Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                &sql,
                [sea_orm::Value::from(now)],
            ))
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
    ) -> Result<loop_step_executions::Model, sea_orm::DbErr> {
        // 044：loop_steps 已无 min_rating/unrated_policy，创建时不再向 step_executions
        // 快照这两列（保持 NULL）。阈值改由 phase_driver 评价 gate_config 后通过
        // set_step_execution_min_rating 回写；历史列仅为旧数据展示保留。
        let am = loop_step_executions::ActiveModel {
            loop_execution_id: ActiveValue::Set(loop_execution_id),
            step_id: ActiveValue::Set(step_id),
            todo_id: ActiveValue::Set(todo_id),
            status: ActiveValue::Set(status.to_string()),
            sequence_index: ActiveValue::Set(sequence_index),
            ..Default::default()
        };
        let model = am.insert(&self.conn).await?;

        // 同步创建门禁记录：从步骤的 gate_config 解析，为每个 gate 创建 pending 记录。
        // 步骤创建时就创建 gate，而非等待进入某个状态后才创建——避免因分支遗漏导致 gate 缺失。
        if let Ok(Some(step)) = crate::db::entity::loop_steps::Entity::find_by_id(step_id)
            .one(&self.conn)
            .await
        {
            if !step.gate_config.is_empty() {
                if let Ok(gates) = serde_json::from_str::<Vec<serde_json::Value>>(&step.gate_config) {
                    for g in &gates {
                        if let (Some(gt), Some(gn)) = (
                            g.get("type").and_then(|v| v.as_str()),
                            g.get("name").and_then(|v| v.as_str()),
                        ) {
                            if let Err(e) = self
                                .create_loop_step_execution_gate(
                                    model.id, gt, gn,
                                    &serde_json::to_string(g).unwrap_or_default(),
                                )
                                .await
                            {
                                tracing::warn!(
                                    "create step_execution #{}: failed to create gate '{}': {}",
                                    model.id, gn, e,
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(model)
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

    /// 批量取多个 loop_execution 的 step_executions，按 loop_execution_id 分组返回。
    /// 执行历史列表用：一次 IN 查询替代逐 execution 查询，消除 N+1（091 性能优化）。
    /// 每组内仍按 sequence_index 升序，与单条版 `list_loop_step_executions` 口径一致。
    pub async fn list_loop_step_executions_by_exec_ids(
        &self,
        loop_execution_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, Vec<loop_step_executions::Model>>, sea_orm::DbErr> {
        use std::collections::HashMap;
        // 空入参直接返回空 map，避免生成非法的 `IN ()` SQL。
        if loop_execution_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = loop_step_executions::Entity::find()
            .filter(loop_step_executions::Column::LoopExecutionId.is_in(loop_execution_ids.to_vec()))
            .order_by_asc(loop_step_executions::Column::SequenceIndex)
            .all(&self.conn)
            .await?;
        // 按 loop_execution_id 分组，组内已按 sequence_index 升序。
        let mut map: HashMap<i64, Vec<_>> = HashMap::new();
        for row in rows {
            map.entry(row.loop_execution_id).or_default().push(row);
        }
        Ok(map)
    }

    /// 批量取多个 task 各自最近一次 loop_execution（任务列表「最近执行」列用）。
    /// 单次 IN 查询（按 started_at 倒序）+ Rust 端按 task_id 取首条，消除逐 task 的 N+1（091 性能优化）。
    /// 返回 `task_id -> 最新执行 Model`，未出现的 task 视为无执行记录。
    pub async fn get_latest_execution_by_task_ids(
        &self,
        task_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, loop_executions::Model>, sea_orm::DbErr> {
        use std::collections::HashMap;
        if task_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = loop_executions::Entity::find()
            .filter(loop_executions::Column::TaskId.is_in(task_ids.to_vec()))
            .order_by_desc(loop_executions::Column::StartedAt)
            .all(&self.conn)
            .await?;
        let mut latest: HashMap<i64, loop_executions::Model> = HashMap::new();
        for row in rows {
            // 倒序遍历：首个出现的 task_id 即其最新执行（started_at 最大），后续同 task_id 跳过。
            // task_id 列可空（历史数据），NULL 行无法按 task 索引，跳过。
            if let Some(tid) = row.task_id {
                latest.entry(tid).or_insert(row);
            }
        }
        Ok(latest)
    }

    pub async fn mark_step_execution_started(
        &self,
        id: i64,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let existing = loop_step_executions::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            // 人工审批步骤复用已 completed todo 时，创建即写入 pending_approval（LoopRunner 4e），
            // 若无条件改回 running 会冲掉该状态，导致步骤卡 running、前端审批入口不出现。
            let preserve_status = c.status == "pending_approval";
            let mut am: loop_step_executions::ActiveModel = c.into();
            if !preserve_status {
                am.status = ActiveValue::Set("running".to_string());
            }
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

    /// 设置环节执行的返工计数。
    pub async fn set_step_execution_rework_count(
        &self,
        id: i64,
        rework_count: i32,
    ) -> Result<(), sea_orm::DbErr> {
        let existing = loop_step_executions::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loop_step_executions::ActiveModel = c.into();
            am.rework_count = ActiveValue::Set(rework_count);
            am.update(&self.conn).await?;
        }
        Ok(())
    }

    /// 回写 step execution 的评审阈值（min_rating）。
    ///
    /// gate_config 风格步骤的阈值在门禁 JSON（ai_criteria_review.min_score）里，
    /// 创建 step execution 时 step.min_rating 为 NULL，需由 PhaseDriver 评价门禁后回写，
    /// 前端环节卡片据此显示「阈值 N」并判定评分是否达标。
    pub async fn set_step_execution_min_rating(
        &self,
        id: i64,
        min_rating: i32,
    ) -> Result<(), sea_orm::DbErr> {
        let existing = loop_step_executions::Entity::find_by_id(id).one(&self.conn).await?;
        if let Some(c) = existing {
            let mut am: loop_step_executions::ActiveModel = c.into();
            am.min_rating = ActiveValue::Set(Some(min_rating));
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
                    s.on_success, s.success_goto_step_id, s.on_rating_fail, s.fail_goto_step_id, \
                    s.phase_id, s.expected_artifacts, s.step_template_refs, s.gate_config, s.max_rework, \
                    s.skill_names, s.expert_name, s.review_prompt, \
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
            // 044：loop_steps 已移除 run_mode/skip_on_source_failed/min_rating/unrated_policy 列，
            // SELECT 与 Model 构造同步去掉这些字段。
            let model = loop_steps::Model {
                id: row.try_get_by::<i64, _>("id")?,
                loop_id: row.try_get_by::<i64, _>("loop_id")?,
                name: row.try_get_by::<String, _>("name")?,
                description: row.try_get_by::<String, _>("description")?,
                order_index: row.try_get_by::<i32, _>("order_index")?,
                todo_id: row.try_get_by::<i64, _>("todo_id")?,
                on_success: row.try_get_by::<String, _>("on_success")?,
                success_goto_step_id: row.try_get_by::<Option<i64>, _>("success_goto_step_id")?,
                on_rating_fail: row.try_get_by::<String, _>("on_rating_fail")?,
                fail_goto_step_id: row.try_get_by::<Option<i64>, _>("fail_goto_step_id")?,
                phase_id: row.try_get_by::<Option<i64>, _>("phase_id")?,
                expected_artifacts: row.try_get_by::<String, _>("expected_artifacts")?,
                step_template_refs: row.try_get_by::<String, _>("step_template_refs")?,
                gate_config: row.try_get_by::<String, _>("gate_config")?,
                max_rework: row.try_get_by::<i32, _>("max_rework")?,
                skill_names: row.try_get_by::<String, _>("skill_names")?,
                expert_name: row.try_get_by::<Option<String>, _>("expert_name")?,
                review_prompt: row.try_get_by::<Option<String>, _>("review_prompt")?,
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

    /// 一次 SQL 把所有 loop + step 计数 + 最近一次 execution 状态 + 待审批数拉出来。
    /// 供左侧 LoopList 用，避免 N+1。按 workspace_id 过滤（唯一键，符合"筛选必须用 id"约定）。
        pub async fn list_loops_with_counts(
            &self,
            workspace_id: Option<i64>,
        ) -> Result<Vec<LoopListRow>, sea_orm::DbErr> {
            use sea_orm::{ConnectionTrait, Statement};
            // 044：loops 已移除 color/icon/review_template_id/webhook_enabled；
            // loop_triggers 表已下线，不再 JOIN 聚合 trigger_count。
            // 列表查询补回 l.workspace_id（筛选键），供 LoopListRow.loop_ 完整填充。
            // 054 起统一按 id DESC（列表默认排序），替代原 updated_at DESC 口径；
            // 与 task.rs / todo.rs 保持一致，下方两个分支（带/不带 workspace_id 过滤）排序口径相同。
            let sql = match workspace_id {
                Some(_) => "SELECT l.id, l.name, l.description, l.workspace_path, \
                              l.workspace_id, l.status, l.limits_config, \
                              l.abnormal_handler_todo_id, l.abnormal_handler_trigger_on, \
                              l.process_template_id, l.process_template_version, \
                              l.created_at, l.updated_at, \
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
                       ORDER BY l.id DESC",
                None => "SELECT l.id, l.name, l.description, l.workspace_path, \
                          l.workspace_id, l.status, l.limits_config, \
                          l.abnormal_handler_todo_id, l.abnormal_handler_trigger_on, \
                          l.process_template_id, l.process_template_version, \
                          l.created_at, l.updated_at, \
                          (SELECT COUNT(*) FROM loop_steps s WHERE s.loop_id = l.id) as step_count, \
                          (SELECT le.status FROM loop_executions le \
                           WHERE le.loop_id = l.id ORDER BY le.started_at DESC LIMIT 1) as last_execution_status, \
                          (SELECT le.started_at FROM loop_executions le \
                           WHERE le.loop_id = l.id ORDER BY le.started_at DESC LIMIT 1) as last_execution_at, \
                          (SELECT COUNT(*) FROM loop_step_executions lse \
                           INNER JOIN loop_executions le2 ON le2.id = lse.loop_execution_id \
                           WHERE le2.loop_id = l.id AND lse.approval_status = 'pending') as pending_approval_count \
                       FROM loops l \
                       ORDER BY l.id DESC",
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
                    status: row.try_get_by::<String, _>("status")?,
                    limits_config: row.try_get_by::<String, _>("limits_config")?,
                    abnormal_handler_todo_id: row.try_get_by::<Option<i64>, _>("abnormal_handler_todo_id")?,
                    abnormal_handler_trigger_on: row.try_get_by::<String, _>("abnormal_handler_trigger_on")?,
                    // 列表查询不返回异常处理 prompt，给 None（详情接口才需展示）
                    abnormal_handler_prompt: None,
                    process_template_id: row.try_get_by::<Option<i64>, _>("process_template_id")?,
                    process_template_version: row.try_get_by::<Option<String>, _>("process_template_version")?,
                    created_at: row.try_get_by::<Option<String>, _>("created_at")?,
                    updated_at: row.try_get_by::<Option<String>, _>("updated_at")?,
                },
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
        let steps_with_meta = self.list_loop_steps_with_todo_meta(loop_id).await?;
        let steps: Vec<loop_steps::Model> =
            steps_with_meta.iter().map(|(s, _, _, _)| s.clone()).collect();
        // 统计该 loop 下待人工审批的环节执行数
        let pending_approval_count = self.count_pending_approvals_for_loop(loop_id).await?;
        // 044：loop_triggers 已下线，详情视图不再聚合 triggers。
        Ok(Some(LoopFullView {
            loop_,
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

    /// count_loop_steps_by_todos（批量，删除校验口径，不区分 enabled）：
    /// 一次 GROUP BY 聚合多 todo 的引用计数；未被引用的 todo 不出现在 map（调用方按 0 兜底）。
    #[tokio::test]
    async fn test_count_loop_steps_by_todos_batch_groups_by_todo() {
        let db = fresh_db().await;
        let t1 = seed_todo(&db, "T1").await;
        let t2 = seed_todo(&db, "T2").await;
        let lp = seed_loop(&db, "L").await;
        // t1 被两个环节引用（一启用一禁用）；删除校验口径不区分 enabled，应计 2。
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) VALUES ({lp}, 'a', {t1}, 1)"
        ))
        .await
        .expect("insert a");
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) VALUES ({lp}, 'b', {t1}, 0)"
        ))
        .await
        .expect("insert b");
        let map = db.count_loop_steps_by_todos(&[t1, t2]).await.unwrap();
        assert_eq!(map.get(&t1).copied().unwrap_or(0), 2, "t1 应计数全部引用（含禁用）");
        assert!(!map.contains_key(&t2), "未被引用的 todo 不应出现在 map");
        assert!(
            db.count_loop_steps_by_todos(&[]).await.unwrap().is_empty(),
            "空入参应返回空 map"
        );
    }

    /// get_latest_execution_by_task_ids：按 task 批量取最近一次执行（started_at 倒序取首条）。
    /// 验证「每 task 取最新」+ task_id 可空行不参与 + 空入参。
    #[tokio::test]
    async fn test_get_latest_execution_by_task_ids_picks_latest() {
        let db = fresh_db().await;
        let lp = seed_loop(&db, "L").await;
        // loop_executions.task_id 有 FK→tasks，先建 task 行再引用。
        db.exec("INSERT INTO tasks (title, description, status, created_by) VALUES ('T','d','pending','test')")
            .await
            .expect("insert task");
        let task_id: i64 = db
            .conn
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT MAX(id) AS m FROM tasks",
            ))
            .await
            .expect("query task id")
            .expect("task row exists")
            .try_get_by("m")
            .expect("task id readable");
        // 同 task 两条执行：started_at 一早一晚，晚的应被选为「最近」。
        db.exec(&format!(
            "INSERT INTO loop_executions (loop_id, trigger_type, started_at, task_id, status) \
             VALUES ({lp}, 'manual', '2026-01-01T00:00:00Z', {task_id}, 'failed')"
        ))
        .await
        .expect("insert old exec");
        db.exec(&format!(
            "INSERT INTO loop_executions (loop_id, trigger_type, started_at, task_id, status) \
             VALUES ({lp}, 'manual', '2026-02-01T00:00:00Z', {task_id}, 'success')"
        ))
        .await
        .expect("insert new exec");
        // 另插一条 task_id=NULL 的执行：不应被任何 task 选中。
        db.exec(&format!(
            "INSERT INTO loop_executions (loop_id, trigger_type, started_at, task_id, status) \
             VALUES ({lp}, 'manual', '2026-03-01T00:00:00Z', NULL, 'running')"
        ))
        .await
        .expect("insert null-task exec");
        let map = db.get_latest_execution_by_task_ids(&[task_id]).await.unwrap();
        let latest = map.get(&task_id).expect("该 task 应有最近执行");
        assert_eq!(latest.status, "success", "应选 started_at 最新的执行");
        assert!(
            db.get_latest_execution_by_task_ids(&[]).await.unwrap().is_empty(),
            "空入参应返回空 map"
        );
    }

    /// find_loop_step_by_todo_id：按 todo_id 反查 step；存在/不存在两条路径。
    /// 用于 todo 级 auto_review 判定是否由环路闸门接管（step.min_rating.is_some()）。
    #[tokio::test]
    async fn test_find_loop_step_by_todo_id_found_and_missing() {
        let db = fresh_db().await;
        let todo_id = seed_todo(&db, "环节todo").await;
        let free_todo = seed_todo(&db, "自由todo").await;
        let loop_id = seed_loop(&db, "L").await;
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) \
             VALUES ({loop_id}, 's', {todo_id}, 1)"
        ))
        .await
        .expect("insert step");

        // 命中：返回该 step
        let found = db
            .find_loop_step_by_todo_id(todo_id)
            .await
            .unwrap()
            .expect("step should exist");
        assert_eq!(found.todo_id, todo_id);

        // 未命中：未被环节引用的 todo 返回 None
        let missing = db.find_loop_step_by_todo_id(free_todo).await.unwrap();
        assert!(missing.is_none(), "未被环节引用的 todo 应返回 None");
    }

    /// list_loop_steps_with_todo_meta 的 raw SQL 手工映射必须读出 step_template_refs（需求 054）。
    /// 该函数绕过 SeaORM 实体、手写 SELECT 列与行映射，加列后漏改会编译中断，
    /// 但「映射错位/读错列」只能靠「写入非标值 → 读回逐字断言」防回归。
    #[tokio::test]
    async fn test_list_loop_steps_with_todo_meta_maps_step_template_refs() {
        let db = fresh_db().await;
        let loop_id = seed_loop(&db, "L").await;
        // 环节一：更新为非默认 JSON，验证映射真的读到了新列而非默认值兼底；
        // 环节二：保持 INSERT 默认，验证存量行默认 '[]' 路径。
        let todo_with_refs = seed_todo(&db, "带引用").await;
        let todo_default = seed_todo(&db, "默认").await;
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) \
             VALUES ({loop_id}, 's1', {todo_with_refs}, 1), ({loop_id}, 's2', {todo_default}, 1)"
        ))
        .await
        .expect("insert steps");
        db.exec(&format!(
            "UPDATE loop_steps SET step_template_refs = \
             '[{{\"name\":\"规范\",\"path\":\"bundled://x.md\"}}]' WHERE todo_id = {todo_with_refs}"
        ))
        .await
        .expect("update refs");

        let rows = db.list_loop_steps_with_todo_meta(loop_id).await.unwrap();
        assert_eq!(rows.len(), 2, "两个环节都应返回");
        let with_refs = rows
            .iter()
            .find(|(s, _, _, _)| s.todo_id == todo_with_refs)
            .expect("带引用环节应在结果中");
        assert_eq!(
            with_refs.0.step_template_refs, r#"[{"name":"规范","path":"bundled://x.md"}]"#,
            "raw SQL 映射应原样读回写入的 JSON"
        );
        let default_row = rows
            .iter()
            .find(|(s, _, _, _)| s.todo_id == todo_default)
            .expect("默认环节应在结果中");
        assert_eq!(
            default_row.0.step_template_refs, "[]",
            "未显式赋值的存量行应读回列默认值 '[]'"
        );
    }

    /// find_loop_step_review_prompt_by_todo：按 todo_id 反查启用环节的内联 review_prompt，
    /// 供评审 prompt 回退（completion.rs 调用）。覆盖命中与未命中两条路径。
    /// 044：loops.review_template_id 已下线，只返回环节内联 prompt。
    #[tokio::test]
    async fn test_find_loop_step_review_prompt_found_and_missing() {
        let db = fresh_db().await;
        let todo_id = seed_todo(&db, "环节todo").await;
        let free_todo = seed_todo(&db, "自由todo").await;
        let loop_id = seed_loop(&db, "L").await;
        // 启用环节，带内联 review_prompt（v75 新增列）
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled, review_prompt) \
             VALUES ({loop_id}, 's', {todo_id}, 1, '请严格评审')"
        ))
        .await
        .expect("insert step");

        // 命中：返回环节内联 prompt
        let found = db
            .find_loop_step_review_prompt_by_todo(todo_id)
            .await
            .unwrap();
        assert_eq!(found.as_deref(), Some("请严格评审"));

        // 未命中：未被任何环节引用的 todo 返回 None
        let missing = db
            .find_loop_step_review_prompt_by_todo(free_todo)
            .await
            .unwrap();
        assert!(missing.is_none(), "未被环节引用的 todo 应返回 None");
    }

    /// enabled=0 的环节不应被选中：WHERE enabled = 1 是评审只取启用环节的关键过滤，
    /// 禁用环节不参与 loop 执行，其 review_prompt 不应泄漏进评审回退。
    #[tokio::test]
    async fn test_find_loop_step_review_prompt_excludes_disabled() {
        let db = fresh_db().await;
        let todo_id = seed_todo(&db, "环节todo").await;
        let loop_id = seed_loop(&db, "L").await;
        // 环节存在但被禁用 → 必须被过滤掉
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled, review_prompt) \
             VALUES ({loop_id}, 's', {todo_id}, 0, '请严格评审')"
        ))
        .await
        .expect("insert disabled step");

        let res = db
            .find_loop_step_review_prompt_by_todo(todo_id)
            .await
            .unwrap();
        assert!(res.is_none(), "禁用环节不应被选中");
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

    /// 「工艺」列依赖 LoopRefSummary 带 process_template 信息：
    /// 环路绑定了模板时要回填 template id/name；未绑定时为 None。
    #[tokio::test]
    async fn test_get_referencing_loops_includes_process_template() {
        let db = fresh_db().await;
        let todo = seed_todo(&db, "T").await;
        let tpl = seed_process_template(&db, "工艺A").await;
        let loop_with_tpl = seed_loop(&db, "环路1").await;
        let loop_no_tpl = seed_loop(&db, "环路2").await;
        // 把环路1 关联到工艺模板
        db.exec(&format!(
            "UPDATE loops SET process_template_id = {tpl}, process_template_version = '9.9.9' WHERE id = {loop_with_tpl}"
        ))
        .await
        .expect("bind template");
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) \
             VALUES ({loop_with_tpl}, 's1', {todo}, 1), ({loop_no_tpl}, 's2', {todo}, 1)"
        ))
        .await
        .expect("insert steps");

        let map = db.get_referencing_loops_for_todos(&[todo]).await.unwrap();
        let refs = map.get(&todo).expect("T 应有引用");
        // 按 loop_id 升序：绑模板的环路1 在前
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].process_template_id, Some(tpl));
        assert_eq!(refs[0].process_template_name.as_deref(), Some("工艺A"));
        assert_eq!(refs[0].process_template_version.as_deref(), Some("9.9.9"));
        // 未绑模板的环路2 → None
        assert_eq!(refs[1].process_template_id, None);
        assert_eq!(refs[1].process_template_name, None);
        assert_eq!(refs[1].process_template_version, None);
    }

    /// 批量取环路：命中全部 / 部分命中（不存在 id 被忽略）/ 空入参直接返回空。
    ///
    /// 对应任务列表批量注入环路快照的 N+1 优化，空入参短路是避免生成非法 `IN ()` SQL。
    #[tokio::test]
    async fn test_get_loops_by_ids_batch() {
        let db = fresh_db().await;
        let l1 = seed_loop(&db, "环路1").await;
        let l2 = seed_loop(&db, "环路2").await;

        let got = db.get_loops_by_ids(&[l1, l2]).await.expect("batch query");
        assert_eq!(got.len(), 2, "两个存在的环路都应命中");
        assert!(got.iter().any(|l| l.id == l1));
        assert!(got.iter().any(|l| l.id == l2));

        let partial = db.get_loops_by_ids(&[l1, 999_999]).await.expect("partial query");
        assert_eq!(partial.len(), 1, "不存在的 id 应被忽略");
        assert_eq!(partial[0].id, l1);

        let empty = db.get_loops_by_ids(&[]).await.expect("empty query");
        assert!(empty.is_empty(), "空入参应短路返回空 Vec");
    }

    /// 插一条工艺模板，返回 id。
    ///
    /// loops.process_template_id 有外键约束（SQLite 外键开启时），
    /// 测试关联工艺前必须先有真实模板行；必填列为 guid/name/display_name。
    /// 工艺正文存在磁盘，DB 只存 source_path 引用，因此此处不插入 definition 列。
    async fn seed_process_template(db: &Database, name: &str) -> i64 {
        db.exec(&format!(
            "INSERT INTO process_templates (guid, name, display_name) VALUES ('guid-{name}', '{name}', '{name}')"
        ))
        .await
        .expect("insert process_template");
        let row = db
            .conn
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                format!("SELECT id FROM process_templates WHERE name = '{name}'"),
            ))
            .await
            .expect("query process_template id")
            .expect("process_template row exists");
        row.try_get_by("id").expect("process_template id column")
    }

    /// list_loops_by_process_template：只返回指定工艺的实例环路，
    /// 按 id 倒序（created_at 相同的时候兜底稳定），其他工艺/普通环路不出现。
    #[tokio::test]
    async fn test_list_loops_by_process_template() {
        let db = fresh_db().await;
        let t100 = seed_process_template(&db, "tpl-100").await;
        let t200 = seed_process_template(&db, "tpl-200").await;
        let l1 = seed_loop(&db, "PA-1").await;
        let l2 = seed_loop(&db, "PA-2").await;
        let l_other = seed_loop(&db, "PB").await;
        let _plain = seed_loop(&db, "PLAIN").await;
        // seed_loop 只插 name，工艺归属通过 UPDATE 补齐真实模板 id
        db.exec(&format!(
            "UPDATE loops SET process_template_id = {t100} WHERE id IN ({l1}, {l2})"
        ))
        .await
        .expect("mark template 100");
        db.exec(&format!(
            "UPDATE loops SET process_template_id = {t200} WHERE id = {l_other}"
        ))
        .await
        .expect("mark template 200");

        let list = db.list_loops_by_process_template(t100).await.unwrap();
        assert_eq!(list.len(), 2, "模板 100 应有 2 个实例环路");
        // created_at 同值时按 id DESC：后插入的 l2 排前
        assert_eq!(list[0].id, l2, "倒序兜底：id 大者在前");
        assert_eq!(list[1].id, l1);
        assert!(
            db.list_loops_by_process_template(t200).await.unwrap().len() == 1,
            "模板 200 应只有 1 个实例环路"
        );
    }

    /// count_loop_executions_by_loop_ids：一次 GROUP BY 聚合计数，
    /// 无执行的环路不出现在 map 中（调用方按 0 兜底），空输入直接返回空 map。
    #[tokio::test]
    async fn test_count_loop_executions_by_loop_ids() {
        let db = fresh_db().await;
        let l1 = seed_loop(&db, "C1").await;
        let l2 = seed_loop(&db, "C2").await;
        db.exec(&format!(
            "INSERT INTO loop_executions (loop_id, trigger_type, status, started_at) \
             VALUES ({l1}, 'manual', 'success', datetime('now'))"
        ))
        .await
        .expect("insert exec 1");
        db.exec(&format!(
            "INSERT INTO loop_executions (loop_id, trigger_type, status, started_at) \
             VALUES ({l1}, 'cron', 'failed', datetime('now'))"
        ))
        .await
        .expect("insert exec 2");

        let map = db.count_loop_executions_by_loop_ids(&[l1, l2]).await.unwrap();
        assert_eq!(map.get(&l1).copied().unwrap_or(0), 2, "l1 应有 2 次执行");
        assert!(!map.contains_key(&l2), "l2 无执行记录不应出现");
        assert!(
            db.count_loop_executions_by_loop_ids(&[]).await.unwrap().is_empty(),
            "空输入应返回空 map"
        );
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

    /// 093-B5：list_recent_loop_executions_for_task——按 task_id 过滤、started_at 倒序、limit 生效。
    #[tokio::test]
    async fn test_list_recent_loop_executions_for_task() {
        let db = fresh_db().await;
        let lp = seed_loop_status(&db, "L", "enabled").await;
        // 两条挂在 task 7、一条挂在 task 8（task_id 直插，绕开 seed helper 的列集）
        for (task, expr) in [(7, "datetime('now','-2 hours')"), (7, "datetime('now','-1 hours')"), (8, "datetime('now')")] {
            db.exec(&format!(
                "INSERT INTO loop_executions (loop_id, trigger_type, status, started_at, task_id) \
                 VALUES ({lp}, 'manual', 'success', {expr}, {task})"
            ))
            .await
            .expect("insert loop_execution");
        }
        let rows = db.list_recent_loop_executions_for_task(7, 20).await.unwrap();
        assert_eq!(rows.len(), 2, "只应返回 task 7 的记录");
        // 倒序：新的（-1h）在前
        assert!(rows[0].started_at > rows[1].started_at, "应按 started_at 倒序");
        // limit 生效
        let limited = db.list_recent_loop_executions_for_task(7, 1).await.unwrap();
        assert_eq!(limited.len(), 1);
    }

    /// 093-B5：get_artifact_workspace_path——三级跳返回 loop 的 workspace_path；断链报 NotFound。
    #[tokio::test]
    async fn test_get_artifact_workspace_path() {
        let db = fresh_db().await;
        db.exec("INSERT INTO loops (name, workspace_path) VALUES ('L', '/ws/path')").await.expect("insert loop");
        let loop_id = max_id(&db, "loops").await;
        let le = seed_loop_execution(&db, loop_id, "manual", "running", "datetime('now')").await;
        let todo = seed_todo(&db, "T").await;
        let step = seed_loop_step(&db, loop_id, todo, "s1").await;
        // execution_record_id 有外键约束，必须先 seed 真实记录再关联
        let record_id = seed_execution_record(&db, "{}").await;
        link_step_execution(&db, le, step, todo, record_id).await;
        let se_id = max_id(&db, "loop_step_executions").await;

        let path = db.get_artifact_workspace_path(se_id).await.unwrap();
        assert_eq!(path, Some("/ws/path".to_string()));
        // 断链：不存在的 step_execution_id → RecordNotFound
        assert!(db.get_artifact_workspace_path(99999).await.is_err());
    }

    /// set_step_execution_min_rating：阈值回写后能被读出；
    /// gate_config 风格步骤（step.min_rating 为 NULL）依赖这条路径让前端显示「阈值 N」。
    #[tokio::test]
    async fn test_set_step_execution_min_rating_updates_column() {
        let db = fresh_db().await;
        let todo = seed_todo(&db, "T").await;
        let lp = seed_loop_status(&db, "L", "enabled").await;
        let step = seed_loop_step(&db, lp, todo, "s1").await;
        let le = seed_loop_execution(&db, lp, "manual", "running", "datetime('now')").await;
        // 直接插入无阈值的 step execution（模拟 gate_config 风格步骤创建时的状态）
        db.exec(&format!(
            "INSERT INTO loop_step_executions (loop_execution_id, step_id, todo_id, status) \
             VALUES ({le}, {step}, {todo}, 'running')"
        ))
        .await
        .expect("insert step_execution");
        let se = max_id(&db, "loop_step_executions").await;

        db.set_step_execution_min_rating(se, 60)
            .await
            .expect("set min_rating");

        let row = db
            .conn
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                format!("SELECT min_rating AS m FROM loop_step_executions WHERE id={se}"),
            ))
            .await
            .expect("query min_rating")
            .expect("row exists");
        assert_eq!(row.try_get_by::<Option<i32>, _>("m").unwrap_or(None), Some(60));
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod loop_approval_tests {
    use crate::db::Database;
    use sea_orm::{ConnectionTrait, DbBackend, Statement};

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    /// 取某表当前最大 id（等价于刚插入行的 id；用 MAX 因连接池不保证同一连接）。
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

    /// list_loop_step_executions_by_exec_ids：按 loop_execution_id 批量取环节执行并分组，
    /// 组内按 sequence_index 升序；未挂环节执行的 exec 不出现；空入参返空（091 批量化新增）。
    #[tokio::test]
    async fn test_list_loop_step_executions_by_exec_ids_groups_and_orders() {
        let db = fresh_db().await;
        db.exec("INSERT INTO todos (title, prompt, status) VALUES ('t','p','pending')")
            .await
            .expect("insert todo");
        let todo_id = max_id(&db, "todos").await;
        db.exec("INSERT INTO loops (name) VALUES ('L')").await.expect("insert loop");
        let loop_id = max_id(&db, "loops").await;
        // 两个 loop_execution：le1 挂环节执行，le2 不挂（验证未命中不出现在 map）。
        db.exec(&format!(
            "INSERT INTO loop_executions (loop_id, trigger_type, status, started_at) \
             VALUES ({loop_id}, 'manual', 'running', datetime('now'))"
        ))
        .await
        .expect("insert le1");
        let le1 = max_id(&db, "loop_executions").await;
        db.exec(&format!(
            "INSERT INTO loop_executions (loop_id, trigger_type, status, started_at) \
             VALUES ({loop_id}, 'manual', 'success', datetime('now'))"
        ))
        .await
        .expect("insert le2");
        let le2 = max_id(&db, "loop_executions").await;
        // 两个真实 loop_step（取自增 id 作 step_id，避免 FK 悬空）。
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) VALUES ({loop_id}, 'a', {todo_id}, 1)"
        ))
        .await
        .expect("insert step a");
        let step_a = max_id(&db, "loop_steps").await;
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) VALUES ({loop_id}, 'b', {todo_id}, 1)"
        ))
        .await
        .expect("insert step b");
        let step_b = max_id(&db, "loop_steps").await;
        // le1 两条 step_execution：故意先插 seq=2 再插 seq=1，验证返回按 seq 升序（非插入序）。
        db.exec(&format!(
            "INSERT INTO loop_step_executions (loop_execution_id, step_id, todo_id, sequence_index, status) \
             VALUES ({le1}, {step_a}, {todo_id}, 2, 'success')"
        ))
        .await
        .expect("insert se seq=2");
        db.exec(&format!(
            "INSERT INTO loop_step_executions (loop_execution_id, step_id, todo_id, sequence_index, status) \
             VALUES ({le1}, {step_b}, {todo_id}, 1, 'success')"
        ))
        .await
        .expect("insert se seq=1");
        let map = db
            .list_loop_step_executions_by_exec_ids(&[le1, le2, 9999])
            .await
            .unwrap();
        let le1_rows = map.get(&le1).expect("le1 应有环节执行");
        assert_eq!(le1_rows.len(), 2, "le1 应有两条环节执行");
        assert_eq!(le1_rows[0].sequence_index, 1, "组内按 sequence_index 升序");
        assert_eq!(le1_rows[1].sequence_index, 2);
        assert!(!map.contains_key(&le2), "le2 无环节执行不应出现");
        assert!(
            db.list_loop_step_executions_by_exec_ids(&[]).await.unwrap().is_empty(),
            "空入参应返回空 map"
        );
    }

    /// 造一条 pending_approval 的环节执行记录（含 todo/loop/step/execution 四级外键），
    /// 返回 (loop_execution_id, step_execution_id)。
    async fn seed_pending_step_execution(db: &Database) -> (i64, i64) {
        db.exec("INSERT INTO todos (title, prompt, status) VALUES ('t', 'p', 'pending')")
            .await
            .expect("insert todo");
        let todo_id = max_id(db, "todos").await;
        db.exec("INSERT INTO loops (name) VALUES ('L')")
            .await
            .expect("insert loop");
        let loop_id = max_id(db, "loops").await;
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) VALUES ({loop_id}, 's', {todo_id}, 1)"
        ))
        .await
        .expect("insert step");
        let step_id = max_id(db, "loop_steps").await;
        db.exec(&format!(
            "INSERT INTO loop_executions (loop_id, trigger_type, status, started_at) \
             VALUES ({loop_id}, 'manual', 'running', datetime('now'))"
        ))
        .await
        .expect("insert loop_execution");
        let exec_id = max_id(db, "loop_executions").await;
        db.exec(&format!(
            "INSERT INTO loop_step_executions (loop_execution_id, step_id, todo_id, status, sequence_index) \
             VALUES ({exec_id}, {step_id}, {todo_id}, 'pending_approval', 1)"
        ))
        .await
        .expect("insert step_execution");
        let se_id = max_id(db, "loop_step_executions").await;
        (exec_id, se_id)
    }

    /// 审批通过落库：status/rating/approval_status/comment 全部写入。
    /// approval_status='approved' 是 resume_loop_execution 定位待恢复环节的查找条件（NTD-004）。
    #[tokio::test]
    async fn test_approve_step_execution_approved_writes_terminal_state() {
        let db = fresh_db().await;
        let (exec_id, se_id) = seed_pending_step_execution(&db).await;

        db.approve_step_execution(se_id, 100, "success", Some("同意上线"))
            .await
            .expect("approve");

        let list = db.list_loop_step_executions(exec_id).await.expect("list");
        let se = list.iter().find(|s| s.id == se_id).expect("target row");
        assert_eq!(se.status, "success");
        assert_eq!(se.rating, Some(100));
        assert_eq!(se.approval_status.as_deref(), Some("approved"));
        assert_eq!(se.approval_comment.as_deref(), Some("同意上线"));
    }

    /// 审批拒绝落库：status=failed、rating=0，resume 据此走 on_rating_fail 分支。
    #[tokio::test]
    async fn test_approve_step_execution_rejected_writes_failed() {
        let db = fresh_db().await;
        let (exec_id, se_id) = seed_pending_step_execution(&db).await;

        db.approve_step_execution(se_id, 0, "failed", None)
            .await
            .expect("approve");

        let list = db.list_loop_step_executions(exec_id).await.expect("list");
        let se = list.iter().find(|s| s.id == se_id).expect("target row");
        assert_eq!(se.status, "failed");
        assert_eq!(se.rating, Some(0));
        assert_eq!(se.approval_status.as_deref(), Some("approved"));
        assert!(se.approval_comment.is_none());
    }

    /// 待审批计数覆盖工艺路径：phase_driver 暂停时只写 status='pending_approval'，
    /// 不写 approval_status；计数必须能统计到，否则前端「N 待审批」角标恒为 0（NTD-004）。
    #[tokio::test]
    async fn test_count_pending_approvals_covers_process_path() {
        let db = fresh_db().await;
        let (exec_id, _se_id) = seed_pending_step_execution(&db).await;

        let counts = db
            .count_pending_approvals_by_execution_ids(&[exec_id])
            .await
            .expect("count");
        assert_eq!(counts.get(&exec_id).copied().unwrap_or(0), 1);
    }

    /// 待审批计数兼容旧评分路径：暂停时写 approval_status='pending'（status 也是 pending_approval）。
    /// 两个条件同时命中时 OR 不重复计数。
    #[tokio::test]
    async fn test_count_pending_approvals_legacy_path_not_double_counted() {
        let db = fresh_db().await;
        let (exec_id, se_id) = seed_pending_step_execution(&db).await;
        // 旧路径会额外写 approval_status='pending'（loop_runner.rs 暂停分支）。
        db.set_step_execution_approval_status(se_id, "pending")
            .await
            .expect("set approval_status");

        let counts = db
            .count_pending_approvals_by_execution_ids(&[exec_id])
            .await
            .expect("count");
        assert_eq!(
            counts.get(&exec_id).copied().unwrap_or(0),
            1,
            "status 与 approval_status 同时命中应按一行计一次"
        );
    }

    /// 审批完成后不再计入待审批。
    #[tokio::test]
    async fn test_count_pending_approvals_excludes_approved() {
        let db = fresh_db().await;
        let (exec_id, se_id) = seed_pending_step_execution(&db).await;
        db.approve_step_execution(se_id, 100, "success", None)
            .await
            .expect("approve");

        let counts = db
            .count_pending_approvals_by_execution_ids(&[exec_id])
            .await
            .expect("count");
        assert_eq!(counts.get(&exec_id).copied().unwrap_or(0), 0);
    }

    /// 造一条带 task_id 的 pending_approval 环节执行（063 测试辅助），
    /// 返回 (task_id, step_execution_id)。复用 seed 的四级外键建法，仅在 loop_executions
    /// 上额外挂 task_id（列可空，INSERT 时需先建 tasks 行满足 FK）。
    async fn seed_pending_step_execution_with_task(db: &Database) -> (i64, i64) {
        db.exec("INSERT INTO todos (title, prompt, status) VALUES ('t', 'p', 'pending')")
            .await
            .expect("insert todo");
        let todo_id = max_id(db, "todos").await;
        db.exec("INSERT INTO loops (name) VALUES ('L')")
            .await
            .expect("insert loop");
        let loop_id = max_id(db, "loops").await;
        db.exec(&format!(
            "INSERT INTO loop_steps (loop_id, name, todo_id, enabled) VALUES ({loop_id}, 's', {todo_id}, 1)"
        ))
        .await
        .expect("insert step");
        let step_id = max_id(db, "loop_steps").await;
        // loop_executions.task_id 有 FK→tasks，先建 task 行再引用（与既有 task 测试同模式）。
        db.exec("INSERT INTO tasks (title, description, status, created_by) VALUES ('T','d','running','test')")
            .await
            .expect("insert task");
        let task_id = max_id(db, "tasks").await;
        db.exec(&format!(
            "INSERT INTO loop_executions (loop_id, trigger_type, status, started_at, task_id) \
             VALUES ({loop_id}, 'manual', 'running', datetime('now'), {task_id})"
        ))
        .await
        .expect("insert loop_execution");
        let exec_id = max_id(db, "loop_executions").await;
        db.exec(&format!(
            "INSERT INTO loop_step_executions (loop_execution_id, step_id, todo_id, status, sequence_index) \
             VALUES ({exec_id}, {step_id}, {todo_id}, 'pending_approval', 1)"
        ))
        .await
        .expect("insert step_execution");
        let se_id = max_id(db, "loop_step_executions").await;
        (task_id, se_id)
    }

    /// 按 task 统计：同一 task 多条执行各挂 1 条待审批时应累加，
    /// 避免旧执行滞留的审批被新执行掩盖（063 口径决策）。
    #[tokio::test]
    async fn test_count_pending_approvals_by_task_ids_aggregates_across_executions() {
        let db = fresh_db().await;
        let (task_id, _se1) = seed_pending_step_execution_with_task(&db).await;
        // 同一 task 再跑一次执行（复用同一 loop/step/todo），同样停在待审批。
        let loop_id: i64 = db
            .conn
            .query_one(Statement::from_string(DbBackend::Sqlite, "SELECT MAX(id) AS m FROM loops"))
            .await
            .expect("query loop id")
            .expect("loop row exists")
            .try_get_by("m")
            .expect("loop id readable");
        let step_id = max_id(&db, "loop_steps").await;
        let todo_id = max_id(&db, "todos").await;
        db.exec(&format!(
            "INSERT INTO loop_executions (loop_id, trigger_type, status, started_at, task_id) \
             VALUES ({loop_id}, 'manual', 'running', datetime('now'), {task_id})"
        ))
        .await
        .expect("insert second execution");
        let exec2 = max_id(&db, "loop_executions").await;
        db.exec(&format!(
            "INSERT INTO loop_step_executions (loop_execution_id, step_id, todo_id, status, sequence_index) \
             VALUES ({exec2}, {step_id}, {todo_id}, 'pending_approval', 1)"
        ))
        .await
        .expect("insert second step_execution");

        let counts = db
            .count_pending_approvals_by_task_ids(&[task_id])
            .await
            .expect("count");
        assert_eq!(
            counts.get(&task_id).copied().unwrap_or(0),
            2,
            "同一 task 两条执行的待审批应累加"
        );
    }

    /// 按 task 统计同样覆盖两条暂停路径（NTD-004 口径）：旧评分路径写 approval_status='pending'，
    /// 工艺路径只写 status='pending_approval'，两者都必须被计入。
    #[tokio::test]
    async fn test_count_pending_approvals_by_task_ids_covers_both_paths() {
        let db = fresh_db().await;
        // seed 产生的是工艺路径（仅 status='pending_approval'）。
        let (task_id, se_id) = seed_pending_step_execution_with_task(&db).await;
        // 追加旧评分路径标记：approval_status='pending'；与 status 同时命中时 OR 不重复计数。
        db.set_step_execution_approval_status(se_id, "pending")
            .await
            .expect("set approval_status");

        let counts = db
            .count_pending_approvals_by_task_ids(&[task_id])
            .await
            .expect("count");
        assert_eq!(
            counts.get(&task_id).copied().unwrap_or(0),
            1,
            "两条路径条件同时命中应按一行计一次"
        );
    }

    /// 边界：空入参返回空 map（避免拼出 IN () 非法 SQL）；无待审批的 task 不出现在 map。
    #[tokio::test]
    async fn test_count_pending_approvals_by_task_ids_empty_and_absent() {
        let db = fresh_db().await;
        assert!(
            db.count_pending_approvals_by_task_ids(&[])
                .await
                .expect("count")
                .is_empty(),
            "空入参应返回空 map"
        );
        let (_task_id, se_id) = seed_pending_step_execution_with_task(&db).await;
        // 审批完成后不再计入；map 中不应出现该 task。
        db.approve_step_execution(se_id, 100, "success", None)
            .await
            .expect("approve");
        let counts = db
            .count_pending_approvals_by_task_ids(&[1])
            .await
            .expect("count");
        assert!(counts.is_empty(), "审批完成后 task 不应出现在 map");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod loop_phase_finalization_tests {
    use crate::db::Database;
    use sea_orm::ConnectionTrait;

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    /// 造一条 loop + phase + phase_execution，phase 状态为 running。
    async fn seed_running_phase(db: &Database) -> (i64, i64) {
        db.exec("INSERT INTO loops (name) VALUES ('L')")
            .await
            .expect("insert loop");
        let loop_id = max_id(db, "loops").await;

        db.exec(&format!(
            "INSERT INTO loop_phases (loop_id, name) VALUES ({loop_id}, 'P1')"
        ))
        .await
        .expect("insert phase");
        let phase_id = max_id(db, "loop_phases").await;

        db.exec(&format!(
            "INSERT INTO loop_executions (loop_id, status, trigger_type, started_at) \
             VALUES ({loop_id}, 'running', 'manual', '2026-01-01T00:00:00Z')"
        ))
        .await
        .expect("insert loop_exec");
        let exec_id = max_id(db, "loop_executions").await;

        db.exec(&format!(
            "INSERT INTO loop_phase_executions (loop_execution_id, phase_id, status, started_at) \
             VALUES ({exec_id}, {phase_id}, 'running', '2026-01-01T00:00:00Z')"
        ))
        .await
        .expect("insert phase_exec");
        (exec_id, phase_id)
    }

    async fn max_id(db: &Database, table: &str) -> i64 {
        let sql = format!("SELECT MAX(id) AS m FROM {table}");
        let row = db
            .conn
            .query_one(sea_orm::Statement::from_string(sea_orm::DbBackend::Sqlite, sql))
            .await
            .expect("query max id")
            .expect("max id row exists");
        row.try_get_by::<i64, _>("m").unwrap_or(0)
    }

    #[tokio::test]
    async fn test_finalize_phase_executions_marks_running_success() {
        let db = fresh_db().await;
        let (exec_id, _phase_id) = seed_running_phase(&db).await;

        db.finalize_phase_executions(exec_id, "success")
            .await
            .expect("finalize");

        // 验证 phase 被标为 success 且有 finished_at
        let row = db
            .conn
            .query_one(sea_orm::Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                "SELECT status FROM loop_phase_executions WHERE loop_execution_id = ?",
                [sea_orm::Value::from(exec_id)],
            ))
            .await
            .expect("query")
            .expect("row exists");
        assert_eq!(row.try_get_by::<String, _>("status").unwrap(), "success");
    }

    #[tokio::test]
    async fn test_finalize_phase_executions_marks_running_failed() {
        let db = fresh_db().await;
        let (exec_id, _phase_id) = seed_running_phase(&db).await;

        db.finalize_phase_executions(exec_id, "failed")
            .await
            .expect("finalize");

        let row = db
            .conn
            .query_one(sea_orm::Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                "SELECT status FROM loop_phase_executions WHERE loop_execution_id = ?",
                [sea_orm::Value::from(exec_id)],
            ))
            .await
            .expect("query")
            .expect("row exists");
        assert_eq!(row.try_get_by::<String, _>("status").unwrap(), "failed");
    }
}
