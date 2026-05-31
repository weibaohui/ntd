use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, DbErr, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
};
use serde_json;

use crate::db::entity::global_default_hooks;
use crate::db::entity::global_hook_config;
use crate::db::entity::hook_logs;
use crate::db::entity::hooks;
use crate::db::entity::todo_hook_rules;
use crate::db::entity::todo_hooks;
use crate::hooks::models::*;

pub struct HookDb;

impl HookDb {
    /// Get all hook rules
    pub async fn get_hooks(conn: &DatabaseConnection) -> Result<Vec<HookRule>, DbErr> {
        let models = hooks::Entity::find()
            .order_by_asc(hooks::Column::Id)
            .all(conn)
            .await?;

        models
            .into_iter()
            .map(|m| Self::model_to_rule(m))
            .collect()
    }

    /// Get enabled hooks by trigger
    pub async fn get_hooks_by_trigger(
        conn: &DatabaseConnection,
        trigger: HookTrigger,
    ) -> Result<Vec<HookRule>, DbErr> {
        let models = hooks::Entity::find()
            .filter(hooks::Column::Enabled.eq(true))
            .filter(hooks::Column::Trigger.eq(trigger.as_str()))
            .order_by_asc(hooks::Column::Id)
            .all(conn)
            .await?;

        models
            .into_iter()
            .map(|m| Self::model_to_rule(m))
            .collect()
    }

    /// Get hook by ID
    pub async fn get_hook_by_id(
        conn: &DatabaseConnection,
        id: i64,
    ) -> Result<Option<HookRule>, DbErr> {
        let model = hooks::Entity::find_by_id(id).one(conn).await?;
        match model {
            Some(m) => Self::model_to_rule(m).map(Some),
            None => Ok(None),
        }
    }

    /// Create a new hook rule
    pub async fn create_hook(
        conn: &DatabaseConnection,
        req: CreateHookRequest,
    ) -> Result<HookRule, DbErr> {
        let trigger = HookTrigger::from_str(&req.trigger)
            .ok_or_else(|| DbErr::Custom("Invalid trigger type".to_string()))?;

        let filter_json = serde_json::to_string(&req.filter).unwrap_or_default();
        let action_json = serde_json::to_string(&req.action).unwrap_or_default();

        let active_model = hooks::ActiveModel {
            name: ActiveValue::Set(req.name),
            description: ActiveValue::Set(req.description),
            enabled: ActiveValue::Set(Some(req.enabled)),
            trigger: ActiveValue::Set(trigger.as_str().to_string()),
            filter: ActiveValue::Set(Some(filter_json)),
            action: ActiveValue::Set(action_json),
            is_async: ActiveValue::Set(Some(req.is_async)),
            ..Default::default()
        };

        let model = active_model.insert(conn).await?;
        Self::model_to_rule(model)
    }

    /// Update a hook rule
    pub async fn update_hook(
        conn: &DatabaseConnection,
        id: i64,
        req: UpdateHookRequest,
    ) -> Result<HookRule, DbErr> {
        let existing = hooks::Entity::find_by_id(id)
            .one(conn)
            .await?
            .ok_or_else(|| DbErr::RecordNotFound("Hook not found".to_string()))?;

        let mut active: hooks::ActiveModel = existing.into();

        if let Some(name) = req.name {
            active.name = ActiveValue::Set(name);
        }
        if let Some(description) = req.description {
            active.description = ActiveValue::Set(Some(description));
        }
        if let Some(enabled) = req.enabled {
            active.enabled = ActiveValue::Set(Some(enabled));
        }
        if let Some(trigger) = req.trigger {
            let trigger = HookTrigger::from_str(&trigger)
                .ok_or_else(|| DbErr::Custom("Invalid trigger type".to_string()))?;
            active.trigger = ActiveValue::Set(trigger.as_str().to_string());
        }
        if let Some(filter) = req.filter {
            active.filter = ActiveValue::Set(Some(serde_json::to_string(&filter).unwrap_or_default()));
        }
        if let Some(action) = req.action {
            active.action = ActiveValue::Set(serde_json::to_string(&action).unwrap_or_default());
        }
        if let Some(is_async) = req.is_async {
            active.is_async = ActiveValue::Set(Some(is_async));
        }

        let model = active.update(conn).await?;
        Self::model_to_rule(model)
    }

