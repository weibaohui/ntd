//! Loop Trigger Dispatcher — 044 环路瘦身后仅保留「手动触发」入口。
//!
//! 触发器表（loop_triggers）已整体下线，webhook / feishu / todo_completed /
//! tag_added / cron 等事件触发派发随之移除。当前唯一入口是任务创建调用的
//! `dispatch_manual_with_meta`，直接 spawn loop_runner 启动执行。
//!
//! 「手动触发」语义：不绑定具体 trigger，trigger_id 恒为 None；所有 status=enabled
//! 的 loop 都允许被手动触发一次。

use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::services::loop_runner::LoopRunner;

pub struct LoopTriggerDispatcher {
    runner: Arc<LoopRunner>,
    db: Arc<crate::db::Database>,
}

impl LoopTriggerDispatcher {
    /// 只需 db：dispatcher 仅查 loop 元数据，实际执行交给 runner 自带的 ctx。
    pub fn new(runner: Arc<LoopRunner>, db: Arc<crate::db::Database>) -> Self {
        Self { runner, db }
    }

    /// 手动触发：trigger_id 为 None（044 后无 trigger 表）,所有 loop 都允许。
    pub async fn dispatch_manual(
        &self,
        loop_id: i64,
    ) -> Option<i64> {
        let meta = serde_json::json!({ "source": "manual" });
        self.dispatch_manual_with_meta(loop_id, meta).await
    }

    /// 手动触发（带自定义 meta）：trigger_id 为 None，支持传入 params 等元数据。
    /// 任务创建路径调用本方法启动 loop 执行。
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
