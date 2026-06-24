use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, Statement,
};

use crate::db::entity::tags;
use crate::db::entity::{steps, todo_tags, todos};
use crate::db::Database;
use crate::models::{Todo, TodoBackup, TodoStatus};

pub struct TodoUpdate<'a> {
    pub id: i64,
    pub title: &'a str,
    pub prompt: &'a str,
    pub status: TodoStatus,
    pub executor: Option<&'a str>,
    pub scheduler_enabled: Option<bool>,
    pub scheduler_config: Option<&'a str>,
    pub scheduler_timezone: Option<&'a str>,
    pub workspace: Option<&'a str>,
    pub worktree_enabled: Option<bool>,
    pub acceptance_criteria: Option<&'a str>,
    pub auto_review_enabled: Option<bool>,
}

pub struct SchedulerUpdate<'a> {
    pub id: i64,
    pub enabled: bool,
    pub config: Option<&'a str>,
    pub timezone: Option<&'a str>,
}

impl Database {
    fn model_to_todo(m: todos::Model, tag_ids: Vec<i64>) -> Todo {
        let scheduler_enabled = m.scheduler_enabled.unwrap_or(false);
        let scheduler_config = m.scheduler_config.clone();
        let scheduler_timezone = m.scheduler_timezone.clone();
        let scheduler_next_run_at = if scheduler_enabled {
            scheduler_config
                .as_deref()
                .and_then(|config| {
                    super::compute_next_run(config, scheduler_timezone.as_deref())
                })
        } else {
            None
        };
        Todo {
            id: m.id,
            title: m.title,
            prompt: m.prompt.unwrap_or_default(),
            status: m
                .status
                .as_deref()
                .and_then(|s| s.parse().ok())
                .unwrap_or(TodoStatus::Pending),
            created_at: m.created_at.unwrap_or_default(),
            updated_at: m.updated_at.unwrap_or_default(),
            tag_ids,
            executor: m.executor,
            scheduler_enabled,
            scheduler_config,
            scheduler_timezone,
            scheduler_next_run_at,
            task_id: m.task_id,
            workspace: m.workspace,
            worktree_enabled: m.worktree_enabled.unwrap_or(false),
            acceptance_criteria: m.acceptance_criteria,
            todo_type: m.todo_type.unwrap_or(0),
            parent_todo_id: m.parent_todo_id,
            review_template_id: m.review_template_id,
            auto_review_enabled: m.auto_review_enabled.unwrap_or(true),
            // kind 默认 'item'；实际数据由 v3 migration 注入。
            // unwrap_or_default 兜底 None(例如老库 v3 升级前的行),与字段语义保持一致。
            kind: m.kind.unwrap_or_else(|| "item".to_string()),
        }
    }

