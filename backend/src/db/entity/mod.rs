pub mod agent_bots;
pub mod execution_logs;
pub mod execution_records;
pub mod executors;
pub mod feishu_homes;
pub mod feishu_history_chats;
pub mod feishu_messages;
pub mod feishu_push_targets;
pub mod feishu_response_config;
pub mod feishu_group_whitelist;
pub mod feishu_project_bindings;
pub mod loop_executions;
pub mod loop_step_executions;
pub mod loop_steps;
// 公开导出 loop_tags / step_tags 实体模块，供 handler 和 DB 层通过 crate::db::entity:: 路径引用；
// 与现有 tags / todo_tags / loops / steps 等实体的导出策略一致，保持模块层级的统一性。
pub mod loop_tags;
pub mod loop_triggers;
pub mod loops;
pub mod project_directories;
pub mod steps;
// 注：step_tags 与 loop_tags 采用相同的公开导出策略；
// 两个关联表的 SeaORM 实体定义（联合主键、外键）完全对称，修改一处请同步修改另一处。
pub mod step_tags;
pub mod sync_records;
pub mod tags;
pub mod todo_tags;
pub mod todo_templates;
pub mod review_templates;
pub mod todos;
pub mod usage_model_breakdown;
pub mod usage_stats;
pub mod usage_executor_daily;
pub mod webhooks;
pub mod webhook_records;

pub mod prelude {
    pub use super::agent_bots::Entity as AgentBots;
    pub use super::execution_logs::Entity as ExecutionLogs;
    pub use super::execution_records::Entity as ExecutionRecords;
    pub use super::executors::Entity as Executors;
    pub use super::feishu_homes::Entity as FeishuHomes;
    pub use super::feishu_history_chats::Entity as FeishuHistoryChats;
    pub use super::feishu_messages::Entity as FeishuMessages;
    pub use super::feishu_push_targets::Entity as FeishuPushTargets;
    pub use super::feishu_response_config::Entity as FeishuResponseConfig;
    pub use super::feishu_group_whitelist::Entity as FeishuGroupWhitelist;
    pub use super::feishu_project_bindings::Entity as FeishuProjectBindings;
    pub use super::loop_executions::Entity as LoopExecutions;
    pub use super::loop_step_executions::Entity as LoopStepExecutions;
    pub use super::loop_steps::Entity as LoopSteps;
    // 在 prelude 层导出 LoopTags / StepTags，使其他模块（如 handlers / db）可以用
    // crate::db::entity::prelude::LoopTags 简洁引用，与 Tags / TodoTags 的导出方式一致。
    pub use super::loop_tags::Entity as LoopTags;
    pub use super::loop_triggers::Entity as LoopTriggers;
    pub use super::loops::Entity as Loops;
    pub use super::project_directories::Entity as ProjectDirectories;
    pub use super::sync_records::Entity as SyncRecords;
    // step_tags 与 loop_tags 在 ORM 层面完全对称（联合主键 + 外键），
    // prelude 导出 StepTags 以便 handlers 和 DB 层统一使用 prelude 路径引用。
    pub use super::step_tags::Entity as StepTags;
    pub use super::steps::Entity as Steps;
    pub use super::tags::Entity as Tags;
    pub use super::todo_tags::Entity as TodoTags;
    pub use super::todo_templates::Entity as TodoTemplates;
    pub use super::review_templates::Entity as ReviewTemplates;
    pub use super::todos::Entity as Todos;
    pub use super::usage_model_breakdown::Entity as UsageModelBreakdowns;
    pub use super::usage_stats::Entity as UsageStats;
    pub use super::usage_executor_daily::Entity as UsageExecutorDaily;
    pub use super::webhooks::Entity as Webhooks;
    pub use super::webhook_records::Entity as WebhookRecords;
}
