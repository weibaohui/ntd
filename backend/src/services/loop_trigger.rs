//! Loop Trigger Dispatcher — 把外部事件（webhook / feishu / todo 完成 / cron）
//! 匹配到对应 loop 的 trigger,并 spawn loop_runner 启动执行。
//!
//! 入口：
//! - `dispatch_webhook(webhook_id)` — webhook handler 调用
//! - `dispatch_feishu_message(bot_id, chat_id, msg_type, content)` — feishu listener 调用
//! - `dispatch_feishu_command(bot_id, command)` — feishu slash command 调用
//! - `dispatch_todo_completed(todo_id, record_id)` — todo 执行完成时调用
//! - `dispatch_tag_added(tag_id, todo_id)` — todo 加 tag 时调用
//!
//! 匹配规则：dispatcher 拉出所有 enabled triggers（按 type 过滤），解析 config 后判断
//! 是否命中；命中的 trigger 拿到 loop_id 后调 loop_runner.spawn_run()。
//!
//! 「同事件命中多个 loop」：dispatcher 对每个命中的 loop 都 spawn 一条 run,
//! 多个 loop 之间是独立的（不会有联动）。
//!
//! 「单个 loop 有多个 trigger 命中」：按 trigger.priority DESC 取最大者,避免重复
//! 启动同一 loop 的多次 run（虽然允许多次 run,但用户期望「一个事件 = 一次启动」）。

use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::services::loop_runner::LoopRunner;

/// Webhook body 上限 64 KiB。触顶直接拒绝,避免 DoS / DB 膨胀。
const MAX_WEBHOOK_BODY_BYTES: usize = 64 * 1024;

pub struct LoopTriggerDispatcher {
    runner: Arc<LoopRunner>,
    ctx: crate::service_context::ServiceContext,
}

impl LoopTriggerDispatcher {
    pub fn new(runner: Arc<LoopRunner>, ctx: crate::service_context::ServiceContext) -> Self {
        Self { runner, ctx }
    }

    /// Webhook 触发：从 webhook 入口的 (webhook_id, body, query) 中匹配 loop trigger。
    /// 配置示例：`{"webhook_id": 5}`
    ///
    /// 安全约束:
    /// - body 上限 64 KiB,超限直接拒绝（避免 DoS / loop_executions.trigger_meta 膨胀）
    /// - HMAC 签名校验尚未实现(见 issue 后续);当前按 webhook_id 路由足够用于内网。
    ///   公开部署时应在 handler 层加 X-Signature 校验后再调用本方法。
    pub async fn dispatch_webhook(
        &self,
        webhook_id: i64,
        body: Option<&str>,
    ) -> Vec<i64> {
        // 上限 64 KiB,触顶直接拒绝 — 不写库、不 spawn
        if let Some(b) = body {
            if b.len() > MAX_WEBHOOK_BODY_BYTES {
                warn!(
                    "loop_trigger: webhook body too large ({} bytes, limit {}), reject",
                    b.len(),
                    MAX_WEBHOOK_BODY_BYTES
                );
                return vec![];
            }
        }
        let triggers = match self
            .ctx
            .db
            .list_enabled_triggers_by_type("webhook")
            .await
        {
            Ok(t) => t,
            Err(e) => {
                warn!("loop_trigger: failed to list webhook triggers: {}", e);
                return vec![];
            }
        };
        let mut started: Vec<i64> = vec![];
        for t in triggers {
            let cfg: serde_json::Value =
                serde_json::from_str(&t.config).unwrap_or_default();
            if cfg
                .get("webhook_id")
                .and_then(|v| v.as_i64())
                .map(|id| id == webhook_id)
                .unwrap_or(false)
            {
                let meta = serde_json::json!({
                    "webhook_id": webhook_id,
                    "body": body.unwrap_or(""),
                });
                let run_id = self.spawn_run(t.loop_id, Some(t.id), "webhook", meta).await;
                if run_id > 0 {
                    started.push(run_id);
                }
            }
        }
        started
    }

