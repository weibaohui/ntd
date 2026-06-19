use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, Statement,
};

use crate::db::entity::tags;
use crate::db::entity::{todo_tags, todos};
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
            hooks: crate::hooks::TodoHooks::parse(m.hooks.as_deref()).items,
            acceptance_criteria: m.acceptance_criteria,
            todo_type: m.todo_type.unwrap_or(0),
            parent_todo_id: m.parent_todo_id,
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

    /// Replace the inline hook list for a todo. The list is stored as a JSON
    /// array in the `hooks` column.
    pub async fn update_todo_hooks(
        &self,
        id: i64,
        items: &[crate::hooks::TodoHookItem],
    ) -> Result<(), sea_orm::DbErr> {
        let wrapped = crate::hooks::TodoHooks {
            items: items.to_vec(),
        };
        let json = serde_json::to_string(&wrapped).map_err(|e| {
            sea_orm::DbErr::Custom(format!("failed to encode hooks for todo #{}: {}", id, e))
        })?;
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            hooks: ActiveValue::Set(Some(json)),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        self.exec_update(am).await
    }

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

    /// 克隆一份 todo 作为"评审实例"。原 todo (template) 的所有字段都复制过来,
    /// 但 todo_type=2, parent_todo_id=Some(parent_id), title 加前缀,
    /// auto_review_enabled=false (评审实例自身不再评审).
    /// 跳过 hooks / scheduler (评审实例是 transient 的).
    pub async fn clone_todo_for_review(
        &self,
        template_id: i64,
        parent_id: i64,
    ) -> Result<i64, sea_orm::DbErr> {
        let template = self
            .get_todo(template_id)
            .await?
            .ok_or_else(|| sea_orm::DbErr::Custom(format!("template todo #{} not found", template_id)))?;
        let now = crate::models::utc_timestamp();
        let title = format!("[评审] {}", template.title);
        let am = todos::ActiveModel {
            title: ActiveValue::Set(title),
            prompt: ActiveValue::Set(template.prompt.clone().into()),
            status: ActiveValue::Set(Some(TodoStatus::Pending.to_string())),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            executor: ActiveValue::Set(template.executor.clone()),
            todo_type: ActiveValue::Set(Some(2)),
            parent_todo_id: ActiveValue::Set(Some(parent_id)),
            auto_review_enabled: ActiveValue::Set(Some(false)),
            ..Default::default()
        };
        let inserted = am.insert(&self.conn).await?;
        Ok(inserted.id)
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

    // ====== 环节（kind=expert）相关 CRUD ======
    //
    // 设计与 v3 migration 对齐：todos.kind 列区分事项与环节。
    // 这里只读 kind='expert' 的子集，loop_stages 强校验只能引用环节。

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

    /// 列出所有环节（kind='expert' 且未删除），按更新时间倒序。
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

    /// 列出可被 loop stage 选择的环节候选（kind='expert' 且未删除），
    /// 字段精简（id/title/executor/prompt），供 loop 编辑器下拉框使用。
    pub async fn list_step_candidates(&self) -> Result<Vec<Todo>, sea_orm::DbErr> {
        // 与 list_steps 同样的过滤条件,字段也由 Todo DTO 决定,
        // 前端拿到后只展示需要的列即可。
        self.list_steps().await
    }

    /// 把事项提升为环节。仅当目标 todo 当前不是 expert 时生效（幂等）。
    /// 返回是否真的发生了状态变更。
    pub async fn promote_to_step(&self, id: i64) -> Result<bool, sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            kind: ActiveValue::Set(Some("step".to_string())),
            updated_at: ActiveValue::Set(Some(now)),
            ..Default::default()
        };
        let res = am.update(&self.conn).await?;
        Ok(res.kind.as_deref() == Some("step"))
    }

    /// 把环节降级为事项。
    /// 若该 todo 正被 loop_stages 引用，禁止降级（返回错误，避免破坏环路引用一致性）。
    pub async fn demote_to_item(&self, id: i64) -> Result<(), String> {
        // 校验是否被 loop_stages 引用
        let in_use = crate::db::entity::loop_stages::Entity::find()
            .filter(crate::db::entity::loop_stages::Column::TodoId.eq(id))
            .one(&self.conn)
            .await
            .map_err(|e| e.to_string())?;
        if in_use.is_some() {
            return Err(format!(
                "todo #{} is referenced by loop_stages, cannot demote to item",
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

    /// 计算一个 todo 被多少个 loop stage 引用（环节页展示"被哪些 loop 使用"）。
    pub async fn count_loop_stages_using_todo(&self, todo_id: i64) -> Result<i64, sea_orm::DbErr> {
        use sea_orm::PaginatorTrait;
        crate::db::entity::loop_stages::Entity::find()
            .filter(crate::db::entity::loop_stages::Column::TodoId.eq(todo_id))
            .count(&self.conn)
            .await
            .map(|c| c as i64)
    }

    /// 批量计算一组 todo 被多少个 loop stage 引用，返回 todo_id -> count 的 map。
    /// 用于环节列表页一次性把所有环节的复用度算出来，避免 N+1。
    pub async fn count_loop_stages_for_todos(
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
            "SELECT todo_id, COUNT(*) AS cnt FROM loop_stages WHERE todo_id IN ({}) GROUP BY todo_id",
            ids_csv
        );
        let result = self
            .conn
            .query_all(Statement::from_string(sea_orm::DbBackend::Sqlite, sql))
            .await?;
        let mut map = std::collections::HashMap::new();
        for row in result {
            let todo_id: i64 = row.try_get_by("todo_id").unwrap_or(0);
            let cnt: i64 = row.try_get_by("cnt").unwrap_or(0);
            if todo_id > 0 {
                map.insert(todo_id, cnt);
            }
        }
        Ok(map)
    }

    /// 列出所有环节 + 各自的 loop stage 引用计数,组装成 StepSummary。
    pub async fn list_steps_with_usage(
        &self,
    ) -> Result<Vec<crate::models::StepSummary>, sea_orm::DbErr> {
        let steps = self.list_steps().await?;
        let ids: Vec<i64> = steps.iter().map(|t| t.id).collect();
        let usage = self.count_loop_stages_for_todos(&ids).await?;
        Ok(steps
            .into_iter()
            .map(|todo| crate::models::StepSummary {
                used_by_loop_stage_count: usage.get(&todo.id).copied().unwrap_or(0),
                todo,
            })
            .collect())
    }
}
