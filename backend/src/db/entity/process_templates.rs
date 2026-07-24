use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 工艺模板：可复用的流程蓝图，实例化后生成 Loop。
///
/// 模板本身只读，版本化存储；实例通过 `loops.process_template_id` 回溯来源。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "process_templates")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 唯一标识，如 `4p12s-delivery`、`superpowers-task`。
    pub name: String,
    /// 人类可读名称。
    pub display_name: String,
    /// 工艺描述。
    pub description: String,
    /// 分类，如 `software`、`migration`。
    pub category: String,
    /// 复杂度：`light` | `standard` | `complex`。
    pub complexity: String,
    /// 语义化版本，如 `1.0.0`。
    pub version: String,
    /// 工艺完整定义（YAML/JSON），包含 phases、links、limits 等。
    #[sea_orm(column_type = "Text")]
    pub definition: String,
    /// 来源路径，如 `bundled://processes/software/4p12s-delivery.yaml`。
    pub source_path: Option<String>,
    /// 所属工作空间 ID；NULL 表示系统内置。
    pub workspace_id: Option<i64>,
    /// 是否系统内置模板，用户不可编辑。
    #[sea_orm(default_value = false)]
    pub is_system: bool,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
