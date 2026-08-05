//! 任务讨论帖实体（需求 060：任务讨论区 / 论坛跟帖）。
//!
//! 一条帖子 = 任务讨论流里的一「楼层」。两类：
//! - `human`：用户写的 Markdown 评论；
//! - `agent`：由 @专家 / @执行器 触发执行后自动回写的智能体帖，
//!   通过 `source_execution_id` 关联 `execution_records`，不重复存执行明细。
//!
//! `parent_post_id` 自引用实现一层楼中楼（应用层限制深度 ≤1）。
//! 外键级联删在 v88 迁移的 DDL 里定义，这里不重复声明 Relation（与 tasks.rs 一致）。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "task_posts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属任务（外键 tasks.id，ON DELETE CASCADE）。
    pub task_id: i64,
    /// 楼中楼被回复楼层；NULL=主楼层。深度 ≤1（仅允许指向主楼层）。
    pub parent_post_id: Option<i64>,
    /// `'human'`（人帖）/ `'agent'`（智能体帖）。
    pub kind: String,
    /// 显示名（人 / 专家 / 执行器）。
    pub author_name: String,
    /// 智能体帖实际执行的执行器名（逻辑引用 executors.name）。
    pub executor: Option<String>,
    /// 智能体帖被 @ 的专家名（人设注入来源）。
    pub expert_name: Option<String>,
    /// Markdown 正文。人帖=用户输入；智能体帖=执行结论。
    pub content: String,
    /// 结构化提及 JSON：`[{type, name, display}]`。触发与徽标渲染均依赖它，不靠解析正文。
    pub mentions: String,
    /// 人帖恒 `'sent'`；智能体帖 `'running'`/`'success'`/`'failed'`。
    pub status: String,
    /// 智能体帖来源 execution_records.id（点进可看完整执行明细）。
    pub source_execution_id: Option<i64>,
    /// 承载本次执行的隐藏载体 todos.id（todo_type=DISCUSSION）。
    pub source_todo_id: Option<i64>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

// Relation 为空：task_posts 的外键级联删在 v88 DDL 里定义（ON DELETE CASCADE），
// 不在 SeaORM Relation 层重复声明（与 tasks.rs 等既有实体一致）。
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

// ActiveModelBehavior 用默认实现：写帖前的业务校验（content 非空、parent 归属等）在
// handler 层完成，这里无需 insert/update hook，保持空。
impl ActiveModelBehavior for ActiveModel {}