    /// 飞书消息触发：匹配 trigger.config.bot_id + chat_id + 内容。
    /// config 示例：`{"bot_id":1,"chat_id":"oc_xxx","match_type":"contains|regex|exact","pattern":"hello"}`
    pub async fn dispatch_feishu_message(
        &self,
        bot_id: i64,
        chat_id: &str,
        content: &str,
    ) -> Vec<i64> {
        let triggers = match self
            .ctx
            .db
            .list_enabled_triggers_by_type("feishu_message")
            .await
        {
            Ok(t) => t,
            Err(e) => {
                warn!(
                    "loop_trigger: failed to list feishu_message triggers: {}",
                    e
                );
                return vec![];
            }
        };
        let mut started: Vec<i64> = vec![];
        for t in triggers {
            let cfg: serde_json::Value =
                serde_json::from_str(&t.config).unwrap_or_default();
            let cfg_bot_id = cfg.get("bot_id").and_then(|v| v.as_i64());
            let cfg_chat_id = cfg.get("chat_id").and_then(|v| v.as_str()).unwrap_or("");
            if cfg_bot_id != Some(bot_id) {
                continue;
            }
            // chat_id 为空表示"任意群/会话"
            if !cfg_chat_id.is_empty() && cfg_chat_id != chat_id {
                continue;
            }
            let match_type = cfg
                .get("match_type")
                .and_then(|v| v.as_str())
                .unwrap_or("contains");
            let pattern = cfg.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
            if !matches_message(match_type, pattern, content) {
                continue;
            }
            let meta = serde_json::json!({
                "bot_id": bot_id,
                "chat_id": chat_id,
                "content": content,
            });
            let run_id = self
                .spawn_run(t.loop_id, Some(t.id), "feishu_message", meta)
                .await;
            if run_id > 0 {
                started.push(run_id);
            }
        }
        started
    }

    /// 飞书 slash command 触发。
    /// config 示例：`{"bot_id":1,"command":"/run"}`
    pub async fn dispatch_feishu_command(
        &self,
        bot_id: i64,
        command: &str,
    ) -> Vec<i64> {
        let triggers = match self
            .ctx
            .db
            .list_enabled_triggers_by_type("feishu_command")
            .await
        {
            Ok(t) => t,
            Err(e) => {
                warn!(
                    "loop_trigger: failed to list feishu_command triggers: {}",
                    e
                );
                return vec![];
            }
        };
        let mut started: Vec<i64> = vec![];
        for t in triggers {
            let cfg: serde_json::Value =
                serde_json::from_str(&t.config).unwrap_or_default();
            let cfg_bot_id = cfg.get("bot_id").and_then(|v| v.as_i64());
            let cfg_command = cfg.get("command").and_then(|v| v.as_str()).unwrap_or("");
            if cfg_bot_id == Some(bot_id) && cfg_command == command {
                let meta = serde_json::json!({
                    "bot_id": bot_id,
                    "command": command,
                });
                let run_id = self
                    .spawn_run(t.loop_id, Some(t.id), "feishu_command", meta)
                    .await;
                if run_id > 0 {
                    started.push(run_id);
                }
            }
        }
        started
    }

    /// Todo 完成时触发：所有 trigger_type = todo_completed 且 config.todo_id = todo_id 的 loop。
    pub async fn dispatch_todo_completed(
        &self,
        todo_id: i64,
        record_id: Option<i64>,
    ) -> Vec<i64> {
        let triggers = match self
            .ctx
            .db
            .list_triggers_by_todo(todo_id)
            .await
        {
            Ok(t) => t,
            Err(e) => {
                warn!("loop_trigger: failed to list todo triggers: {}", e);
                return vec![];
            }
        };
        let mut started: Vec<i64> = vec![];
        for t in triggers.iter().filter(|t| t.trigger_type == "todo_completed") {
            let meta = serde_json::json!({
                "todo_id": todo_id,
                "execution_record_id": record_id,
            });
            let run_id = self
                .spawn_run(t.loop_id, Some(t.id), "todo_completed", meta)
                .await;
            if run_id > 0 {
                started.push(run_id);
            }
        }
        started
    }

    /// 标签添加时触发：所有 trigger_type = tag_added 且 config.tag_id = tag_id 的 loop。
    pub async fn dispatch_tag_added(
        &self,
        tag_id: i64,
        todo_id: i64,
    ) -> Vec<i64> {
        let triggers = match self
            .ctx
            .db
            .list_enabled_triggers_by_type("tag_added")
            .await
        {
            Ok(t) => t,
            Err(e) => {
                warn!("loop_trigger: failed to list tag_added triggers: {}", e);
                return vec![];
            }
        };
        let mut started: Vec<i64> = vec![];
        for t in triggers {
            let cfg: serde_json::Value =
                serde_json::from_str(&t.config).unwrap_or_default();
            if cfg
                .get("tag_id")
                .and_then(|v| v.as_i64())
                .map(|id| id == tag_id)
                .unwrap_or(false)
            {
                let meta = serde_json::json!({
                    "tag_id": tag_id,
                    "todo_id": todo_id,
                });
                let run_id = self
                    .spawn_run(t.loop_id, Some(t.id), "tag_added", meta)
                    .await;
                if run_id > 0 {
                    started.push(run_id);
                }
            }
        }
        started
    }

