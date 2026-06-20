use std::sync::{Arc, OnceLock};
use tracing::{warn};

use crate::executor_service::RunTodoExecutionRequest;
use crate::handlers::execution::start_todo_execution;
use crate::hooks::models::*;
use crate::models::{ExecutionRecord, ExecutionStatus, Todo};
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
                    &this,
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
                &this,
                item,
                &todo,
                next_chain,
            )
            .await;
        }
    }

    /// Fire `before_execution` hooks attached to `todo_id` and **wait synchronously**
    /// for all target todos to complete before returning.
    ///
    /// Unlike `fire_for_todo` (which is fire-and-forget), this method blocks the
    /// caller until every pre-hook target finishes. The source execution must
    /// not proceed until all pre-flight steps are done.
    ///
    /// Errors (target missing, execution failure, etc.) are logged but do not
    /// propagate — the caller decides what to do on failure. The `skip_if_missing`
    /// flag on each hook item is respected: if true, a missing target is silently
    /// skipped; if false, we log a warning but still return `Ok` to let the caller
    /// decide.
    pub async fn fire_before_execution(
        self: Arc<Self>,
        todo_id: i64,
        ctx: HookContext,
    ) -> Result<(), String> {
        let this = self.clone();

        let todo = match this.ctx.db.get_todo(todo_id).await {
            Ok(Some(t)) => t,
            Ok(None) => {
                warn!("pre-hook fire skipped: todo #{} not found", todo_id);
                return Err(format!("todo #{} not found", todo_id));
            }
            Err(e) => {
                warn!("pre-hook fire skipped: failed to load todo #{}: {}", todo_id, e);
                return Err(format!("failed to load todo #{}: {}", todo_id, e));
            }
        };

        let hooks: Vec<_> = matching_items(&todo, ctx.trigger);

        // No pre-hooks registered — nothing to wait for.
        if hooks.is_empty() {
            return Ok(());
        }

        // Execute each pre-hook target sequentially, collecting errors.
        // We don't use `futures::future::join_all` because we want the first
        // failure to potentially short-circuit (though we continue all to
        // give every hook a chance to run), and we want deterministic logging order.
        let mut errors: Vec<String> = Vec::new();
        for item in hooks {
            let next_chain = append_to_chain(&ctx.chain, todo_id);
            if next_chain.contains(&item.target_todo_id) {
                warn!(
                    "pre-hook #{} skipped: target todo #{} already in chain {:?}",
                    item.id, item.target_todo_id, next_chain
                );
                continue;
            }

            // `execute_target_todo` spawns a helper thread and waits for the
            // result via a oneshot channel — this blocks until the target
            // execution finishes, which is exactly what we want for a
            // synchronous pre-flight step.
            match execute_target_todo(&this.ctx, &this, item, &todo, next_chain).await {
                Ok(()) => {}
                Err(msg) => {
                    if !item.skip_if_missing {
                        // Non-missing target that failed — log and record.
                        warn!("pre-hook #{} failed: {}", item.id, msg);
                        errors.push(msg);
                    } else {
                        warn!("pre-hook #{} skipped (skip_if_missing): {}", item.id, msg);
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
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
///
/// 使用 `OnceLock::get_or_init` 的闭包版本返回的是值，不会出现初始化失败路径，
/// 但 tokio runtime builder 本身可能因资源限制/OOM 而失败。改用
/// `get_or_init` 配合 `expect` 仍会在底层 panic。这里记录触发路径并返回
/// 占位运行时是个坏选择（破坏 hook 分发），所以保留 `expect` 但加注释
/// 说明：构建失败是进程级致命错误，无法降级处理。
#[allow(clippy::expect_used)]
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
        todo_id: Some(source_id),
            step_id: None,
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

/// Look up the most recent FINISHED (success or failed) execution record for
/// `source_id`. Returns the full record so callers can inspect `rating` for
/// the rating gate. Returns `None` if the source has never run or its most
/// recent record is still in-flight (`running`/`pending`).
///
/// We deliberately look at the most recent finished record rather than the
/// most recent record of any status: while a hook is firing the source
/// could have a fresh `running` row that hides a previously-finished
/// scored row, which would make the gate flicker. The terminal record is
/// the one whose rating is stable and meaningful for chaining decisions.
async fn lookup_latest_finished_record(
    ctx: &ServiceContext,
    source_id: i64,
) -> Option<ExecutionRecord> {
    // status="all" means no status filter (see `get_execution_records`), and
    // ORDER BY started_at DESC LIMIT 1 gives us the most recent record.
    let query = crate::db::execution::ExecutionRecordQuery {
        todo_id: Some(source_id),
            step_id: None,
        limit: 1,
        offset: 0,
        status: Some("all"),
    };
    match ctx.db.get_execution_records(query).await {
        Ok((records, _)) => records
            .into_iter()
            .find(|r| r.status != ExecutionStatus::Running),
        Err(e) => {
            warn!(
                "hook: failed to load source todo #{} latest finished record: {}",
                source_id, e
            );
            None
        }
    }
}

/// Decision produced by `evaluate_rating_gate`. The hook service uses this
/// to decide whether to dispatch the target todo or skip the hook.
#[derive(Debug, Clone, PartialEq, Eq)]
enum GateDecision {
    /// Fire the target todo normally.
    Allow,
    /// Skip the hook because the gate wasn't met. Carries a short reason
    /// suitable for the warn log.
    Deny(&'static str),
}

/// Evaluate the optional rating gate on a hook item against the source
/// todo's most recent finished execution record.
///
/// Rules:
///
/// * `min_rating = None` → always allow (no gate configured). This is the
///   backward-compatible path for any existing hook that hasn't opted in.
/// * `latest_finished = None` (source has never finished a run) → apply
///   `unrated_policy`. Default `Skip` denies, `Pass` allows.
/// * `latest_finished.rating = None` (user hasn't scored the run) → apply
///   `unrated_policy` again. Same semantics as "no record at all" — the
///   gate can't say the run is good enough, but the user policy decides
///   whether that's a fail.
/// * `latest_finished.rating >= min_rating` → allow.
/// * otherwise → deny with a descriptive reason including the actual score.
fn evaluate_rating_gate(
    item: &TodoHookItem,
    latest_finished: Option<&ExecutionRecord>,
) -> GateDecision {
    let Some(min_rating) = item.min_rating else {
        return GateDecision::Allow;
    };
    let Some(record) = latest_finished else {
        return match item.unrated_policy {
            UnratedPolicy::Skip => GateDecision::Deny("no finished record"),
            UnratedPolicy::Pass => GateDecision::Allow,
        };
    };
    match record.rating {
        None => match item.unrated_policy {
            UnratedPolicy::Skip => GateDecision::Deny("rating is null"),
            UnratedPolicy::Pass => GateDecision::Allow,
        },
        Some(r) if r >= min_rating => GateDecision::Allow,
        Some(_) => GateDecision::Deny("rating below threshold"),
    }
}

async fn execute_target_todo(
    ctx: &ServiceContext,
    // 复用调用方传进来的 hook_service 单例：target todo 执行末段还要 fire 它的钩子，
    // 必须继续走同一份实例，否则又会出现多份 HookService (issue #509)。
    hook_service: &Arc<HookService>,
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

    // Rating gate: only enforce when the hook is configured with a
    // `min_rating`. Fetching the latest finished record is one extra cheap
    // query, and we only do it when the gate might be active. The lookup
    // also doubles as the "is the source ready to chain off of" check.
    let source_record = lookup_latest_finished_record(ctx, source.id).await;
    match evaluate_rating_gate(item, source_record.as_ref()) {
        GateDecision::Allow => {}
        GateDecision::Deny(reason) => {
            let rating = source_record
                .as_ref()
                .and_then(|r| r.rating)
                .map(|r| r.to_string())
                .unwrap_or_else(|| "null".to_string());
            let min = item
                .min_rating
                .map(|m| m.to_string())
                .unwrap_or_else(|| "none".to_string());
            warn!(
                "hook #{} skipped by rating gate (min_rating={}, policy={}, source rating={}, reason={})",
                item.id, min, item.unrated_policy, rating, reason
            );
            return Ok(());
        }
    }

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
        // target 执行末段 fire 钩子要继续走同一份 hook_service 单例 (issue #509)。
        hook_service: hook_service.clone(),
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
            loop_step_execution_id: None,
            step_id: None,
            feishu_bot_id: None,
        feishu_receive_id: None,
        workspace: None,
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

#[cfg(test)]
mod tests {
    //! Unit tests for the rating-gate logic. We don't need a database for
    //! these — the gate is a pure function over `(TodoHookItem, Option<&ExecutionRecord>)`.
    use super::*;
    use crate::models::ExecutionStatus;

    fn item(min_rating: Option<i32>, policy: UnratedPolicy) -> TodoHookItem {
        TodoHookItem {
            id: 1,
            trigger: HookTrigger::StateChangedToCompleted,
            target_todo_id: 2,
            skip_if_missing: true,
            enabled: true,
            min_rating,
            unrated_policy: policy,
        }
    }

    fn rec(rating: Option<i32>) -> ExecutionRecord {
        ExecutionRecord {
            id: 99,
            todo_id: 1,
            status: ExecutionStatus::Success,
            command: String::new(),
            stdout: String::new(),
            stderr: String::new(),
            result: None,
            started_at: String::new(),
            finished_at: None,
            usage: None,
            executor: None,
            model: None,
            trigger_type: "manual".to_string(),
            pid: None,
            task_id: None,
            session_id: None,
            todo_progress: None,
            execution_stats: None,
            resume_message: None,
            source_todo_id: None,
            source_todo_title: None,
            source_hook_id: None,
            loop_step_execution_id: None,
            step_id: None,
            rating,
            source_execution_record_id: None,
            last_review_status: None,
            last_reviewed_at: None,
            // issue #643: 测试夹具不模拟 worktree 场景，固定 None
            worktree_path: None,
        }
    }

    #[test]
    fn no_min_rating_always_allows() {
        // Hooks that don't opt into the gate keep their original behaviour
        // regardless of the source record's rating (or lack thereof).
        let i = item(None, UnratedPolicy::Skip);
        assert_eq!(
            evaluate_rating_gate(&i, None),
            GateDecision::Allow,
            "no record at all"
        );
        assert_eq!(
            evaluate_rating_gate(&i, Some(&rec(None))),
            GateDecision::Allow,
            "record with no rating"
        );
        assert_eq!(
            evaluate_rating_gate(&i, Some(&rec(Some(0)))),
            GateDecision::Allow,
            "record with low rating"
        );
        assert_eq!(
            evaluate_rating_gate(&i, Some(&rec(Some(100)))),
            GateDecision::Allow,
            "record with high rating"
        );
    }

    #[test]
    fn min_rating_passes_when_score_meets_threshold() {
        let i = item(Some(80), UnratedPolicy::Skip);
        assert_eq!(evaluate_rating_gate(&i, Some(&rec(Some(80)))), GateDecision::Allow);
        assert_eq!(evaluate_rating_gate(&i, Some(&rec(Some(99)))), GateDecision::Allow);
        assert_eq!(evaluate_rating_gate(&i, Some(&rec(Some(100)))), GateDecision::Allow);
    }

    #[test]
    fn min_rating_denies_when_score_below_threshold() {
        let i = item(Some(80), UnratedPolicy::Skip);
        match evaluate_rating_gate(&i, Some(&rec(Some(79)))) {
            GateDecision::Deny(_) => {}
            other => panic!("expected Deny, got {:?}", other),
        }
        match evaluate_rating_gate(&i, Some(&rec(Some(0)))) {
            GateDecision::Deny(_) => {}
            other => panic!("expected Deny, got {:?}", other),
        }
    }

    #[test]
    fn unrated_skip_denies_null_rating() {
        // Default policy: unrated records don't pass the gate. This is the
        // safe behaviour for users who set a threshold because they care
        // about quality — an unrated result can't be trusted.
        let i = item(Some(60), UnratedPolicy::Skip);
        assert_eq!(
            evaluate_rating_gate(&i, Some(&rec(None))),
            GateDecision::Deny("rating is null")
        );
    }

    #[test]
    fn unrated_pass_allows_null_rating() {
        // Opt-in permissive mode: missing rating is treated as "no
        // objection", so the hook fires.
        let i = item(Some(60), UnratedPolicy::Pass);
        assert_eq!(
            evaluate_rating_gate(&i, Some(&rec(None))),
            GateDecision::Allow
        );
    }

    #[test]
    fn unrated_skip_denies_when_no_record_exists() {
        // Source has never finished a run. Even with `Skip`, the gate
        // should deny because there's no record to evaluate. The
        // distinction between "no record" and "record with null rating"
        // matters in the log message but not in the decision.
        let i = item(Some(60), UnratedPolicy::Skip);
        assert_eq!(
            evaluate_rating_gate(&i, None),
            GateDecision::Deny("no finished record")
        );
    }

    #[test]
    fn unrated_pass_allows_when_no_record_exists() {
        let i = item(Some(60), UnratedPolicy::Pass);
        assert_eq!(
            evaluate_rating_gate(&i, None),
            GateDecision::Allow
        );
    }

    #[test]
    fn unrated_policy_does_not_change_threshold_decision() {
        // Policy is only consulted when rating is missing. With a numeric
        // rating, the threshold itself is the sole deciding factor.
        let i = item(Some(50), UnratedPolicy::Pass);
        assert_eq!(evaluate_rating_gate(&i, Some(&rec(Some(60)))), GateDecision::Allow);
        match evaluate_rating_gate(&i, Some(&rec(Some(40)))) {
            GateDecision::Deny(_) => {}
            other => panic!("expected Deny, got {:?}", other),
        }
    }

    #[test]
    fn unrated_policy_deserializes_from_snake_case() {
        // The on-disk format is snake_case; make sure the parser accepts
        // both variants and rejects unknown values.
        assert_eq!(UnratedPolicy::from_str("skip"), Some(UnratedPolicy::Skip));
        assert_eq!(UnratedPolicy::from_str("pass"), Some(UnratedPolicy::Pass));
        assert_eq!(UnratedPolicy::from_str(""), None);
        assert_eq!(UnratedPolicy::from_str("deny"), None);
    }

    #[test]
    fn unrated_policy_default_is_skip() {
        // Default::default() is the safe option, matching the documented
        // contract on the field.
        assert_eq!(UnratedPolicy::default(), UnratedPolicy::Skip);
    }

    #[test]
    fn parse_tolerates_missing_or_unknown_gate_fields() {
        // Older payloads that don't have the new gate fields must still
        // parse cleanly. Items with out-of-range `min_rating` are kept
        // (with the gate dropped) so a bad write doesn't take down the
        // whole hook list.
        let json = r#"{
          "items": [
            { "id": 1, "trigger": "state_changed_to_completed", "target_todo_id": 2, "enabled": true },
            { "id": 2, "trigger": "state_changed_to_completed", "target_todo_id": 3, "enabled": true, "min_rating": 75, "unrated_policy": "pass" },
            { "id": 3, "trigger": "state_changed_to_completed", "target_todo_id": 4, "enabled": true, "min_rating": 200 },
            { "id": 4, "trigger": "state_changed_to_completed", "target_todo_id": 5, "enabled": true, "min_rating": 50, "unrated_policy": "nonsense" }
          ]
        }"#;
        let parsed = TodoHooks::parse(Some(json));
        assert_eq!(parsed.items.len(), 4);

        // Item 1: legacy payload, no gate.
        assert!(parsed.items[0].min_rating.is_none());
        assert_eq!(parsed.items[0].unrated_policy, UnratedPolicy::Skip);

        // Item 2: full gate configured.
        assert_eq!(parsed.items[1].min_rating, Some(75));
        assert_eq!(parsed.items[1].unrated_policy, UnratedPolicy::Pass);

        // Item 3: out-of-range min_rating gets dropped (so the hook still
        // works, just without the gate), and policy falls back to default.
        assert!(parsed.items[2].min_rating.is_none());
        assert_eq!(parsed.items[2].unrated_policy, UnratedPolicy::Skip);

        // Item 4: unknown policy string falls back to default (Skip).
        assert_eq!(parsed.items[3].min_rating, Some(50));
        assert_eq!(parsed.items[3].unrated_policy, UnratedPolicy::Skip);
    }
}
