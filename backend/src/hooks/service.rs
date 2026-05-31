use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::process::Command;
use tokio::sync::{Semaphore, OwnedSemaphorePermit};
use tracing::{error, info, warn};

use crate::db::Database;
use crate::hooks::db::HookDb;
use crate::hooks::models::*;
use crate::hooks::template::TemplateRenderer;

pub struct HookService {
    db: Arc<Database>,
    semaphore: Arc<Semaphore>,
    default_timeout_secs: u64,
}

impl HookService {
    pub fn new(db: Arc<Database>, max_concurrency: u64, default_timeout_secs: u64) -> Self {
        Self {
            db,
            semaphore: Arc::new(Semaphore::new(max_concurrency as usize)),
            default_timeout_secs,
        }
    }

    /// Fire before_* hooks synchronously - returns error if any hook fails
    pub async fn fire_before_hooks(
        &self,
        ctx: &HookContext,
        tag_ids: &[i64],
    ) -> Result<(), String> {
        let rules = self.get_matching_rules(ctx.trigger, ctx, tag_ids).await?;

        // Only execute sync (before_*) hooks
        let sync_hooks: Vec<_> = rules.into_iter().filter(|r| r.trigger.is_sync()).collect();

        for rule in sync_hooks {
            if !self.matches_filter(&rule.filter, ctx, tag_ids) {
                continue;
            }

            let permit = self.acquire_permit().await;

            let result = self
                .execute_hook(&rule, ctx)
                .await;

            drop(permit);

            if result.success {
                info!(
                    hook_id = rule.id,
                    hook_name = %rule.name,
                    todo_id = ctx.todo_id,
                    "before hook executed successfully"
                );
            } else {
                let msg = format!(
                    "Hook '{}' failed: exit_code={}, stderr={}",
                    rule.name,
                    result.exit_code.unwrap_or(-1),
                    result.stderr
                );
                error!(hook_id = rule.id, "{}", msg);
                return Err(msg);
            }
        }

        Ok(())
    }

    /// Fire after_* hooks asynchronously - does not block
    pub fn fire_after_hooks(self: Arc<Self>, ctx: HookContext, _tag_ids: Vec<i64>) {
        let db = self.db.clone();
        let semaphore = self.semaphore.clone();
        let default_timeout = self.default_timeout_secs;

        tokio::spawn(async move {
            let trigger = ctx.trigger;
            if !trigger.is_sync() {
                // This is an after_* hook
                if let Ok(rules) = HookDb::get_hooks_by_trigger(&db.conn, trigger).await {
                    for rule in rules {
                        if !matches!(&rule.trigger, t if t.is_sync()) {
                            // This is an after hook, execute asynchronously
                            let permit = semaphore.clone().acquire_owned().await.ok();

                            let ctx_clone = ctx.clone();
                            let db_clone = db.clone();

                            tokio::spawn(async move {
                                let result = execute_hook_internal(&rule, &ctx_clone, default_timeout).await;

                                // Log result
                                let args_json = serde_json::to_string(&ctx_clone).ok();
                                let _ = HookDb::insert_hook_log(
                                    &db_clone.conn,
                                    rule.id,
                                    Some(rule.name.clone()),
                                    ctx_clone.trigger.as_str(),
                                    ctx_clone.todo_id,
                                    args_json.as_deref(),
                                    None,
                                    result.exit_code,
                                    Some(&result.stdout),
                                    Some(&result.stderr),
                                    result.duration_ms,
                                    result.success,
                                    result.error_msg.as_deref(),
                                )
                                .await;

                                drop(permit);
                            });
                        }
                    }
                }
            }
        });
    }