    /// 手动触发：trigger_id 为 None（不绑定具体 trigger）,所有 loop 都允许。
    pub async fn dispatch_manual(
        &self,
        loop_id: i64,
    ) -> Option<i64> {
        let loop_ = self.ctx.db.get_loop(loop_id).await.ok().flatten();
        if loop_.is_none() {
            return None;
        }
        if loop_.unwrap().status != "enabled" {
            warn!(
                "loop_trigger: manual dispatch on loop #{} skipped (status != enabled)",
                loop_id
            );
            return None;
        }
        let meta = serde_json::json!({ "source": "manual" });
        let id = self.spawn_run(loop_id, None, "manual", meta).await;
        if id > 0 { Some(id) } else { None }
    }

    /// 共用：调 runner.spawn_run。返回 loop_execution_id,失败返回 -1。
    async fn spawn_run(
        &self,
        loop_id: i64,
        trigger_id: Option<i64>,
        trigger_type: &str,
        meta: serde_json::Value,
    ) -> i64 {
        debug!(
            "loop_trigger: spawning loop #{} via {} (trigger_id={:?})",
            loop_id, trigger_type, trigger_id
        );
        let id = self
            .runner
            .clone()
            .spawn_run(loop_id, trigger_id, trigger_type, meta)
            .await;
        info!(
            "loop_trigger: started loop #{} execution #{} via {}",
            loop_id, id, trigger_type
        );
        id
    }
}

/// 内容匹配规则：contains/regex/exact/empty。
fn matches_message(match_type: &str, pattern: &str, content: &str) -> bool {
    if pattern.is_empty() {
        return true; // 无 pattern = 全部命中
    }
    match match_type {
        "contains" => content.contains(pattern),
        "exact" => content == pattern,
        "regex" => match regex_lite_match(pattern, content) {
            Ok(b) => b,
            Err(e) => {
                warn!(
                    "loop_trigger: invalid regex '{}' (fall back to contains): {}",
                    pattern, e
                );
                content.contains(pattern)
            }
        },
        _ => content.contains(pattern),
    }
}

/// 真实 regex 匹配:用 `regex` crate 全功能语法。
///
/// 之前用 contains 假实现,导致用户配 `match_type: "regex"` + `pattern: "^/run \\d+$"`
/// 时 `/runabc` 也会命中,违反配置语义。`regex` 已经在 Cargo.toml 里,直接用。
fn regex_lite_match(pattern: &str, content: &str) -> Result<bool, String> {
    let re = regex::Regex::new(pattern).map_err(|e| format!("invalid regex: {}", e))?;
    Ok(re.is_match(content))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_message_empty_pattern_always_true() {
        assert!(matches_message("contains", "", "anything"));
        assert!(matches_message("regex", "", "anything"));
    }

    #[test]
    fn matches_message_contains() {
        assert!(matches_message("contains", "hello", "hello world"));
        assert!(!matches_message("contains", "hello", "bye world"));
    }

    #[test]
    fn matches_message_exact() {
        assert!(matches_message("exact", "stop", "stop"));
        assert!(!matches_message("exact", "stop", "stop!"));
    }

    #[test]
    fn matches_message_unknown_falls_back_to_contains() {
        assert!(matches_message("fancy", "abc", "xx-abcyy"));
    }

    /// 验证 regex 不再降级为 contains: `^/run \\d+$` 应只匹配 `/run 123`,
    /// 不应匹配 `/runabc`(之前 contains 假实现会通过)。
    #[test]
    fn matches_message_regex_uses_real_engine() {
        assert!(matches_message("regex", r"^/run \d+$", "/run 123"));
        assert!(!matches_message("regex", r"^/run \d+$", "/runabc"));
        assert!(!matches_message("regex", r"^/run \d+$", "pre /run 123"));
    }

    /// 无效 regex 模式应该 warn 降级,而不是 panic。
    #[test]
    fn matches_message_invalid_regex_falls_back_to_contains() {
        // 未闭合的方括号是无效 regex
        assert!(!matches_message("regex", "[unclosed", "anything"));
    }
}
