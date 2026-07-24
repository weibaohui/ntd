use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 工艺环节原型：供工艺模板引用的环节模板。
///
/// 与 `todo_templates` 分离，避免用户 prompt 片段与系统工艺环节互相污染。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "process_step_templates")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 唯一标识，供 YAML 中 `step_template` 字段引用。
    pub name: String,
    /// 人类可读标题。
    pub title: String,
    /// 环节 prompt，实例化后写入 todo.prompt。
    #[sea_orm(column_type = "Text")]
    pub prompt: String,
    /// 执行器标识，如 `claudecode`。
    pub executor: Option<String>,
    /// 专家/团队名称，如 `product-manager`。
    pub expert_name: Option<String>,
    /// 绑定的 skill 名称列表（JSON 数组）。
    #[sea_orm(column_type = "Text")]
    pub skill_names: String,
    /// 执行模型，如 `claude-sonnet-5`。
    pub model: Option<String>,
    /// 验收标准文本。
    #[sea_orm(column_type = "Text")]
    pub acceptance_criteria: String,
    /// 所属工作空间 ID；NULL 表示系统内置。
    pub workspace_id: Option<i64>,
    /// 是否系统内置。
    #[sea_orm(default_value = false)]
    pub is_system: bool,
    pub source_path: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
