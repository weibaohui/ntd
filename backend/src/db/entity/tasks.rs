use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "tasks")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub title: String,
    pub description: String,
    pub status: String,
    pub workspace_id: Option<i64>,
    pub template_id: Option<i64>,
    pub loop_id: Option<i64>,
    pub created_by: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    // —— 需求 092：任务委派执行（工艺环路之外的另一种执行方式）——
    /// 执行方式：`loop`（工艺环路，默认）/ `delegate`（委派给处理人跑）。
    /// DB 列 NOT NULL DEFAULT 'loop'，故用非 Option；旧任务由 v91 迁移回填为 'loop'。
    pub execution_mode: String,
    /// 委派对象类型：`executor` / `expert`，仅 delegate 模式有值；loop 模式恒为 None。
    pub assignee_kind: Option<String>,
    /// 委派处理人名（执行器名或专家名），仅 delegate 模式有值。
    pub assignee_name: Option<String>,
    /// 自动接力开关（0 关 / 1 开）；仅 `assignee_kind='expert'` 时允许为 1。
    /// 用 i64 与 SQLite INTEGER 列对齐，由 handler 层按 0/1 转 bool，避免 bool 映射歧义。
    pub auto_continue: i64,
    /// 自动接力已执行轮数（护栏计数；达「有效上限」强制停止，上限三级可配，见 task_posts.rs）。
    pub continue_rounds: i64,
    /// 本任务接力轮数「上限阈值」覆盖（NULL=回退工作空间默认 → 兜底常量；三级解析
    /// 见 `resolve_delegate_max_rounds`）。注意区别于 continue_rounds：本字段是用户可调的
    /// 上限，后者是后端单调递增的「已跑计数」，前端不可直传（设计 §护栏不可绕过）。
    pub delegate_max_rounds: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
