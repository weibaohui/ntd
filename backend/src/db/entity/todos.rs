use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "todos")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub title: String,
    pub prompt: Option<String>,
    pub status: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub deleted_at: Option<String>,
    /// 归档时间戳（UTC 字符串）。NULL=未归档，参与事项中心日常分类；
    /// 非 NULL=已归档，进入「已归档」分类，从日常视图隐藏但数据保留。
    /// 与 deleted_at 区别：归档仅隐藏，不清理 scheduler/在跑 task、不解除 Loop 引用。
    pub archived_at: Option<String>,
    pub executor: Option<String>,
    pub scheduler_enabled: Option<bool>,
    pub scheduler_config: Option<String>,
    pub scheduler_timezone: Option<String>,
    pub task_id: Option<String>,
    /// 该 todo 所属的工作空间目录路径（对应 project_directories.path）。
    /// 注意：path 不唯一，筛选与外键只用 workspace_id；path 只用于 cwd/worktree。
    pub workspace_path: Option<String>,
    /// 该 todo 所属的工作空间 ID（关联 project_directories.id）
    pub workspace_id: Option<i64>,
    pub webhook_enabled: Option<bool>,
    /// 验收标准（自动评审时作为评审 prompt 的一部分）。
    pub acceptance_criteria: Option<String>,
    /// 0=普通 todo, 1=评审任务（系统自动维护的专用 todo）,
    /// 2=自动评审任务（由 spawn 出来的、跑在用户视角下可见的评审实例）.
    /// 普通 todo 不需要这个字段; 但为了避免 schema 变更，所有行都有值.
    pub todo_type: Option<i32>,
    /// 当 todo_type=2 (review instance) 时, 关联到被评审的原 todo.
    pub parent_todo_id: Option<i64>,
    /// 当 todo_type=2 (review instance) 时, 关联到生成此评审实例的 review_template。
    /// 0/NULL = 评审模板已删除或来自更老的迁移 (V15 之前)。
    pub review_template_id: Option<i64>,
    /// 是否在执行完成后自动派生一个评审 todo (默认 true, 只对 normal 类型有效).
    pub auto_review_enabled: Option<bool>,
    /// 'item' = 一次性事项, 'step' = 可复用的环节 (loop 编排引用)。
    /// 同一张 todos 表承载两种语义, 由 kind 列区分; 详细见 migrations v3。
    pub kind: Option<String>,
    /// Action 类型标记（如 "title_optimize"、"prompt_optimize"）。
    /// 与 action_key 配合，由 /api/actions/execute 用于查找或自动创建 action 模板 todo。
    pub action_type: Option<String>,
    /// Action 键值，与 action_type 配合唯一标识一个 action 模板 todo。
    /// 由 /api/actions/execute 用于查找或自动创建 action 模板 todo。
    pub action_key: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::execution_records::Entity")]
    ExecutionRecords,
}

impl Related<super::execution_records::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ExecutionRecords.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
