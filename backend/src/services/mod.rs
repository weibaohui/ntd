pub mod auto_review;
pub mod auto_update;
pub mod bundled_sync;
pub mod blackboard;
pub mod blackboard_debouncer;
// 黑板防抖 flush 监听与调度（096-W3-PR1 从 executor_service::completion 整族搬入）
pub mod blackboard_flush;
// 096-W4-6：飞书管家聊天（108 前称默认响应）与 wiki chat 两通路共用的 spawn+流读+超时骨架（DirectExecutorSession）
pub mod executor_session;
pub mod feishu_api_client;
pub mod feishu_card;
pub mod feishu_card_actions;
pub mod feishu_history_fetcher;
pub mod feishu_listener;
pub mod feishu_push;
pub mod feishu_slash_commands;
pub mod loop_runner;
// 044：loop_scheduler（cron 触发器调度）随 loop_triggers 表下线，模块已删除。
pub mod loop_trigger;
pub mod message_debounce;
pub mod process;
pub mod startup_check;
pub mod usage_stats;
pub mod worktree;
