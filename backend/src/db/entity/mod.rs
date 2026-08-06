pub mod agent_bots;

pub mod blackboards;
pub mod execution_logs;
pub mod execution_records;
pub mod executors;
pub mod feishu_history_chats;
pub mod feishu_messages;
pub mod feishu_push_targets;
pub mod feishu_response_config;
pub mod feishu_group_whitelist;
pub mod feishu_project_bindings;
pub mod loop_executions;
pub mod loop_phase_executions;
pub mod loop_phases;
pub mod loop_step_artifacts;
pub mod loop_step_execution_gates;
pub mod loop_step_executions;
pub mod loop_steps;
pub mod loop_tags;
pub mod tasks;
/// 任务讨论帖实体（需求 060：论坛跟帖 + @专家/@执行器 触发执行后回帖）。
pub mod task_posts;
pub mod loops;
pub mod process_templates;
pub mod project_directories;
// 两个关联表的 SeaORM 实体定义（联合主键、外键）完全对称，修改一处请同步修改另一处。
pub mod sync_records;
pub mod tags;
pub mod todo_tags;
pub mod todo_templates;
pub mod review_templates;
pub mod todos;
pub mod usage_model_breakdown;
pub mod usage_stats;
pub mod usage_executor_daily;
pub mod workspace_settings;
pub mod workspace_slash_commands;
pub mod quick_buttons;

pub mod prelude {
    pub use super::agent_bots::Entity as AgentBots;

    pub use super::blackboards::Entity as Blackboards;
    pub use super::execution_logs::Entity as ExecutionLogs;
    pub use super::execution_records::Entity as ExecutionRecords;
    pub use super::executors::Entity as Executors;
    pub use super::feishu_history_chats::Entity as FeishuHistoryChats;
    pub use super::feishu_messages::Entity as FeishuMessages;
    pub use super::feishu_push_targets::Entity as FeishuPushTargets;
    pub use super::feishu_response_config::Entity as FeishuResponseConfig;
    pub use super::feishu_group_whitelist::Entity as FeishuGroupWhitelist;
    pub use super::feishu_project_bindings::Entity as FeishuProjectBindings;
    pub use super::loop_executions::Entity as LoopExecutions;
    pub use super::loop_phase_executions::Entity as LoopPhaseExecutions;
    pub use super::loop_phases::Entity as LoopPhases;
    pub use super::loop_step_artifacts::Entity as LoopStepArtifacts;
    pub use super::loop_step_execution_gates::Entity as LoopStepExecutionGates;
    pub use super::loop_step_executions::Entity as LoopStepExecutions;
    pub use super::loop_steps::Entity as LoopSteps;
    pub use super::loop_tags::Entity as LoopTags;
    pub use super::loops::Entity as Loops;
    /// 任务讨论帖实体别名（需求 060），供 SeaORM 查询时用 TaskPosts::find() 引用。
    pub use super::task_posts::Entity as TaskPosts;
    pub use super::process_templates::Entity as ProcessTemplates;
    pub use super::project_directories::Entity as ProjectDirectories;
    pub use super::sync_records::Entity as SyncRecords;
    pub use super::tags::Entity as Tags;
    pub use super::todo_tags::Entity as TodoTags;
    pub use super::todo_templates::Entity as TodoTemplates;
    pub use super::review_templates::Entity as ReviewTemplates;
    pub use super::todos::Entity as Todos;
    pub use super::usage_stats::Entity as UsageStats;
    pub use super::workspace_settings::Entity as WorkspaceSettings;
    pub use super::workspace_slash_commands::Entity as WorkspaceSlashCommands;
    pub use super::quick_buttons::Entity as QuickButtons;
}