    /// Delete a hook rule
    pub async fn delete_hook(conn: &DatabaseConnection, id: i64) -> Result<(), DbErr> {
        let model = hooks::Entity::find_by_id(id)
            .one(conn)
            .await?
            .ok_or_else(|| DbErr::RecordNotFound("Hook not found".to_string()))?;

        let active: hooks::ActiveModel = model.into();
        active.delete(conn).await?;
        Ok(())
    }

    /// Get global hook config
    pub async fn get_global_config(
        conn: &DatabaseConnection,
    ) -> Result<crate::hooks::models::GlobalHookConfig, DbErr> {
        let model = global_hook_config::Entity::find_by_id(1)
            .one(conn)
            .await?;

        match model {
            Some(m) => Ok(crate::hooks::models::GlobalHookConfig {
                enabled: m.enabled.unwrap_or(true),
                default_timeout_secs: m.default_timeout_secs.unwrap_or(30) as u64,
                max_concurrency: m.max_concurrency.unwrap_or(5) as u64,
            }),
            None => Ok(crate::hooks::models::GlobalHookConfig::default()),
        }
    }

    /// Update global hook config
    pub async fn update_global_config(
        conn: &DatabaseConnection,
        req: UpdateGlobalHookConfigRequest,
    ) -> Result<crate::hooks::models::GlobalHookConfig, DbErr> {
        let existing = global_hook_config::Entity::find_by_id(1)
            .one(conn)
            .await?;

        let model = match existing {
            Some(m) => {
                let mut active: global_hook_config::ActiveModel = m.into();
                if let Some(enabled) = req.enabled {
                    active.enabled = ActiveValue::Set(Some(enabled));
                }
                if let Some(timeout) = req.default_timeout_secs {
                    active.default_timeout_secs = ActiveValue::Set(Some(timeout as i64));
                }
                if let Some(concurrency) = req.max_concurrency {
                    active.max_concurrency = ActiveValue::Set(Some(concurrency as i64));
                }
                active.update(conn).await?
            }
            None => {
                let active = global_hook_config::ActiveModel {
                    id: ActiveValue::Set(1),
                    enabled: ActiveValue::Set(Some(req.enabled.unwrap_or(true))),
                    default_timeout_secs: ActiveValue::Set(Some(req.default_timeout_secs.unwrap_or(30) as i64)),
                    max_concurrency: ActiveValue::Set(Some(req.max_concurrency.unwrap_or(5) as i64)),
                    updated_at: ActiveValue::Set(Some(crate::models::utc_timestamp())),
                };
                active.insert(conn).await?
            }
        };

        Ok(crate::hooks::models::GlobalHookConfig {
            enabled: model.enabled.unwrap_or(true),
            default_timeout_secs: model.default_timeout_secs.unwrap_or(30) as u64,
            max_concurrency: model.max_concurrency.unwrap_or(5) as u64,
        })
    }

    /// Get global default hook IDs
    pub async fn get_global_default_hook_ids(
        conn: &DatabaseConnection,
    ) -> Result<Vec<i64>, DbErr> {
        let models = global_default_hooks::Entity::find()
            .order_by_asc(global_default_hooks::Column::Priority)
            .all(conn)
            .await?;

        Ok(models
            .into_iter()
            .filter_map(|m| m.hook_id)
            .collect())
    }

    /// Set global default hooks
    pub async fn set_global_default_hooks(
        conn: &DatabaseConnection,
        hook_ids: Vec<i64>,
    ) -> Result<(), DbErr> {
        // Delete existing
        global_default_hooks::Entity::delete_many()
            .exec(conn)
            .await?;

        // Insert new
        for (priority, hook_id) in hook_ids.into_iter().enumerate() {
            let active = global_default_hooks::ActiveModel {
                hook_id: ActiveValue::Set(Some(hook_id)),
                priority: ActiveValue::Set(Some(priority as i64)),
                ..Default::default()
            };
            active.insert(conn).await?;
        }

        Ok(())
    }

