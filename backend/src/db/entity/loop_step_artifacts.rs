use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 环节产物快照：记录一次 step execution 产出的文件/文本/URL/JSON。
///
/// 文件类产物只保存路径快照，真实文件留在工作目录。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "loop_step_artifacts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属 loop_step_execution ID。
    pub loop_step_execution_id: i64,
    /// 产物名称，如 `PRD`、`delivery-state`。
    pub name: String,
    /// 产物类型：`file` | `text` | `url` | `json`。
    pub artifact_type: String,
    /// 定位符：路径、标记、正则或 URL。
    pub locator: String,
    /// 文本类产物的内容快照。
    #[sea_orm(column_type = "Text")]
    pub content_text: Option<String>,
    /// 捕获时间（UTC ISO8601）。
    pub captured_at: String,
    /// 捕获来源：execution_record_id 或 `manual`。
    pub captured_by: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::loop_step_executions::Entity",
        from = "Column::LoopStepExecutionId",
        to = "super::loop_step_executions::Column::Id"
    )]
    BelongsToLoopStepExecution,
}

impl Related<super::loop_step_executions::Entity> for Entity {
    fn to() -> RelationDef { Relation::BelongsToLoopStepExecution.def() }
}

impl ActiveModelBehavior for ActiveModel {}