    async fn get_matching_rules(
        &self,
        trigger: HookTrigger,
        ctx: &HookContext,
        _tag_ids: &[i64],
    ) -> Result<Vec<HookRule>, String> {
        // Get global config
        let global_config = HookDb::get_global_config(&self.db.conn)
            .await
            .map_err(|e| e.to_string())?;

        if !global_config.enabled {
            return Ok(vec![]);
        }

        // Get hooks by trigger
        let mut rules = HookDb::get_hooks_by_trigger(&self.db.conn, trigger)
            .await
            .map_err(|e| e.to_string())?;

        // If this todo has custom hooks, use those instead
        if let Some(todo_id) = ctx.todo_id {
            let todo_config = HookDb::get_todo_hook_config(&self.db.conn, todo_id)
                .await
                .map_err(|e| e.to_string())?;

            if let Some(config) = todo_config {
                match config.hook_mode {
                    HookMode::Disabled => return Ok(vec![]),
                    HookMode::Custom => {
                        // Get custom rule IDs for this todo
                        let custom_rule_ids = HookDb::get_todo_hook_rule_ids(&self.db.conn, todo_id)
                            .await
                            .map_err(|e| e.to_string())?;

                        if !custom_rule_ids.is_empty() {
                            // Filter rules to only those in custom_rule_ids
                            rules.retain(|r| r.id.map(|id| custom_rule_ids.contains(&id)).unwrap_or(false));
                        }
                    }
                    HookMode::Inherit => {
                        // Use global defaults + trigger-specific hooks
                        let default_ids = HookDb::get_global_default_hook_ids(&self.db.conn)
                            .await
                            .map_err(|e| e.to_string())?;

                        if !default_ids.is_empty() {
                            rules.retain(|r| r.id.map(|id| default_ids.contains(&id)).unwrap_or(false));
                        }
                    }
                }
            }
        } else {
            // For new todos (no todo_id), use global defaults
            let default_ids = HookDb::get_global_default_hook_ids(&self.db.conn)
                .await
                .map_err(|e| e.to_string())?;

            if !default_ids.is_empty() {
                rules.retain(|r| r.id.map(|id| default_ids.contains(&id)).unwrap_or(false));
            }
        }

        Ok(rules)
    }

    fn matches_filter(&self, filter: &HookFilter, ctx: &HookContext, tag_ids: &[i64]) -> bool {
        filter.matches(
            &ctx.todo_title,
            ctx.new_status.as_deref().unwrap_or(""),
            tag_ids,
            ctx.executor.as_deref(),
        )
    }

    async fn acquire_permit(&self) -> OwnedSemaphorePermit {
        self.semaphore.clone().acquire_owned().await.expect("semaphore closed")
    }

    async fn execute_hook(&self, rule: &HookRule, ctx: &HookContext) -> HookResult {
        execute_hook_internal(rule, ctx, rule.action.timeout_secs).await
    }
}

/// Execute a single hook (non-async version for reuse)
async fn execute_hook_internal(rule: &HookRule, ctx: &HookContext, _timeout_secs: u64) -> HookResult {
    let start = Instant::now();

    // Render template
    let args = TemplateRenderer::render_args(&rule.action.args, ctx);
    let env = TemplateRenderer::render_env(&rule.action.env, ctx);

    // Execute command
    match Command::new(&rule.action.command)
        .args(&args)
        .envs(&env)
        .output()
        .await
    {
        Ok(output) => {
            let duration = start.elapsed().as_millis() as i64;
            let exit_code = output.status.code().unwrap_or(-1);
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            HookResult::success(
                exit_code,
                stdout,
                stderr,
                duration,
            )
        }
        Err(e) => {
            let duration = start.elapsed().as_millis() as i64;
            warn!(
                error = %e,
                command = %rule.action.command,
                "failed to execute hook command"
            );
            HookResult::error(e.to_string(), duration)
        }
    }
}

/// Execute a hook command with timeout
pub async fn execute_with_timeout(
    command: &str,
    args: &[String],
    env: &HashMap<String, String>,
    timeout_secs: u64,
) -> HookResult {
    let start = Instant::now();

    match tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        Command::new(command)
            .args(args)
            .envs(env)
            .output(),
    )
    .await
    {
        Ok(Ok(output)) => {
            let duration = start.elapsed().as_millis() as i64;
            let exit_code = output.status.code().unwrap_or(-1);
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            HookResult::success(exit_code, stdout, stderr, duration)
        }
        Ok(Err(e)) => {
            let duration = start.elapsed().as_millis() as i64;
            HookResult::error(format!("execution error: {}", e), duration)
        }
        Err(_) => {
            let duration = start.elapsed().as_millis() as i64;
            HookResult::error("execution timed out".to_string(), duration)
        }
    }
}