    /// Get per-todo hook config
    pub async fn get_todo_hook_config(
        conn: &DatabaseConnection,
        todo_id: i64,
    ) -> Result<Option<TodoHookConfig>, DbErr> {
        let model = todo_hooks::Entity::find()
            .filter(todo_hooks::Column::TodoId.eq(todo_id))
            .one(conn)
            .await?;

        match model {
            Some(m) => {
                let hook_mode = HookMode::from_str(&m.hook_mode.unwrap_or_default())
                    .unwrap_or(HookMode::Inherit);
                Ok(Some(TodoHookConfig {
                    hook_mode,
                    override_enabled: m.override_enabled.unwrap_or(true),
                }))
            }
            None => Ok(None),
        }
    }

    /// Get hook rule IDs for a todo
    pub async fn get_todo_hook_rule_ids(
        conn: &DatabaseConnection,
        todo_id: i64,
    ) -> Result<Vec<i64>, DbErr> {
        let todo_hook = todo_hooks::Entity::find()
            .filter(todo_hooks::Column::TodoId.eq(todo_id))
            .one(conn)
            .await?;

        match todo_hook {
            Some(th) => {
                let rules = todo_hook_rules::Entity::find()
                    .filter(todo_hook_rules::Column::TodoHookId.eq(th.id))
                    .order_by_asc(todo_hook_rules::Column::Priority)
                    .all(conn)
                    .await?;

                Ok(rules
                    .into_iter()
                    .filter_map(|r| r.hook_id)
                    .collect())
            }
            None => Ok(vec![]),
        }
    }

    /// Update per-todo hook config
    pub async fn update_todo_hook_config(
        conn: &DatabaseConnection,
        todo_id: i64,
        req: UpdateTodoHookRequest,
    ) -> Result<TodoHookConfig, DbErr> {
        let existing = todo_hooks::Entity::find()
            .filter(todo_hooks::Column::TodoId.eq(todo_id))
            .one(conn)
            .await?;

        let hook_mode = req
            .hook_mode
            .as_ref()
            .and_then(|s| HookMode::from_str(s))
            .unwrap_or(HookMode::Inherit);

        let todo_hook_id = match existing {
            Some(th) => {
                let mut active: todo_hooks::ActiveModel = th.into();
                if let Some(ref mode) = req.hook_mode {
                    active.hook_mode = ActiveValue::Set(Some(mode.clone()));
                }
                if let Some(enabled) = req.override_enabled {
                    active.override_enabled = ActiveValue::Set(Some(enabled));
                }
                let updated = active.update(conn).await?;
                updated.id
            }
            None => {
                let active = todo_hooks::ActiveModel {
                    todo_id: ActiveValue::Set(todo_id),
                    hook_mode: ActiveValue::Set(Some(hook_mode.as_str().to_string())),
                    override_enabled: ActiveValue::Set(Some(req.override_enabled.unwrap_or(true))),
                    ..Default::default()
                };
                let inserted = active.insert(conn).await?;
                inserted.id
            }
        };

        // Update rule IDs if provided
        if let Some(rule_ids) = req.rule_ids {
            // Delete existing rules
            todo_hook_rules::Entity::delete_many()
                .filter(todo_hook_rules::Column::TodoHookId.eq(todo_hook_id))
                .exec(conn)
                .await?;

            // Insert new rules
            for (priority, hook_id) in rule_ids.into_iter().enumerate() {
                let active = todo_hook_rules::ActiveModel {
                    todo_hook_id: ActiveValue::Set(todo_hook_id),
                    hook_id: ActiveValue::Set(Some(hook_id)),
                    priority: ActiveValue::Set(Some(priority as i64)),
                    ..Default::default()
                };
                active.insert(conn).await?;
            }
        }

        Ok(TodoHookConfig {
            hook_mode,
            override_enabled: req.override_enabled.unwrap_or(true),
        })
    }

