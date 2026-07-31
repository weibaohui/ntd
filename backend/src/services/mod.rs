pub mod auto_review;
pub mod auto_update;
pub mod bundled_sync;
pub mod blackboard;
pub mod blackboard_debouncer;
pub mod feishu_card;
pub mod feishu_history_fetcher;
pub mod feishu_listener;
pub mod feishu_push;
pub mod loop_runner;
// 044：loop_scheduler（cron 触发器调度）随 loop_triggers 表下线，模块已删除。
pub mod loop_trigger;
pub mod message_debounce;
pub mod process;
pub mod startup_check;
pub mod usage_stats;
pub mod worktree;
