use std::sync::{Arc, OnceLock};
use tracing::{warn};

use crate::executor_service::RunTodoExecutionRequest;
use crate::handlers::execution::start_todo_execution;
use crate::hooks::models::*;
use crate::models::Todo;
use crate::service_context::ServiceContext;

/// Hook dispatch engine.
///
/// Hooks live on the parent todo (a JSON array stored in `todos.hooks`). When
/// the parent emits a `trigger`, the service reads its own hook list, filters
/// to matches, and starts the target todos one by one.
///
/// Cycle protection: every `HookContext` carries a `chain` of todo ids already
/// visited on the current dispatch path. A hook whose target appears in the
/// chain is rejected, which blocks A → B → A and any longer cycle.
pub struct HookService {
    ctx: ServiceContext,
}

impl HookService {
    pub fn new(ctx: ServiceContext) -> Self {
        Self { ctx }
    }

    /// Fire the hooks attached to `todo_id` whose trigger matches `ctx.trigger`.
    /// Fire-and-forget — failures are logged but never bubble up to the caller,
    /// because the lifecycle event has already happened by the time we run.
    pub fn fire_for_todo(self: Arc<Self>, todo_id: i64, ctx: HookContext) {
        let this = self.clone();

        tokio::spawn(async move {
            let todo = match this.ctx.db.get_todo(todo_id).await {
                Ok(Some(t)) => t,
                Ok(None) => {
                    warn!("hook fire skipped: todo #{} not found", todo_id);
                    return;
                }
                Err(e) => {
                    warn!(
                        "hook fire skipped: failed to load todo #{}: {}",
                        todo_id, e
                    );
                    return;
                }
            };

            let hooks = matching_items(&todo, ctx.trigger);
            for item in hooks {
                let next_chain = append_to_chain(&ctx.chain, todo_id);
                if next_chain.contains(&item.target_todo_id) {
                    warn!(
                        "hook #{} skipped: target todo #{} already in chain {:?}",
                        item.id, item.target_todo_id, next_chain
                    );
                    continue;
                }

                let _ = execute_target_todo(
                    &this.ctx,
                    item,
                    &todo,
                    next_chain,
                )
                .await;
            }
        });
    }

    /// Fire a single source todo's hooks with the given context. The chain is
    /// taken from `ctx.chain` and extended with `source_todo_id`.
    pub async fn fire_for_source(
        self: Arc<Self>,
        source_todo_id: i64,
        ctx: HookContext,
    ) {
        let this = self.clone();

        let todo = match this.ctx.db.get_todo(source_todo_id).await {
            Ok(Some(t)) => t,
            Ok(None) => {
                warn!("hook fire skipped: source todo #{} not found", source_todo_id);
                return;
            }
            Err(e) => {
                warn!(
                    "hook fire skipped: failed to load source todo #{}: {}",
                    source_todo_id, e
                );
                return;
            }
        };

        let hooks = matching_items(&todo, ctx.trigger);
        for item in hooks {
            let next_chain = append_to_chain(&ctx.chain, source_todo_id);
            if next_chain.contains(&item.target_todo_id) {
                warn!(
                    "hook #{} skipped: target todo #{} already in chain {:?}",
                    item.id, item.target_todo_id, next_chain
                );
                continue;
            }

            let _ = execute_target_todo(
                &this.ctx,
                item,
                &todo,
                next_chain,
            )
            .await;
        }
    }
}

fn append_to_chain(chain: &[i64], source: i64) -> Vec<i64> {
    let mut next = chain.to_vec();
    if !next.contains(&source) {
        next.push(source);
    }
    next
}

/// Long-lived, multi-threaded tokio runtime used to dispatch hook-triggered
/// executions. See `execute_target_todo` for why a shared runtime is required.
fn hook_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .thread_name("hook-runtime")
            .build()
            .expect("failed to build hook runtime")
    })
}

/// Iterate a todo's enabled hooks that match the given trigger.
fn matching_items(todo: &Todo, trigger: HookTrigger) -> Vec<&TodoHookItem> {
    todo.hooks
        .iter()
        .filter(|item| item.enabled && item.trigger == trigger)
        .collect()
}