    /// Insert hook execution log
    pub async fn insert_hook_log(
        conn: &DatabaseConnection,
        hook_id: Option<i64>,
        hook_name: Option<String>,
        trigger: &str,
        todo_id: Option<i64>,
        args_sent: Option<&str>,
        env_sent: Option<&str>,
        exit_code: Option<i32>,
        stdout: Option<&str>,
        stderr: Option<&str>,
        duration_ms: i64,
        success: bool,
        error_msg: Option<&str>,
    ) -> Result<i64, DbErr> {
        let active = hook_logs::ActiveModel {
            hook_id: ActiveValue::Set(hook_id),
            hook_name: ActiveValue::Set(hook_name),
            trigger: ActiveValue::Set(trigger.to_string()),
            todo_id: ActiveValue::Set(todo_id),
            args_sent: ActiveValue::Set(args_sent.map(String::from)),
            env_sent: ActiveValue::Set(env_sent.map(String::from)),
            exit_code: ActiveValue::Set(exit_code),
            stdout: ActiveValue::Set(stdout.map(String::from)),
            stderr: ActiveValue::Set(stderr.map(String::from)),
            duration_ms: ActiveValue::Set(Some(duration_ms)),
            success: ActiveValue::Set(Some(success)),
            error_msg: ActiveValue::Set(error_msg.map(String::from)),
            ..Default::default()
        };

        let model = active.insert(conn).await?;
        Ok(model.id)
    }

    /// Get hook logs with pagination
    pub async fn get_hook_logs(
        conn: &DatabaseConnection,
        query: HookLogQuery,
    ) -> Result<(Vec<HookLogEntry>, i64), DbErr> {
        let mut base_filter = hook_logs::Column::Id.is_not_null();

        if let Some(hook_id) = query.hook_id {
            base_filter = base_filter.and(hook_logs::Column::HookId.eq(hook_id));
        }
        if let Some(todo_id) = query.todo_id {
            base_filter = base_filter.and(hook_logs::Column::TodoId.eq(todo_id));
        }
        if let Some(ref status) = query.status {
            let success = status == "success";
            base_filter = base_filter.and(hook_logs::Column::Success.eq(success));
        }

        let total: i64 = hook_logs::Entity::find()
            .filter(base_filter.clone())
            .count(conn)
            .await? as i64;

        let models = hook_logs::Entity::find()
            .filter(base_filter)
            .order_by_desc(hook_logs::Column::Id)
            .offset((query.page * query.limit) as u64)
            .limit(query.limit as u64)
            .all(conn)
            .await?;

        let logs = models
            .into_iter()
            .map(|m| HookLogEntry {
                id: m.id,
                hook_id: m.hook_id,
                hook_name: m.hook_name,
                trigger: m.trigger,
                todo_id: m.todo_id,
                args_sent: m.args_sent,
                env_sent: m.env_sent,
                exit_code: m.exit_code,
                stdout: m.stdout,
                stderr: m.stderr,
                duration_ms: m.duration_ms,
                success: m.success,
                error_msg: m.error_msg,
                created_at: m.created_at.unwrap_or_default(),
            })
            .collect();

        Ok((logs, total))
    }

    /// Delete hook logs
    pub async fn delete_hook_logs(conn: &DatabaseConnection) -> Result<u64, DbErr> {
        let result = hook_logs::Entity::delete_many().exec(conn).await?;
        Ok(result.rows_affected)
    }

    // Helper: convert entity model to HookRule
    fn model_to_rule(m: hooks::Model) -> Result<HookRule, DbErr> {
        let trigger = HookTrigger::from_str(&m.trigger)
            .ok_or_else(|| DbErr::Custom(format!("Invalid trigger: {}", m.trigger)))?;

        let filter: HookFilter = m
            .filter
            .as_ref()
            .and_then(|f| serde_json::from_str(f).ok())
            .unwrap_or_default();

        let action: HookAction = serde_json::from_str(&m.action)
            .map_err(|e| DbErr::Custom(format!("Invalid action JSON: {}", e)))?;

        Ok(HookRule {
            id: Some(m.id),
            name: m.name,
            description: m.description,
            enabled: m.enabled.unwrap_or(true),
            trigger,
            filter,
            action,
            is_async: m.is_async.unwrap_or(true),
        })
    }
}
