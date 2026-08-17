// Loop Studio 类型定义。
//
// 与 backend/src/models/loop_.rs 一一对应：
// - LoopDto = 环路主表 DTO
// - LoopExecutionDto = 单次执行记录
//
// 需求 044（环路瘦身）：环路降级为「工艺的运行时承载」，
// 触发器（loop_triggers）、webhook、评审模板、手工创建/更新等概念整体下线，
// 本文件只保留只读查询与运行态（启停/标签/审批）所需的类型。
// 前端组件用这些类型组装 LoopStudio 页面。

type LoopStatus = 'enabled' | 'paused';

export interface LoopExecutionDto {
  id: number;
  loop_id: number;
  trigger_id: number | null;
  trigger_type: string;
  trigger_meta: string;
  started_at: string;
  finished_at: string | null;
  status: string;
  total_steps: number;
  completed_steps: number;
  failed_steps: number;
  total_executed_steps: number;
  /** 待人工审批的环节数 */
  pending_approval_count: number;
  /** 该次执行的 Token 消耗汇总（从 step_executions 的 usage 聚合计算） */
  token_summary?: LoopExecutionTokenSummary;
  /** 执行失败时的错误说明（仅在 status=failed 时有值） */
  error_message?: string | null;
}

/** 门禁评价摘要（需求 047）：门禁级 status/result，随 LoopStepExecutionDto 下发。 */
export interface GateResultDto {
  id: number;
  gate_type: string;
  gate_name: string;
  /** pending | passed | failed */
  status: string;
  /** 评价结果文本（如「AI 评审未通过（评分 45，阈值 60）」） */
  result?: string | null;
}

export interface LoopStepDto {
  id: number;
  loop_id: number;
  name: string;
  description: string;
  order_index: number;
  /** 关联的 todo id */
  todo_id: number;
  on_success: string;
  success_goto_step_id: number | null;
  on_rating_fail: string;
  fail_goto_step_id: number | null;
  enabled: boolean;
  created_at: string | null;
  /** 关联的 todo title */
  todo_title: string;
  /** 关联的 todo executor */
  todo_executor: string;
  /** 关联的 todo 归档时间。非空=已归档，Loop 详情图上标记，提醒环节指向已隐藏事项。 */
  todo_archived_at?: string | null;
  /** 所属阶段 ID（工艺管理） */
  phase_id?: number | null;
  /** 所属阶段名称（工艺管理） */
  phase_name?: string | null;
}

export interface LoopDetail {
  id: number;
  name: string;
  description: string;
  /** 工作空间 ID（workspaces.id，唯一键）。组件间统一以 id 传递，path 不再上送。 */
  workspace_id: number | null;
  status: string;
  limits_config: string;
  created_at: string | null;
  updated_at: string | null;
  steps: LoopStepDto[];
  /** 待人工审批的环节执行数 */
  pending_approval_count: number;
  /** 异常处理提示词快照（工艺定义）；null=未配置。需求 035。 */
  abnormal_handler_prompt?: string | null;
  /** 异常处理触发条件 JSON 字符串 */
  abnormal_handler_trigger_on?: string;
  /** 来源工艺模板 ID（后端 LoopDto 经 flatten 合并进详情；非工艺实例化时缺省） */
  process_template_id?: number | null;
  /** 实例化时的工艺版本快照（「来源工艺」面包屑展示用） */
  process_template_version?: string | null;
  /** 来源工艺模板唯一名（面包屑展示用；040 起不再唯一） */
  process_template_name?: string | null;
  /** 来源工艺模板 guid（040：面包屑回跳按 guid 寻址） */
  process_template_guid?: string | null;
  /** 来源工艺模板显示名（面包屑展示用） */
  process_template_display_name?: string | null;
}

export interface LoopListItem {
  id: number;
  name: string;
  description: string;
  /** 工作空间 ID（workspaces.id，唯一键）。组件间统一以 id 传递，path 不再上送。 */
  workspace_id: number | null;
  status: string;
  created_at: string | null;
  updated_at: string | null;
  step_count: number;
  last_execution_status: string;
  last_execution_at: string | null;
  /** 待人工审批的环节执行数 */
  pending_approval_count: number;
  /** 来源工艺模板 ID */
  process_template_id?: number | null;
  /** 来源工艺模板显示名（列表「工艺」列展示用；由后端列表接口注入） */
  process_template_display_name?: string | null;
  /** 来源工艺模板标识名（display_name 缺失时回退） */
  process_template_name?: string | null;
  /** 实例化时的工艺版本快照（列表「工艺」列展示用） */
  process_template_version?: string | null;
}

export interface LoopExecutionTokenSummary {
  total_input_tokens: number;
  total_output_tokens: number;
  total_cache_read_input_tokens: number;
  total_cache_creation_input_tokens: number;
  total_cost_usd: number;
}

export interface LoopExecutionDetail {
  id: number;
  loop_id: number;
  trigger_id: number | null;
  trigger_type: string;
  trigger_meta: string;
  started_at: string;
  finished_at: string | null;
  status: string;
  total_steps: number;
  completed_steps: number;
  failed_steps: number;
  total_executed_steps: number;
  step_executions: Record<string, any>[];
  loop_name: string;
  token_summary: LoopExecutionTokenSummary;
}

export interface LoopExecutionListResponse {
  items: LoopExecutionDto[];
  total: number;
  page: number;
  limit: number;
}

// ─── Request types ────────────────────────────────────────

export interface UpdateLoopStatusRequest {
  status: LoopStatus | string;
}

export interface LoopExecutionListQuery {
  page?: number;
  limit?: number;
  /** 按最近 N 小时过滤 */
  hours?: number;
}