/// Look up the most recent successful execution record for `source_id` and
/// return its `result` string. Returns `None` if the source has never run,
/// has no successful runs, or its most recent run produced an empty result.
///
/// This is the data that lands in the target todo's `{{message}}` placeholder
/// when a hook fires — the "what did the previous executor actually produce"
/// half of the chain. A single `LIMIT 1 WHERE status = success` query is
/// enough; we don't need the full history.
async fn lookup_source_result(ctx: &ServiceContext, source_id: i64) -> Option<String> {
    let query = crate::db::execution::ExecutionRecordQuery {
        todo_id: source_id,
        limit: 1,
        offset: 0,
        status: Some("success"),
    };
    match ctx.db.get_execution_records(query).await {
        Ok((records, _)) => records.into_iter().find_map(|r| r.result),
        Err(e) => {
            warn!(
                "hook: failed to load source todo #{} latest result: {}",
                source_id, e
            );
            None
        }
    }
}

async fn execute_target_todo(
    ctx: &ServiceContext,
    item: &TodoHookItem,
    source: &Todo,
    chain: Vec<i64>,
) -> Result<(), String> {
    let target = ctx
        .db
        .get_todo(item.target_todo_id)
        .await
        .map_err(|e| format!("lookup target todo #{}: {}", item.target_todo_id, e))?
        .ok_or_else(|| format!("target todo #{} not found", item.target_todo_id));

    let target = match target {
        Ok(t) => t,
        Err(msg) => {
            if item.skip_if_missing {
                warn!("{}", msg);
                return Ok(());
            }
            return Err(msg);
        }
    };

    // The target todo's own `prompt` is the template (it may contain a
    // `{{message}}` placeholder). We pass two things in `params`:
    // - `{{message}}` ← the source todo's most recent successful execution
    //   `result` (what its executor actually produced). Falls back to the
    //   source's `prompt` when the source has no execution record yet
    //   (e.g., a state-change trigger fires immediately on creation).
    //
    // `run_todo_execution_with_params` does the substitution and the
    // executor sees the final composed message.
    let source_result = lookup_source_result(ctx, source.id).await;
    let message_payload = build_hook_message(source, source_result.as_deref());
    let message = target.prompt.clone();
    let mut params = std::collections::HashMap::new();
    params.insert("message".to_string(), message_payload);

    let trigger_type = format!("hook:{}", item.trigger.as_str());
    let request = RunTodoExecutionRequest {
        db: ctx.db.clone(),
        executor_registry: ctx.executor_registry.clone(),
        tx: ctx.tx.clone(),
        task_manager: ctx.task_manager.clone(),
        config: ctx.config.clone(),
        todo_id: target.id,
        message,
        req_executor: target.executor.clone(),
        trigger_type,
        params: Some(params),
        resume_session_id: None,
        resume_message: None,
        chain,
        source_todo_id: Some(source.id),
        source_todo_title: Some(source.title.clone()),
        source_hook_id: Some(item.id),
        feishu_bot_id: None,
        feishu_receive_id: None,
    };

    // Dispatch on a dedicated hook runtime. A `std::thread::spawn` is needed
    // to break the async type-cycle:
    //   run_todo_execution → (terminal-state hook in spawned task) → fire_for_todo
    //   → execute_target_todo → start_todo_execution → run_todo_execution.
    // `block_on` returns a concrete output type (not a future), so the type
    // cycle is severed at the thread boundary.
    //
    // The runtime itself must outlive the helper thread: `run_todo_execution`
    // creates the execution record synchronously, then `tokio::spawn`s the
    // long-running task that drives the child process and writes the final
    // status. That spawned task is what actually waits for the executor to
    // finish — if the runtime is dropped when the helper thread exits, the
    // spawned task is aborted and the record stays stuck in "running"
    // forever. A shared, multi-threaded runtime kept alive via `OnceLock`
    // lets the spawned task continue after the helper thread is gone.
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    let runtime = hook_runtime();
    std::thread::spawn(move || {
        let result = runtime.block_on(start_todo_execution(request));
        let _ = reply_tx.send(result);
    });

    match reply_rx.await {
        Ok(_) => Ok(()),
        Err(_) => Err(format!(
            "hook trigger thread for todo #{} dropped reply channel",
            target.id
        )),
    }
}