    pub(crate) async fn fetch_tag_ids_for_many(
        &self,
        todo_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, Vec<i64>>, sea_orm::DbErr> {
        if todo_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let models = todo_tags::Entity::find()
            .filter(todo_tags::Column::TodoId.is_in(todo_ids.to_vec()))
            .all(&self.conn)
            .await?;
        Ok(models
            .into_iter()
            .fold(std::collections::HashMap::new(), |mut map, t| {
                map.entry(t.todo_id).or_default().push(t.tag_id);
                map
            }))
    }

    pub async fn get_todos(&self) -> Result<Vec<Todo>, sea_orm::DbErr> {
        let models = todos::Entity::find()
            .filter(todos::Column::DeletedAt.is_null())
            .order_by_desc(todos::Column::UpdatedAt)
            .all(&self.conn)
            .await?;

        let ids: Vec<i64> = models.iter().map(|m| m.id).collect();
        let tag_map = self.fetch_tag_ids_for_many(&ids).await?;

        Ok(models
            .into_iter()
            .map(|m| {
                let tag_ids = tag_map.get(&m.id).cloned().unwrap_or_default();
                Self::model_to_todo(m, tag_ids)
            })
            .collect())
    }

    pub async fn create_todo(&self, title: &str, prompt: &str) -> Result<i64, sea_orm::DbErr> {
        self.create_todo_with_executor(title, prompt, Some(crate::adapters::DEFAULT_EXECUTOR)).await
    }

    /// 创建 Todo，可指定执行器。
    /// executor 为 None、空串或仅空白时默认为 claudecode（防止空/空白字符串污染 DB）。
    pub async fn create_todo_with_executor(&self, title: &str, prompt: &str, executor: Option<&str>) -> Result<i64, sea_orm::DbErr> {
        self.create_todo_with_extras(title, prompt, executor, None).await
    }

    /// 创建 Todo，带所有可选字段。
    pub async fn create_todo_with_extras(
        &self,
        title: &str,
        prompt: &str,
        executor: Option<&str>,
        acceptance_criteria: Option<&str>,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let executor_str = executor
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(crate::adapters::DEFAULT_EXECUTOR);
        let am = todos::ActiveModel {
            title: ActiveValue::Set(title.to_string()),
            prompt: ActiveValue::Set(Some(prompt.to_string())),
            status: ActiveValue::Set(Some(TodoStatus::Pending.to_string())),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            executor: ActiveValue::Set(Some(executor_str.to_string())),
            acceptance_criteria: ActiveValue::Set(acceptance_criteria.map(|s| s.to_string())),
            auto_review_enabled: ActiveValue::Set(Some(true)),
            todo_type: ActiveValue::Set(Some(0)),
            ..Default::default()
        };
        let inserted = am.insert(&self.conn).await?;
        Ok(inserted.id)
    }

    pub async fn update_todo_full(&self, update: TodoUpdate<'_>) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let mut am = todos::ActiveModel {
            id: ActiveValue::Unchanged(update.id),
            title: ActiveValue::Set(update.title.to_string()),
            prompt: ActiveValue::Set(Some(update.prompt.to_string())),
            status: ActiveValue::Set(Some(update.status.to_string())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        if let Some(exec) = update.executor {
            am.executor = ActiveValue::Set(Some(exec.to_string()));
        }
        if let Some(enabled) = update.scheduler_enabled {
            am.scheduler_enabled = ActiveValue::Set(Some(enabled));
        }
        if let Some(cfg) = update.scheduler_config {
            am.scheduler_config = ActiveValue::Set(Some(cfg.to_string()));
        }
        if let Some(tz) = update.scheduler_timezone {
            if tz.is_empty() {
                am.scheduler_timezone = ActiveValue::Set(None);
            } else {
                am.scheduler_timezone = ActiveValue::Set(Some(tz.to_string()));
            }
        }
        if let Some(ws) = update.workspace {
            let ws = ws.trim();
            if ws.is_empty() {
                am.workspace = ActiveValue::Set(None);
            } else {
                am.workspace = ActiveValue::Set(Some(ws.to_string()));
            }
        }
        if let Some(wt) = update.worktree_enabled {
            am.worktree_enabled = ActiveValue::Set(Some(wt));
        }
        if let Some(criteria) = update.acceptance_criteria {
            am.acceptance_criteria = ActiveValue::Set(Some(criteria.to_string()));
        }
        if let Some(enabled) = update.auto_review_enabled {
            am.auto_review_enabled = ActiveValue::Set(Some(enabled));
        }
        self.exec_update(am).await
    }

    pub async fn update_todo_executor(
        &self,
        id: i64,
        executor: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            executor: ActiveValue::Set(Some(executor.to_string())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 批量更新事项执行器（单条 SQL，原子语义）。
    pub async fn batch_update_todos_executor(
        &self,
        ids: &[i64],
        executor: &str,
    ) -> Result<u64, sea_orm::DbErr> {
        if ids.is_empty() {
            return Ok(0);
        }
        let now = crate::models::utc_timestamp();
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{}", i)).collect();
        let in_clause = placeholders.join(",");
        let executor_idx = ids.len() + 1;
        let now_idx = ids.len() + 2;
        let sql = format!(
            "UPDATE todos SET executor = ?{executor_idx}, updated_at = ?{now_idx} WHERE id IN ({in_clause})"
        );
        let mut vals: Vec<sea_orm::Value> = ids.iter().map(|id| (*id).into()).collect();
        vals.push(executor.to_string().into());
        vals.push(now.into());
        let stmt = sea_orm::Statement::from_sql_and_values(sea_orm::DbBackend::Sqlite, sql, vals);
        let rows_affected = self.conn.execute(stmt).await?.rows_affected();
        Ok(rows_affected)
    }

    /// todo hook 已整块移除（见 plan `purring-forging-petal`），`update_todo_hooks` 不再存在：
    /// todo 的 `hooks` 字段与 `todos.hooks` 列随 V23 迁移一起删掉。

    pub async fn update_todo_task_id(
        &self,
        id: i64,
        task_id: Option<&str>,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            task_id: ActiveValue::Set(task_id.map(|s| s.to_string())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    pub async fn update_todo_scheduler(
        &self,
        req: SchedulerUpdate<'_>,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        // Normalize empty strings to None
        let timezone = req.timezone.filter(|s| !s.is_empty());
        let config = req.config.filter(|s| !s.is_empty());
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(req.id),
            scheduler_enabled: ActiveValue::Set(Some(req.enabled)),
            scheduler_config: ActiveValue::Set(config.map(|s| s.to_string())),
            scheduler_timezone: ActiveValue::Set(timezone.map(|s| s.to_string())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    pub async fn update_todo_workspace(
        &self,
        id: i64,
        workspace: Option<&str>,
    ) -> Result<(), sea_orm::DbErr> {
        let ws = workspace.and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            workspace: ActiveValue::Set(ws),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    pub async fn update_todo_worktree_enabled(
        &self,
        id: i64,
        enabled: bool,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            worktree_enabled: ActiveValue::Set(Some(enabled)),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 单独更新 auto_review_enabled. 在 create_todo 之后被 handler 调用, 以接受
    /// 来自请求的覆盖. review_instance / reviewer_template 类型不允许改这个开关.
    pub async fn update_todo_auto_review_enabled(
        &self,
        id: i64,
        enabled: bool,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            auto_review_enabled: ActiveValue::Set(Some(enabled)),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 按 title 精确查找 todo (仅未软删的). 用于评审任务 todo 的探测.
    pub async fn get_todo_by_title(&self, title: &str) -> Result<Option<Todo>, sea_orm::DbErr> {
        let model = todos::Entity::find()
            .filter(todos::Column::Title.eq(title))
            .filter(todos::Column::DeletedAt.is_null())
            .one(&self.conn)
            .await?;
        let Some(m) = model else { return Ok(None) };
        let tag_ids = todo_tags::Entity::find()
            .filter(todo_tags::Column::TodoId.eq(m.id))
            .all(&self.conn)
            .await?
            .into_iter()
            .map(|t| t.tag_id)
            .collect();
        Ok(Some(Self::model_to_todo(m, tag_ids)))
    }

    /// 设置 todo_type. 主要用于将刚 create_todo_with_extras 出来的 todo 标记为
    /// 评审任务 (1) 或 评审实例 (2). 不在公共 API 暴露.
    pub async fn set_todo_type(&self, id: i64, todo_type: i32) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            todo_type: ActiveValue::Set(Some(todo_type)),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 创建一个"评审实例" todo (todo_type=2)。
    /// 设计原因: V15 之后 review_template 是独立表 (不再挂 todo_type=1),
    /// 评审模板不再有 executor 字段。执行评审时需要新建一条 todo:
    /// - `prompt` = caller 合成好的评审 prompt (含原 output 截断 + 模板占位符替换)
    /// - `executor` = 从被评审的 record/original todo 继承 (review_template 不带 executor)
    /// - `todo_type` = 2 (评审实例)
    /// - `parent_todo_id` = 源 todo id (loop 触发时为 0, 因为 loop step 没有单一 source todo)
    /// - `review_template_id` = 使用的评审模板 id
    /// - `auto_review_enabled` = false (评审实例自身不再评审, 防止无限嵌套)
    /// 评审实例是 transient 的, 不挂 hooks / scheduler.
    pub async fn create_review_instance_todo(
        &self,
        parent_todo_id: i64,
        review_template_id: i64,
        review_template_name: &str,
        composed_prompt: String,
        executor: Option<String>,
    ) -> Result<i64, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let title = format!("[评审] {}", review_template_name);
        let am = todos::ActiveModel {
            title: ActiveValue::Set(title),
            // todos.prompt 列是 Option<String>; 新建的评审实例一定有 prompt, 直接 Some 包一层.
            prompt: ActiveValue::Set(Some(composed_prompt)),
            status: ActiveValue::Set(Some(TodoStatus::Pending.to_string())),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            executor: ActiveValue::Set(executor),
            todo_type: ActiveValue::Set(Some(2)),
            parent_todo_id: ActiveValue::Set(Some(parent_todo_id)),
            review_template_id: ActiveValue::Set(Some(review_template_id)),
            auto_review_enabled: ActiveValue::Set(Some(false)),
            ..Default::default()
        };
        let inserted = am.insert(&self.conn).await?;
        Ok(inserted.id)
    }

    /// 根据 review_template_id 查找一条未删除的评审实例 todo (todo_type=2)。
    ///
    /// 复用语义：同一评审模板的所有评审执行共享同一条评审实例 todo,
    /// 避免 todos 表被「同一模板 N 次评审 → N 条 todo」刷屏。
    /// 多条匹配时返回 id 最大（最新创建）的那条，
    /// 保证 V17 数据清理前老数据也能被定位到。
    pub async fn find_review_instance_by_template(
        &self,
        review_template_id: i64,
    ) -> Result<Option<todos::Model>, sea_orm::DbErr> {
        todos::Entity::find()
            .filter(todos::Column::TodoType.eq(2_i32))
            .filter(todos::Column::ReviewTemplateId.eq(review_template_id))
            .filter(todos::Column::DeletedAt.is_null())
            .order_by_desc(todos::Column::Id)
            .one(&self.conn)
            .await
    }

    /// 复用现有评审实例 todo:重置 prompt/executor/status/updated_at,
    /// 保留 todo id 和 execution_records 关联(历史 record 仍可见)。
    ///
    /// 调用方负责先调 `find_review_instance_by_template` 拿到 id;
    /// 找不到时不要调本方法,应改走 `create_review_instance_todo`。
    pub async fn reset_review_instance_for_reuse(
        &self,
        id: i64,
        new_prompt: &str,
        new_executor: Option<&str>,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            prompt: ActiveValue::Set(Some(new_prompt.to_string())),
            executor: ActiveValue::Set(new_executor.map(|s| s.to_string())),
            status: ActiveValue::Set(Some(TodoStatus::Pending.to_string())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    pub async fn force_update_todo_status(
        &self,
        id: i64,
        status: TodoStatus,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            status: ActiveValue::Set(Some(status.to_string())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    pub async fn delete_todo(&self, id: i64) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            deleted_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    pub async fn get_todo(&self, id: i64) -> Result<Option<Todo>, sea_orm::DbErr> {
        let model = match todos::Entity::find_by_id(id)
            .filter(todos::Column::DeletedAt.is_null())
            .one(&self.conn)
            .await?
        {
            Some(m) => m,
            None => return Ok(None),
        };
        let tag_ids = todo_tags::Entity::find()
            .filter(todo_tags::Column::TodoId.eq(id))
            .all(&self.conn)
            .await?
            .into_iter()
            .map(|t| t.tag_id)
            .collect();
        Ok(Some(Self::model_to_todo(model, tag_ids)))
    }

    pub async fn get_scheduler_todos(&self) -> Result<Vec<Todo>, sea_orm::DbErr> {
        let models = todos::Entity::find()
            .filter(todos::Column::DeletedAt.is_null())
            .filter(todos::Column::SchedulerEnabled.eq(true))
            .filter(todos::Column::SchedulerConfig.is_not_null())
            .all(&self.conn)
            .await?;

        let ids: Vec<i64> = models.iter().map(|m| m.id).collect();
        let tag_map = self.fetch_tag_ids_for_many(&ids).await?;

        Ok(models
            .into_iter()
            .map(|m| {
                let tag_ids = tag_map.get(&m.id).cloned().unwrap_or_default();
                Self::model_to_todo(m, tag_ids)
            })
            .collect())
    }

    pub async fn get_running_todos(&self) -> Result<Vec<Todo>, sea_orm::DbErr> {
        let models = todos::Entity::find()
            .filter(todos::Column::DeletedAt.is_null())
            .filter(todos::Column::Status.eq(TodoStatus::Running.to_string()))
            .filter(todos::Column::TaskId.is_not_null())
            .all(&self.conn)
            .await?;

        let ids: Vec<i64> = models.iter().map(|m| m.id).collect();
        let tag_map = self.fetch_tag_ids_for_many(&ids).await?;

        Ok(models
            .into_iter()
            .map(|m| {
                let tag_ids = tag_map.get(&m.id).cloned().unwrap_or_default();
                Self::model_to_todo(m, tag_ids)
            })
            .collect())
    }

    pub async fn update_todo_status(
        &self,
        todo_id: i64,
        status: TodoStatus,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(todo_id),
            status: ActiveValue::Set(Some(status.to_string())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    pub async fn start_todo_execution(
        &self,
        todo_id: i64,
        task_id: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(todo_id),
            status: ActiveValue::Set(Some(TodoStatus::Running.to_string())),
            task_id: ActiveValue::Set(Some(task_id.to_string())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    pub async fn finish_todo_execution(
        &self,
        todo_id: i64,
        success: bool,
    ) -> Result<(), sea_orm::DbErr> {
        if todo_id == 0 { return Ok(()); } // 环节独立执行
        let status = if success {
            TodoStatus::Completed
        } else {
            TodoStatus::Failed
        };
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(todo_id),
            status: ActiveValue::Set(Some(status.to_string())),
            task_id: ActiveValue::Set(None),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

    /// 根据task_id查找对应的todo
    pub async fn get_todo_by_task_id(&self, task_id: &str) -> Result<Option<Todo>, sea_orm::DbErr> {
        let model = match todos::Entity::find()
            .filter(todos::Column::TaskId.eq(task_id))
            .filter(todos::Column::DeletedAt.is_null())
            .one(&self.conn)
            .await?
        {
            Some(m) => m,
            None => return Ok(None),
        };
        let tag_map = self.fetch_tag_ids_for_many(&[model.id]).await?;
        let tag_ids = tag_map.get(&model.id).cloned().unwrap_or_default();
        Ok(Some(Self::model_to_todo(model, tag_ids)))
    }

    /// 获取所有 todo 的备份数据（非软删除），包含标签名称
    pub async fn get_todo_backups(&self) -> Result<Vec<TodoBackup>, sea_orm::DbErr> {
        let models = todos::Entity::find()
            .filter(todos::Column::DeletedAt.is_null())
            .all(&self.conn)
            .await?;

        let ids: Vec<i64> = models.iter().map(|m| m.id).collect();
        let tag_map = self.fetch_tag_ids_for_many(&ids).await?;

        // 获取所有标签 id -> name 映射
        let all_tags: std::collections::HashMap<i64, String> = tags::Entity::find()
            .all(&self.conn)
            .await?
            .into_iter()
            .map(|t| (t.id, t.name))
            .collect();

        Ok(models
            .into_iter()
            .map(|m| {
                let tag_ids = tag_map.get(&m.id).cloned().unwrap_or_default();
                let tag_names: Vec<String> = tag_ids
                    .iter()
                    .filter_map(|tid| all_tags.get(tid).cloned())
                    .collect();
                TodoBackup {
                    title: m.title,
                    prompt: m.prompt.unwrap_or_default(),
                    status: m
                        .status
                        .as_deref()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(TodoStatus::Pending),
                    executor: m.executor,
                    scheduler_enabled: m.scheduler_enabled.unwrap_or(false),
                    scheduler_config: m.scheduler_config,
                    tag_names,
                    workspace: m.workspace.clone(),
                    worktree: if m.worktree_enabled.unwrap_or(false) {
                        m.workspace.clone()
                    } else {
                        None
                    },
                }
            })
            .collect::<Vec<_>>())
    }

    /// 按 ID 列表获取 todo 的备份数据
    pub async fn get_todo_backups_by_ids(
        &self,
        ids: &[i64],
    ) -> Result<Vec<TodoBackup>, sea_orm::DbErr> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let models = todos::Entity::find()
            .filter(todos::Column::Id.is_in(ids.to_vec()))
            .filter(todos::Column::DeletedAt.is_null())
            .all(&self.conn)
            .await?;

        let model_ids: Vec<i64> = models.iter().map(|m| m.id).collect();
        let tag_map = self.fetch_tag_ids_for_many(&model_ids).await?;

        let all_tags: std::collections::HashMap<i64, String> = tags::Entity::find()
            .all(&self.conn)
            .await?
            .into_iter()
            .map(|t| (t.id, t.name))
            .collect();

        Ok(models
            .into_iter()
            .map(|m| {
                let tag_ids = tag_map.get(&m.id).cloned().unwrap_or_default();
                let tag_names: Vec<String> = tag_ids
                    .iter()
                    .filter_map(|tid| all_tags.get(tid).cloned())
                    .collect();
                TodoBackup {
                    title: m.title,
                    prompt: m.prompt.unwrap_or_default(),
                    status: m
                        .status
                        .as_deref()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(TodoStatus::Pending),
                    executor: m.executor,
                    scheduler_enabled: m.scheduler_enabled.unwrap_or(false),
                    scheduler_config: m.scheduler_config,
                    tag_names,
                    workspace: m.workspace.clone(),
                    worktree: if m.worktree_enabled.unwrap_or(false) {
                        m.workspace.clone()
                    } else {
                        None
                    },
                }
            })
            .collect())
    }

    /// 按 tag name 列表查询 tag 备份数据
    pub async fn get_tag_backups_by_names(
        &self,
        names: &[&str],
    ) -> Result<Vec<crate::models::TagBackup>, sea_orm::DbErr> {
        if names.is_empty() {
            return Ok(Vec::new());
        }
        Ok(tags::Entity::find()
            .filter(
                tags::Column::Name.is_in(names.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
            )
            .all(&self.conn)
            .await?
            .into_iter()
            .map(|t| crate::models::TagBackup {
                name: t.name,
                color: t.color.unwrap_or_default(),
            })
            .collect())
    }

    /// 从备份数据导入 todo（清空现有数据后导入，失败时自动回滚）
    pub async fn import_backup(
        &self,
        tags_in: &[crate::models::TagBackup],
        todos_in: &[TodoBackup],
    ) -> Result<(), sea_orm::DbErr> {
        use sea_orm::QueryFilter;
        use sea_orm::TransactionTrait;

        let txn = self.conn.begin().await?;

        // 清空现有数据
        todo_tags::Entity::delete_many().exec(&txn).await?;
        todos::Entity::delete_many().exec(&txn).await?;
        tags::Entity::delete_many().exec(&txn).await?;

        // 导入标签
        for tag in tags_in {
            let am = crate::db::entity::tags::ActiveModel {
                name: ActiveValue::Set(tag.name.clone()),
                color: ActiveValue::Set(Some(tag.color.clone())),
                ..Default::default()
            };
            am.insert(&txn).await?;
        }

        // 导入 todo
        for todo in todos_in {
            let now = crate::models::utc_timestamp();
            let workspace = todo.worktree.clone().or(todo.workspace.clone());
            let worktree_enabled = todo.worktree.is_some();
            let am = todos::ActiveModel {
                title: ActiveValue::Set(todo.title.clone()),
                prompt: ActiveValue::Set(Some(todo.prompt.clone())),
                status: ActiveValue::Set(Some(todo.status.to_string())),
                executor: ActiveValue::Set(todo.executor.clone()),
                scheduler_enabled: ActiveValue::Set(Some(todo.scheduler_enabled)),
                scheduler_config: ActiveValue::Set(todo.scheduler_config.clone()),
                workspace: ActiveValue::Set(workspace),
                worktree_enabled: ActiveValue::Set(Some(worktree_enabled)),
                created_at: ActiveValue::Set(Some(now.clone())),
                updated_at: ActiveValue::Set(Some(now)),
                ..Default::default()
            };
            let inserted = am.insert(&txn).await?;

            // 关联标签（通过名称查找 tag id）
            for tag_name in &todo.tag_names {
                let tid = tags::Entity::find()
                    .filter(tags::Column::Name.eq(tag_name))
                    .one(&txn)
                    .await?
                    .map(|t| t.id);
                if let Some(tid) = tid {
                    let rel = todo_tags::ActiveModel {
                        todo_id: ActiveValue::Set(inserted.id),
                        tag_id: ActiveValue::Set(tid),
                    };
                    todo_tags::Entity::insert(rel)
                        .on_conflict(
                            sea_orm::sea_query::OnConflict::columns([
                                todo_tags::Column::TodoId,
                                todo_tags::Column::TagId,
                            ])
                            .do_nothing()
                            .to_owned(),
                        )
                        .exec(&txn)
                        .await?;
                }
            }
        }

        txn.commit().await?;
        Ok(())
    }

    /// 智能合并导入：不删除现有数据，按 title+prompt 匹配进行覆盖或新建
    pub async fn merge_backup(
        &self,
        tags_in: &[crate::models::TagBackup],
        todos_in: &[TodoBackup],
    ) -> Result<(u64, u64), sea_orm::DbErr> {
        use sea_orm::TransactionTrait;

        let txn = self.conn.begin().await?;

        // 确保所有 tag 都存在（不存在则创建），并构建 name -> id 映射
        let mut tag_name_map: std::collections::HashMap<String, i64> = tags::Entity::find()
            .all(&txn)
            .await?
            .into_iter()
            .map(|t| (t.name, t.id))
            .collect();

        for tag in tags_in {
            if !tag_name_map.contains_key(&tag.name) {
                let now = crate::models::utc_timestamp();
                let am = tags::ActiveModel {
                    name: ActiveValue::Set(tag.name.clone()),
                    color: ActiveValue::Set(Some(tag.color.clone())),
                    created_at: ActiveValue::Set(Some(now)),
                    ..Default::default()
                };
                let inserted = am.insert(&txn).await?;
                tag_name_map.insert(tag.name.clone(), inserted.id);
            }
        }

        let mut created: u64 = 0;
        let mut updated: u64 = 0;

        for todo in todos_in {
            // 按 title + prompt 查找匹配
            let existing = todos::Entity::find()
                .filter(todos::Column::Title.eq(&todo.title))
                .filter(todos::Column::Prompt.eq(&todo.prompt))
                .filter(todos::Column::DeletedAt.is_null())
                .one(&txn)
                .await?;

            if let Some(model) = existing {
                // 覆盖：更新字段
                let mut am: todos::ActiveModel = model.into();
                am.status = ActiveValue::Set(Some(todo.status.to_string()));
                am.executor = ActiveValue::Set(todo.executor.clone());
                am.scheduler_enabled = ActiveValue::Set(Some(todo.scheduler_enabled));
                am.scheduler_config = ActiveValue::Set(todo.scheduler_config.clone());
                am.workspace = ActiveValue::Set(todo.worktree.clone().or(todo.workspace.clone()));
                am.worktree_enabled = ActiveValue::Set(Some(todo.worktree.is_some()));
                am.updated_at = ActiveValue::Set(Some(crate::models::utc_timestamp()));
                let saved = am.update(&txn).await?;

                // 重建 tag 关联
                todo_tags::Entity::delete_many()
                    .filter(todo_tags::Column::TodoId.eq(saved.id))
                    .exec(&txn)
                    .await?;
                for tag_name in &todo.tag_names {
                    if let Some(&tid) = tag_name_map.get(tag_name) {
                        let rel = todo_tags::ActiveModel {
                            todo_id: ActiveValue::Set(saved.id),
                            tag_id: ActiveValue::Set(tid),
                        };
                        todo_tags::Entity::insert(rel)
                            .on_conflict(
                                sea_orm::sea_query::OnConflict::columns([
                                    todo_tags::Column::TodoId,
                                    todo_tags::Column::TagId,
                                ])
                                .do_nothing()
                                .to_owned(),
                            )
                            .exec(&txn)
                            .await?;
                    }
                }
                updated += 1;
            } else {
                // 新建
                let now = crate::models::utc_timestamp();
                let workspace = todo.worktree.clone().or(todo.workspace.clone());
                let worktree_enabled = todo.worktree.is_some();
                let am = todos::ActiveModel {
                    title: ActiveValue::Set(todo.title.clone()),
                    prompt: ActiveValue::Set(Some(todo.prompt.clone())),
                    status: ActiveValue::Set(Some(todo.status.to_string())),
                    executor: ActiveValue::Set(todo.executor.clone()),
                    scheduler_enabled: ActiveValue::Set(Some(todo.scheduler_enabled)),
                    scheduler_config: ActiveValue::Set(todo.scheduler_config.clone()),
                    workspace: ActiveValue::Set(workspace),
                    worktree_enabled: ActiveValue::Set(Some(worktree_enabled)),
                    created_at: ActiveValue::Set(Some(now.clone())),
                    updated_at: ActiveValue::Set(Some(now)),
                    ..Default::default()
                };
                let inserted = am.insert(&txn).await?;

                for tag_name in &todo.tag_names {
                    if let Some(&tid) = tag_name_map.get(tag_name) {
                        let rel = todo_tags::ActiveModel {
                            todo_id: ActiveValue::Set(inserted.id),
                            tag_id: ActiveValue::Set(tid),
                        };
                        todo_tags::Entity::insert(rel)
                            .on_conflict(
                                sea_orm::sea_query::OnConflict::columns([
                                    todo_tags::Column::TodoId,
                                    todo_tags::Column::TagId,
                                ])
                                .do_nothing()
                                .to_owned(),
                            )
                            .exec(&txn)
                            .await?;
                    }
                }
                created += 1;
            }
        }

        txn.commit().await?;
        Ok((created, updated))
    }

    pub async fn get_recent_completed_todos(
        &self,
        hours: u32,
    ) -> Result<Vec<crate::models::RecentCompletedTodo>, sea_orm::DbErr> {
        let backend = self.conn.get_database_backend();
        let time_filter = format!("datetime('now', '-{} hours')", hours);

        let sql = format!(
            "SELECT t.id as todo_id, t.title, t.prompt, t.executor, \
             er.status as execution_status, er.finished_at, er.result, er.model, er.usage, \
             er.trigger_type, er.id as record_id, er.rating \
             FROM todos t \
             JOIN execution_records er ON er.id = ( \
                 SELECT er2.id FROM execution_records er2 \
                 WHERE er2.todo_id = t.id \
                 ORDER BY er2.finished_at DESC LIMIT 1 \
             ) \
             WHERE t.deleted_at IS NULL \
               AND t.status IN ('completed', 'failed') \
               AND er.finished_at >= {} \
             ORDER BY er.finished_at DESC",
            time_filter
        );

        let rows = self
            .conn
            .query_all(Statement::from_string(backend, sql))
            .await?;

        let todo_ids: Vec<i64> = rows
            .iter()
            .filter_map(|r| r.try_get_by("todo_id").ok())
            .collect();
        let tag_map = self.fetch_tag_ids_for_many(&todo_ids).await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let todo_id: i64 = row.try_get_by("todo_id").ok()?;
                let title: String = row.try_get_by("title").ok()?;
                let executor: Option<String> = row.try_get_by("executor").ok().flatten();
                let completed_at: String =
                    row.try_get_by("finished_at").ok().flatten().unwrap_or_default();
                let result: Option<String> = row.try_get_by("result").ok().flatten();
                let model: Option<String> = row.try_get_by("model").ok().flatten();
                let usage: Option<String> = row.try_get_by("usage").ok().flatten();
                let trigger_type: String =
                    row.try_get_by("trigger_type").ok().flatten().unwrap_or_default();
                let execution_status: String =
                    row.try_get_by("execution_status").ok().flatten().unwrap_or_default();
                let prompt: Option<String> = row.try_get_by("prompt").ok().flatten();
                let record_id: i64 = row.try_get_by("record_id").ok()?;

                let usage: Option<crate::models::ExecutionUsage> =
                    usage.and_then(|u| serde_json::from_str(&u).ok());

                Some(crate::models::RecentCompletedTodo {
                    todo_id,
                    title,
                    prompt,
                    executor,
                    tag_ids: tag_map.get(&todo_id).cloned().unwrap_or_default(),
                    completed_at,
                    result,
                    model,
                    usage,
                    execution_status,
                    trigger_type,
                    record_id,
                    rating: row.try_get_by("rating").ok().flatten(),
                })
            })
            .collect())
    }

    // ====== 环节（kind=step）相关 CRUD ======
    //
    // 设计与 v3 migration 对齐：todos.kind 列区分事项与环节。
    // 这里只读 kind='step' 的子集，loop_steps 强校验只能引用环节。

    /// 按 kind 列过滤列出 todo。供 TodoList 前端 filter 用（事项 / 环节 / 全部）。
    pub async fn list_todos_by_kind(&self, kind: &str) -> Result<Vec<Todo>, sea_orm::DbErr> {
        let models = todos::Entity::find()
            .filter(todos::Column::DeletedAt.is_null())
            .filter(todos::Column::Kind.eq(kind))
            .order_by_desc(todos::Column::UpdatedAt)
            .all(&self.conn)
            .await?;
        let ids: Vec<i64> = models.iter().map(|m| m.id).collect();
        let tag_map = self.fetch_tag_ids_for_many(&ids).await?;
        Ok(models
            .into_iter()
            .map(|m| {
                let tag_ids = tag_map.get(&m.id).cloned().unwrap_or_default();
                Self::model_to_todo(m, tag_ids)
            })
            .collect())
    }

    /// 列出所有环节（kind='step' 且未删除），按更新时间倒序。
    pub async fn list_steps(&self) -> Result<Vec<Todo>, sea_orm::DbErr> {
        let models = todos::Entity::find()
            .filter(todos::Column::DeletedAt.is_null())
            .filter(todos::Column::Kind.eq("step"))
            .order_by_desc(todos::Column::UpdatedAt)
            .all(&self.conn)
            .await?;
        let ids: Vec<i64> = models.iter().map(|m| m.id).collect();
        let tag_map = self.fetch_tag_ids_for_many(&ids).await?;
        Ok(models
            .into_iter()
            .map(|m| {
                let tag_ids = tag_map.get(&m.id).cloned().unwrap_or_default();
                Self::model_to_todo(m, tag_ids)
            })
            .collect())
    }

    /// 列出可被 loop step 选择的环节候选（kind='step' 且未删除），
    /// 字段精简（id/title/executor/prompt），供 loop 编辑器下拉框使用。
    pub async fn list_step_candidates(&self) -> Result<Vec<Todo>, sea_orm::DbErr> {
        // 与 list_steps 同样的过滤条件,字段也由 Todo DTO 决定,
        // 前端拿到后只展示需要的列即可。
        self.list_steps().await
    }

    /// 把事项提升为环节。仅当目标 todo 当前不是 step 时生效（幂等）。
    /// 返回是否真的发生了状态变更。
    ///
    /// 同步在 steps 表创建对应行（id 复用 todo.id），保证 loop_steps.step_id → steps.id
    /// 与原 todo.id 的引用关系不破（V9 迁移的回填 INSERT 也用同样的 id 对齐策略）。
    /// 已存在则跳过,避免重复行。
    pub async fn promote_to_step(&self, id: i64) -> Result<bool, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let updated_now = now.clone();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            kind: ActiveValue::Set(Some("step".to_string())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        let res = am.update(&self.conn).await?;
        if res.kind.as_deref() != Some("step") {
            return Ok(false);
        }
        // 幂等插入 steps 行：用 todo.id 作为 steps.id，复用 loop_steps.step_id 关联链
        steps::Entity::insert(steps::ActiveModel {
            id: ActiveValue::Set(id),
            title: ActiveValue::Set(res.title.clone()),
            prompt: ActiveValue::Set(res.prompt.clone().unwrap_or_default()),
            executor: ActiveValue::Set(res.executor.clone()),
            acceptance_criteria: ActiveValue::Set(res.acceptance_criteria.clone()),
            source_todo_id: ActiveValue::Set(Some(id)),
            color: ActiveValue::Set("#722ed1".to_string()),
            created_at: ActiveValue::Set(res.created_at.clone()),
            updated_at: ActiveValue::Set(Some(updated_now)),
            ..Default::default()
        })
        .on_conflict(
            sea_orm::sea_query::OnConflict::column(steps::Column::Id)
                .do_nothing()
                .to_owned(),
        )
        .exec_without_returning(&self.conn)
        .await
        .ok();
        Ok(true)
    }

    /// 把环节降级为事项。
    /// 若该 todo 正被 loop_steps 引用，禁止降级（返回错误，避免破坏环路引用一致性）。
    pub async fn demote_to_item(&self, id: i64) -> Result<(), String> {
        // 校验是否被 loop_steps 引用
        let in_use = crate::db::entity::loop_steps::Entity::find()
            .filter(crate::db::entity::loop_steps::Column::StepId.eq(id))
            .one(&self.conn)
            .await
            .map_err(|e| e.to_string())?;
        if in_use.is_some() {
            return Err(format!(
                "todo #{} is referenced by loop_steps, cannot demote to item",
                id
            ));
        }
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            kind: ActiveValue::Set(Some("item".to_string())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        am.update(&self.conn).await.map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 计算一个 todo 被多少个 loop step 引用（环节页展示"被哪些 loop 使用"）。
    pub async fn count_loop_steps_using_todo(&self, todo_id: i64) -> Result<i64, sea_orm::DbErr> {
        use sea_orm::PaginatorTrait;
        crate::db::entity::loop_steps::Entity::find()
            .filter(crate::db::entity::loop_steps::Column::StepId.eq(todo_id))
            .count(&self.conn)
            .await
            .map(|c| c as i64)
    }

    /// 批量计算一组 todo 被多少个 loop step 引用，返回 todo_id -> count 的 map。
    /// 用于环节列表页一次性把所有环节的复用度算出来，避免 N+1。
    pub async fn count_loop_steps_for_todos(
        &self,
        todo_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, i64>, sea_orm::DbErr> {
        use sea_orm::{ConnectionTrait, Statement};
        if todo_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        // 用 raw SQL 一次 GROUP BY 出来,避免 N 次 SELECT。
        let ids_csv = todo_ids
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT step_id, COUNT(*) AS cnt FROM loop_steps WHERE step_id IN ({}) GROUP BY step_id",
            ids_csv
        );
        let result = self
            .conn
            .query_all(Statement::from_string(sea_orm::DbBackend::Sqlite, sql))
            .await?;
        let mut map = std::collections::HashMap::new();
        for row in result {
            let step_id: i64 = row.try_get_by("step_id").unwrap_or(0);
            let cnt: i64 = row.try_get_by("cnt").unwrap_or(0);
            if step_id > 0 {
                map.insert(step_id, cnt);
            }
        }
        Ok(map)
    }

    /// 列出所有环节 + 各自的 loop step 引用计数,组装成 StepSummary。
    pub async fn list_steps_with_usage(
        &self,
    ) -> Result<Vec<crate::models::StepSummary>, sea_orm::DbErr> {
        let steps = self.list_steps().await?;
        let ids: Vec<i64> = steps.iter().map(|t| t.id).collect();
        let usage = self.count_loop_steps_for_todos(&ids).await?;
        Ok(steps
            .into_iter()
            .map(|todo| crate::models::StepSummary {
                used_by_loop_step_count: usage.get(&todo.id).copied().unwrap_or(0),
                todo,
            })
            .collect())
    }
}

#[cfg(test)]
mod review_instance_reuse_tests {
    //! 评审实例 todo 复用逻辑的单元测试。
    //!
    //! 关注三个新方法:`create_review_instance_todo` /
    //! `find_review_instance_by_template` / `reset_review_instance_for_reuse`。
    //! 每次评审运行共享同一条 todo(todo_type=2, review_template_id=N),
    //! 避免 todos 表被「同模板 N 次评审 → N 条 todo」刷屏。

    use super::*;
    use crate::db::Database;

    async fn fresh_db() -> Database {
        Database::new(":memory:").await.expect("memory db must open")
    }

    async fn seed_template(db: &Database, name: &str) -> i64 {
        // review_templates 表有 review_template_id, 直接插一条确保模板存在
        use sea_orm::{ActiveModelTrait, Set};
        let now = crate::models::utc_timestamp();
        let am = crate::db::entity::review_templates::ActiveModel {
            name: Set(name.to_string()),
            description: Set(None),
            prompt: Set(format!("{name} prompt")),
            created_at: Set(Some(now.clone())),
            updated_at: Set(Some(now)),
            ..Default::default()
        };
        let inserted = am.insert(&db.conn).await.expect("insert template");
        inserted.id
    }

    // -------- find_review_instance_by_template --------

    #[tokio::test]
    async fn find_review_instance_by_template_returns_existing() {
        let db = fresh_db().await;
        let template_id = seed_template(&db, "默认评审").await;
        let first_id = db
            .create_review_instance_todo(0, template_id, "默认评审", "p1".into(), None)
            .await
            .expect("create first");
        let second_id = db
            .create_review_instance_todo(0, template_id, "默认评审", "p2".into(), None)
            .await
            .expect("create second");
        // 多条匹配 → 返回最新 (id 大的那条)
        let found = db
            .find_review_instance_by_template(template_id)
            .await
            .expect("find");
        assert!(found.is_some(), "must find a review instance");
        let found = found.unwrap();
        assert_eq!(found.id, second_id, "newest by id wins");
        assert_ne!(first_id, second_id);
        assert_eq!(found.review_template_id, Some(template_id));
        assert_eq!(found.todo_type, Some(2));
    }

    #[tokio::test]
    async fn find_review_instance_by_template_returns_none_when_absent() {
        let db = fresh_db().await;
        let template_id = seed_template(&db, "未使用").await;
        let found = db
            .find_review_instance_by_template(template_id)
            .await
            .expect("find");
        assert!(found.is_none(), "no review instance yet");
    }

    #[tokio::test]
    async fn find_review_instance_by_template_excludes_deleted() {
        let db = fresh_db().await;
        let template_id = seed_template(&db, "X").await;
        let id = db
            .create_review_instance_todo(0, template_id, "X", "p".into(), None)
            .await
            .expect("create");
        // 软删除
        use sea_orm::{ActiveModelTrait, Set};
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: Set(id),
            deleted_at: Set(Some(now)),
            ..Default::default()
        };
        am.update(&db.conn).await.expect("soft delete");
        let found = db
            .find_review_instance_by_template(template_id)
            .await
            .expect("find");
        assert!(found.is_none(), "soft-deleted must be excluded");
    }

    #[tokio::test]
    async fn find_review_instance_by_template_isolates_by_template_id() {
        let db = fresh_db().await;
        let t1 = seed_template(&db, "T1").await;
        let t2 = seed_template(&db, "T2").await;
        db.create_review_instance_todo(0, t1, "T1", "p".into(), None).await.expect("c1");
        db.create_review_instance_todo(0, t2, "T2", "p".into(), None).await.expect("c2");
        let f1 = db.find_review_instance_by_template(t1).await.expect("f1");
        let f2 = db.find_review_instance_by_template(t2).await.expect("f2");
        assert_eq!(f1.unwrap().review_template_id, Some(t1));
        assert_eq!(f2.unwrap().review_template_id, Some(t2));
    }

    // -------- reset_review_instance_for_reuse --------

    #[tokio::test]
    async fn reset_review_instance_for_reuse_updates_prompt_status_executor() {
        let db = fresh_db().await;
        let template_id = seed_template(&db, "R").await;
        let id = db
            .create_review_instance_todo(0, template_id, "R", "old-prompt".into(), Some("claude".to_string()))
            .await
            .expect("create");
        db.reset_review_instance_for_reuse(id, "new-prompt", Some("pi"))
            .await
            .expect("reset");
        let found = db
            .find_review_instance_by_template(template_id)
            .await
            .expect("find")
            .expect("must exist");
        assert_eq!(found.id, id, "id preserved");
        assert_eq!(found.prompt.as_deref(), Some("new-prompt"));
        assert_eq!(found.executor.as_deref(), Some("pi"));
        assert_eq!(found.status.as_deref(), Some("pending"), "reset to pending");
        assert_eq!(found.review_template_id, Some(template_id));
        assert_eq!(found.todo_type, Some(2));
    }

    #[tokio::test]
    async fn reset_review_instance_for_reuse_allows_executor_to_become_none() {
        let db = fresh_db().await;
        let template_id = seed_template(&db, "N").await;
        let id = db
            .create_review_instance_todo(0, template_id, "N", "p".into(), Some("claude".to_string()))
            .await
            .expect("create");
        db.reset_review_instance_for_reuse(id, "p2", None)
            .await
            .expect("reset");
        let found = db.find_review_instance_by_template(template_id).await.expect("find").unwrap();
        assert!(found.executor.is_none(), "executor must clear to None");
    }
}
