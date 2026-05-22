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
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            title: ActiveValue::Set(title.to_string()),
            prompt: ActiveValue::Set(Some(prompt.to_string())),
            status: ActiveValue::Set(Some(TodoStatus::Pending.to_string())),
            created_at: ActiveValue::Set(Some(now.clone())),
            updated_at: ActiveValue::Set(Some(now)),
            executor: ActiveValue::Set(Some("claudecode".to_string())),
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
        id: i64,
        enabled: bool,
        config: Option<&str>,
        timezone: Option<&str>,
    ) -> Result<(), sea_orm::DbErr> {
        let now = crate::models::utc_timestamp();
        let am = todos::ActiveModel {
            id: ActiveValue::Unchanged(id),
            scheduler_enabled: ActiveValue::Set(Some(enabled)),
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
             er.trigger_type, er.id as record_id \
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
                })
            })
            .collect())
    }
}
