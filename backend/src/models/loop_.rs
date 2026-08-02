//! Loop Studio 数据模型（API DTO）。
//!
//! 与 `db::entity::loops` 等实体不同：
//! - 实体是 SeaORM 自动派生的,直接对应数据库行
//! - 这里定义的是面向 API 层的 DTO,经过 snake_case / camelCase 转换、字段精简、
//!   嵌套结构组装,直接给前端消费
//!
//! 044（环路瘦身）：触发器、webhook、评审模板、手工创建/更新/触发/审批评分制等
//! 概念整体下线，本文件只保留只读查询与运行态（启停/标签/门禁审批）所需的 DTO。
use serde::{Deserialize, Serialize};

use crate::db::entity::{
    loop_executions, loop_step_executions, loop_steps, loops, process_templates,
};
use crate::db::loop_::{LoopFullView, LoopListRow};

/// Loop 列表行(左栏一行)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopListItem {
    #[serde(flatten)]
    pub loop_: LoopDto,
    pub step_count: i32,
    pub last_execution_status: String,
    pub last_execution_at: Option<String>,
    /// 该 loop 下所有待人工审批的环节执行数
    #[serde(default)]
    pub pending_approval_count: i32,
}

impl From<LoopListRow> for LoopListItem {
    fn from(row: LoopListRow) -> Self {
        Self {
            loop_: row.loop_.into(),
            step_count: row.step_count,
            last_execution_status: row.last_execution_status,
            last_execution_at: row.last_execution_at,
            pending_approval_count: row.pending_approval_count,
        }
    }
}

impl LoopListItem {
    /// 在 handler 已完成关联表查询后注入标签，避免列表行转换隐式依赖数据库访问。
    /// 不放入 `From<LoopListRow>` 是因为标签信息需要额外跨表查询，
    /// 由 handler 在操作事务边界外统一获取后注入。
    pub fn with_tags(mut self, tag_ids: Vec<i64>) -> Self {
        self.loop_ = self.loop_.with_tags(tag_ids);
        self
    }
}

/// Loop 全局聚合统计(供 dashboard「自动化」Tab 用)。
///
/// 一次聚合所有 loop 的规模/成功率/触发器分布/Token,避免前端逐 loop 拉取造成的 N+1。
/// total_loops/active_loops 来自 loops 配置表(不受时间窗影响);
/// 其余执行类指标来自 loop_executions,按 hours 过滤 started_at。
/// 044：触发器入口已下线，trigger_type_distribution 仅反映历史执行的触发来源。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopStats {
    pub total_loops: i64,
    pub active_loops: i64,
    pub total_executions: i64,
    pub success_executions: i64,
    pub failed_executions: i64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
    /// 触发类型分布(按 loop_executions.trigger_type GROUP BY)。
    pub trigger_type_distribution: Vec<LoopTriggerTypeCount>,
}

/// Loop 触发类型分布项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopTriggerTypeCount {
    pub trigger_type: String,
    pub count: i64,
    pub success_count: i64,
    pub failed_count: i64,
}

/// Loop 详情(基本+子项完整数据),LoopStudio 详情页一次拿到。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopDetail {
    #[serde(flatten)]
    pub loop_: LoopDto,
    pub steps: Vec<LoopStepDto>,
    /// 待人工审批的环节执行数（approval_status='pending' 的 loop_step_executions 数量）
    #[serde(default)]
    pub pending_approval_count: i32,
}

