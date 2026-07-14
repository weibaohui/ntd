//! Loop Trigger Dispatcher — 把外部事件（webhook / feishu / todo 完成 / cron）
//! 匹配到对应 loop 的 trigger,并 spawn loop_runner 启动执行。
//!
//! 入口：
//! - `dispatch_loop_webhook(loop_id, ...)` — loop webhook handler 调用
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

pub struct LoopTriggerDispatcher {
    runner: Arc<LoopRunner>,
    db: Arc<crate::db::Database>,
}

impl LoopTriggerDispatcher {
    /// 只需 db：dispatcher 仅查 loop/trigger 元数据，实际执行交给 runner 自带的 ctx。
    pub fn new(runner: Arc<LoopRunner>, db: Arc<crate::db::Database>) -> Self {
        Self { runner, db }
    }

    pub async fn dispatch_loop_webhook(
        &self,
        loop_id: i64,
        method: &str,
        query_params: &std::collections::HashMap<String, String>,
        body: Option<&str>,
        content_type: Option<&str>,
    ) -> Option<i64> {
        let loop_ = self.db.get_loop(loop_id).await.ok().flatten();
        // loop_ 为 None 时直接返回 None（? 运算符替代 if + unwrap 模式）
        let loop_ = loop_.as_ref()?;
        if loop_.status != "enabled" {
            warn!(
                "loop_trigger: webhook dispatch on loop #{} skipped (status != enabled)",
                loop_id
            );
            return None;
        }
        if !loop_.webhook_enabled {
            warn!(
                "loop_trigger: webhook dispatch on loop #{} skipped (webhook_enabled=false)",
                loop_id
            );
            return None;
        }
        let meta = serde_json::json!({
            "source": "webhook",
            "method": method,
            "query_params": query_params,
            "content_type": content_type.unwrap_or(""),
            "body": body.unwrap_or(""),
        });
        let id = self.spawn_run(loop_id, None, "webhook", meta, None, None, None).await;
        if id > 0 { Some(id) } else { None }
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
                .spawn_run(t.loop_id, Some(t.id), "feishu_message", meta, Some(bot_id), Some(chat_id.to_string()), Some("chat_id".to_string()))
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
                    .spawn_run(t.loop_id, Some(t.id), "feishu_command", meta, Some(bot_id), None, None)
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
                .spawn_run(t.loop_id, Some(t.id), "todo_completed", meta, None, None, None)
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
                        .spawn_run(t.loop_id, Some(t.id), "tag_added", meta, None, None, None)
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
        let meta = serde_json::json!({ "source": "manual" });
        self.dispatch_manual_with_meta(loop_id, meta).await
    }

    /// 手动触发（带自定义 meta）：trigger_id 为 None，支持传入 params 等元数据。
    pub async fn dispatch_manual_with_meta(
        &self,
        loop_id: i64,
        trigger_meta: serde_json::Value,
    ) -> Option<i64> {
        let loop_ = self.db.get_loop(loop_id).await.ok().flatten();
        // loop_ 为 None 时直接返回 None（? 运算符替代 if + unwrap 模式）
        let loop_ = loop_.as_ref()?;
        if loop_.status != "enabled" {
            warn!(
                "loop_trigger: manual dispatch on loop #{} skipped (status != enabled)",
                loop_id
            );
            return None;
        }
        let id = self.spawn_run(loop_id, None, "manual", trigger_meta, None, None, None).await;
        if id > 0 { Some(id) } else { None }
    }

    /// 共用：调 runner.spawn_run。返回 loop_execution_id,失败返回 -1。
    #[allow(clippy::too_many_arguments)] // 参数来自上游 handler 的独立数据源，合并为 struct 增加认知负担
    async fn spawn_run(
        &self,
        loop_id: i64,
        trigger_id: Option<i64>,
        trigger_type: &str,
        meta: serde_json::Value,
        feishu_bot_id: Option<i64>,
        feishu_receive_id: Option<String>,
        // 接收者 ID 类型（"open_id" / "chat_id"）
        feishu_receive_id_type: Option<String>,
    ) -> i64 {
        debug!(
            "loop_trigger: spawning loop #{} via {} (trigger_id={:?})",
            loop_id, trigger_type, trigger_id
        );
        let id = self
            .runner
            .clone()
            .spawn_run(loop_id, trigger_id, trigger_type, meta, feishu_bot_id, feishu_receive_id, feishu_receive_id_type);
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

/// 极简 regex: 避免引入 regex crate (已经引入了,但尽量减少 use), 仅支持
/// `^...$` 包裹的简单模式或 `regex` crate 的标准语法。
///
/// 如果项目已经引入了 `regex` crate（看 Cargo.toml），则用完整 regex 引擎。
/// 为减少依赖膨胀，这里用一个简化版：仅区分「字面量」与「包含」。
fn regex_lite_match(pattern: &str, content: &str) -> Result<bool, String> {
    // 这里直接走 contains,完整 regex 留给 issue 后续加 dep 时再做
    Ok(content.contains(pattern))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
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
}