impl From<LoopFullView> for LoopDetail {
    fn from(view: LoopFullView) -> Self {
        let steps = view
            .steps_meta
            .into_iter()
            .map(
                |(s, todo_title, todo_executor, todo_archived_at): (
                    loop_steps::Model,
                    String,
                    String,
                    Option<String>,
                )| LoopStepDto {
                    step: s.into(),
                    todo_title,
                    todo_executor,
                    todo_archived_at,
                },
            )
            .collect();
        Self {
            loop_: view.loop_.into(),
            steps,
            pending_approval_count: view.pending_approval_count,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopDto {
    pub id: i64,
    pub name: String,
    pub description: String,
    /// 所属工作空间 ID（project_directories.id），唯一键。
    /// 路径不再作为 API 字段返回——cwd 在后端内部从 project_directories 解析，
    /// 前端展示用 project_directories.name，避免长路径重复上送。
    pub workspace_id: Option<i64>,
    pub status: String,
    /// 标签 ID 列表（单选，复用 Todo 的标签体系）
    #[serde(default)]
    pub tag_ids: Vec<i64>,
    pub limits_config: String,
    /// 异常处理提示词快照（工艺定义）；NULL=未配置异常处理。需求 035。
    pub abnormal_handler_prompt: Option<String>,
    /// 异常处理触发条件 JSON 数组
    pub abnormal_handler_trigger_on: String,
    /// 来源工艺模板 ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_template_id: Option<i64>,
    /// 实例化时的工艺版本快照（实体列早已存在，DTO 补齐以消除漂移）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_template_version: Option<String>,
    /// 来源工艺模板唯一名（面包屑跳转用）；由 handler 注入，From 不查库。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_template_name: Option<String>,
    /// 来源工艺模板 guid（040：面包屑/回跳按 guid 寻址）；由 handler 注入，From 不查库。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_template_guid: Option<String>,
    /// 来源工艺模板显示名（面包屑展示用）；由 handler 注入，From 不查库。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_template_display_name: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

impl From<loops::Model> for LoopDto {
    fn from(m: loops::Model) -> Self {
        Self {
            id: m.id,
            name: m.name,
            description: m.description,
            workspace_id: m.workspace_id,
            status: m.status,
            tag_ids: vec![],
            limits_config: m.limits_config,
            abnormal_handler_prompt: m.abnormal_handler_prompt,
            abnormal_handler_trigger_on: m.abnormal_handler_trigger_on,
            process_template_id: m.process_template_id,
            process_template_version: m.process_template_version,
            // 模板名称属跨表关联数据，不在 ORM 转换时隐式查询；
            // 由 handler 通过 with_process_template 在事务边界外注入（与 with_tags 同模式）。
            process_template_name: None,
            process_template_guid: None,
            process_template_display_name: None,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

impl LoopDto {
    /// 将外部查询到的标签关联附加到基础 DTO，保持 `From<loops::Model>` 只做纯字段映射。
    /// 标签属于跨表关联数据，不应在 ORM 模型转换时隐式查询，
    /// 由 handler 使用此方法在查询事务边界外手动注入。
    pub fn with_tags(mut self, tag_ids: Vec<i64>) -> Self {
        self.tag_ids = tag_ids;
        self
    }

    /// 注入来源工艺模板的名称信息（环路详情「来源工艺」面包屑用）。
    /// None 表示该环路非工艺实例化（或模板已被删除），字段保持缺省不序列化。
    pub fn with_process_template(mut self, meta: Option<process_templates::Model>) -> Self {
        if let Some(t) = meta {
            self.process_template_name = Some(t.name);
            self.process_template_guid = Some(t.guid);
            self.process_template_display_name = Some(t.display_name);
        }
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopStepDto {
    #[serde(flatten)]
    pub step: LoopStepRawDto,
    /// 冗余字段,JOIN 时一并查出来,避免前端再请求 step 模板详情。
    /// 修复 JOIN 误用 todos 表后：todo_title/todo_executor 现在都从 steps 表读。
    pub todo_title: String,
    pub todo_executor: String,
    /// 该环节引用的 todo 是否已归档。非空=已归档，Loop 详情图上标记，
    /// 提醒用户该环节指向已从日常视图隐藏的事项（归档不解除 Loop 引用）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub todo_archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopStepRawDto {
    pub id: i64,
    pub loop_id: i64,
    pub name: String,
    pub description: String,
    pub order_index: i32,
    /// 关联的 todo id
    pub todo_id: i64,
    /// 成功时策略: "next" | "goto" | "end"
    pub on_success: String,
    /// on_success="goto" 时的目标 step_id
    pub success_goto_step_id: Option<i64>,
    /// 评分不通过时策略: "break" | "skip" | "goto" | "end"
    pub on_rating_fail: String,
    /// on_rating_fail="goto" 时的目标 step_id
    pub fail_goto_step_id: Option<i64>,
    pub enabled: bool,
    pub created_at: Option<String>,
    /// 所属阶段 ID（process management）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_id: Option<i64>,
    /// 所属阶段名称，仅当 phase_id 有值时填充（工艺管理）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_name: Option<String>,
}

impl From<loop_steps::Model> for LoopStepRawDto {
    fn from(m: loop_steps::Model) -> Self {
        Self {
            id: m.id,
            loop_id: m.loop_id,
            name: m.name,
            description: m.description,
            order_index: m.order_index,
            todo_id: m.todo_id,
            on_success: m.on_success,
            success_goto_step_id: m.success_goto_step_id,
            on_rating_fail: m.on_rating_fail,
            fail_goto_step_id: m.fail_goto_step_id,
            enabled: m.enabled != 0,
            created_at: m.created_at,
            phase_id: m.phase_id,
            phase_name: None, // 名称由 handler 在查询 phases 后填入
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopExecutionDto {
    pub id: i64,
    pub loop_id: i64,
    pub trigger_id: Option<i64>,
    pub trigger_type: String,
    pub trigger_meta: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub total_steps: i32,
    pub completed_steps: i32,
    pub failed_steps: i32,
    /// 待人工审批的环节数（approval_status='pending' 的 loop_step_executions 数量）
    #[serde(default)]
    pub pending_approval_count: i32,
    /// 该次执行的 Token 消耗汇总（从 step_executions 的 usage 聚合计算）
    /// 仅在 list_executions 响应中由 handler 填充，从 Model 转换时默认为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_summary: Option<LoopExecutionTokenSummary>,
    /// 执行失败时的错误说明（仅在 status=failed 时有值）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

impl From<loop_executions::Model> for LoopExecutionDto {
    fn from(m: loop_executions::Model) -> Self {
        Self {
            id: m.id,
            loop_id: m.loop_id,
            trigger_id: m.trigger_id,
            trigger_type: m.trigger_type,
            trigger_meta: m.trigger_meta,
            started_at: m.started_at,
            finished_at: m.finished_at,
            status: m.status,
            total_steps: m.total_steps,
            completed_steps: m.completed_steps,
            failed_steps: m.failed_steps,
            pending_approval_count: 0, // 由 handler 后续查询填充
            token_summary: None,       // 由 handler 后续加载 step_executions 后聚合填充
            error_message: m.error_message, // 直接透传 DB 中的错误说明
        }
    }
}

/// 门禁评价摘要（需求 047）：随 LoopStepExecutionDto 下发，前端展示门禁级 status/result。
///
/// 字段与 `loop_step_execution_gates` 表对齐，前端复用 `GateDto` 渲染。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GateResultDto {
    pub id: i64,
    pub gate_type: String,
    pub gate_name: String,
    /// pending | passed | failed
    pub status: String,
    /// 评价结果文本（如「AI 评审未通过（评分 45，阈值 60）」）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct LoopStepExecutionDto {
    pub id: i64,
    pub loop_execution_id: i64,
    pub step_id: i64,
    pub todo_id: i64,
    pub execution_record_id: Option<i64>,
    pub status: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub error_message: Option<String>,
    /// AI 评审得分（0-100）。由 phase_driver 门禁评估时回写（resolve_step_rating）。
    pub rating: Option<i32>,
    /// 历史快照：旧评分制未达标策略，仅为历史记录展示保留
    pub unrated_policy: Option<String>,
    /// 门禁阈值（ai_criteria_review.min_score）。由 phase_driver 回写（set_step_execution_min_rating）。
    pub min_rating: Option<i32>,
    /// 环节名称，来自 loop_steps 表
    pub step_name: Option<String>,
    /// 全局执行序号（黑板用）
    pub sequence_index: i32,
    /// 本次步执行的结论摘要
    pub conclusion: Option<String>,
    /// 人工审批状态: NULL | "pending" | "approved"
    pub approval_status: Option<String>,
    /// 审批人的备注/意见
    pub approval_comment: Option<String>,
    /// 待审批的 human_approval 门禁 ID（044 门禁制审批）：
    /// 仅当环节处于 pending_approval 且存在 pending 门禁时由 handler 注入，
    /// 前端凭它调门禁审批接口，无需再查审计接口。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_gate_id: Option<i64>,
    /// 本次环节执行消耗的 token（从 execution_record.usage JSON 解析）
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_input_tokens: Option<i64>,
    pub cache_creation_input_tokens: Option<i64>,
    pub total_cost_usd: Option<f64>,
    /// 评分来源评审 record id（需求 047）：反查 source_execution_record_id 得到，
    /// 前端做可点击徽章跳转看评审理由。仅当有 execution_record_id 且被评审过时由 handler 注入。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_record_id: Option<i64>,
    /// 门禁级评价摘要（需求 047）：前端展示每个门禁的 status/result（失败原因）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gate_results: Vec<GateResultDto>,
}

impl From<loop_step_executions::Model> for LoopStepExecutionDto {
    fn from(m: loop_step_executions::Model) -> Self {
        Self {
            id: m.id,
            loop_execution_id: m.loop_execution_id,
            step_id: m.step_id,
            todo_id: m.todo_id,
            execution_record_id: m.execution_record_id,
            status: m.status,
            started_at: m.started_at,
            finished_at: m.finished_at,
            error_message: m.error_message,
            rating: m.rating,
            unrated_policy: m.unrated_policy,
            min_rating: m.min_rating,
            step_name: None,
            sequence_index: m.sequence_index,
            conclusion: m.conclusion,
            approval_status: m.approval_status,
            approval_comment: m.approval_comment,
            pending_gate_id: None, // 由 handler 在 pending_approval 时查询门禁注入
            input_tokens: None,
            output_tokens: None,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            total_cost_usd: None,
            review_record_id: None,
            gate_results: Vec::new(),
        }
    }
}

/// Loop Execution 附加的 token 汇总统计,
/// 由后端在 get_execution 时从 execution_records.usage JSON 聚合计算。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoopExecutionTokenSummary {
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cache_read_input_tokens: i64,
    pub total_cache_creation_input_tokens: i64,
    pub total_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopExecutionDetail {
    #[serde(flatten)]
    pub execution: LoopExecutionDto,
    pub step_executions: Vec<LoopStepExecutionDto>,
    pub loop_name: String,
    /// 本次 loop execution 的 token 汇总统计
    pub token_summary: LoopExecutionTokenSummary,
}

// ====== 请求体（仅保留运行态：启停）======

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateLoopStatusRequest {
    /// enabled | paused
    pub status: String,
}

pub fn validate_loop_status(s: &str) -> Result<(), String> {
    match s {
        "enabled" | "paused" => Ok(()),
        _ => Err(format!("未知的 loop status: {}", s)),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod loop_dto_tests {
    use super::*;

    /// 构造一个最小的 loops::Model：with_process_template 只关心 DTO 转换结果，
    /// 其余字段填零值即可（测试内允许直白构造，生产代码禁止的 unwrap 此处豁免）。
    fn minimal_loop_model() -> loops::Model {
        loops::Model {
            id: 1,
            name: "L".into(),
            description: String::new(),
            workspace_path: None,
            workspace_id: None,
            status: "paused".into(),
            limits_config: "{}".into(),
            abnormal_handler_todo_id: None,
            abnormal_handler_trigger_on: "[]".into(),
            abnormal_handler_prompt: None,
            process_template_id: Some(7),
            process_template_version: Some("1.2.0".into()),
            created_at: None,
            updated_at: None,
        }
    }

    /// 构造一个最小的 process_templates::Model，字段值仅供断言比对。
    fn minimal_template_model() -> process_templates::Model {
        process_templates::Model {
            id: 7,
            guid: "guid-4p12s".into(),
            name: "4p12s-delivery".into(),
            display_name: "标准需求交付工艺".into(),
            description: String::new(),
            category: "software".into(),
            complexity: "standard".into(),
            version: "1.2.0".into(),
            source_path: None,
            workspace_id: None,
            is_system: true,
            previous_version_id: None,
            created_at: None,
            updated_at: None,
        }
    }

    /// with_process_template(Some)：注入模板唯一名与显示名，版本快照来自 loops 行本身。
    #[test]
    fn test_with_process_template_injects_names() {
        let dto = LoopDto::from(minimal_loop_model())
            .with_process_template(Some(minimal_template_model()));
        assert_eq!(dto.process_template_name.as_deref(), Some("4p12s-delivery"));
        assert_eq!(
            dto.process_template_display_name.as_deref(),
            Some("标准需求交付工艺")
        );
        assert_eq!(dto.process_template_version.as_deref(), Some("1.2.0"));
    }

    /// with_process_template(None)：非工艺实例化环路（或模板已删）字段保持 None，
    /// 配合 skip_serializing_if 不出现在 JSON 中，前端据此前置隐藏面包屑。
    #[test]
    fn test_with_process_template_none_keeps_empty() {
        let dto = LoopDto::from(minimal_loop_model()).with_process_template(None);
        assert!(dto.process_template_name.is_none());
        assert!(dto.process_template_display_name.is_none());
    }
}
