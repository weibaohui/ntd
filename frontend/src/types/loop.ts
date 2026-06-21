// Loop Studio 类型定义。
//
// 与 backend/src/models/loop_.rs 一一对应：
// - LoopDto = 环路主表 DTO
// - LoopTriggerDto = 触发器 DTO (cron / webhook / feishu / manual)
// - LoopExecutionDto = 单次执行记录
//
// 前端组件用这些类型组装 LoopStudio 页面。

export type LoopStatus = 'enabled' | 'paused';

export type LoopTriggerType =
  | 'manual'
  | 'cron'
  | 'webhook'
  | 'feishu_message'
  | 'feishu_command'
  | 'todo_completed'
  | 'todo_state_changed'
  | 'tag_added';

export type LoopRunMode = 'sequential';

export type LoopUnratedPolicy = 'skip' | 'continue';

export type LoopOnSuccessPolicy = 'next' | 'goto' | 'end';
export type LoopOnRatingFailPolicy = 'break' | 'skip' | 'goto' | 'end';

export type LoopExecutionStatus = 'running' | 'success' | 'partial' | 'failed' | 'cancelled' | 'capped';

export interface LoopDto {
  id: number;
  name: string;
  description: string;
  workspace: string | null;
  status: string;
  color: string;
  icon: string;
  limits_config: string;
  created_at: string | null;
  updated_at: string | null;
}

export interface LoopTriggerDto {
  id: number;
  loop_id: number;
  trigger_type: string; // 后端字符串灵活
  config: string; // JSON 字符串 (cron 表达式 / webhook_id / matches 等)
  enabled: boolean;
  priority: number;
  created_at: string | null;
}

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
}

export interface LoopStepExecutionDto {
  id: number;
  loop_execution_id: number;
  step_id: number;
  todo_id: number;
  status: string;
  execution_record_id: number | null;
  error_message: string | null;
  started_at: string | null;
  finished_at: string | null;
  rating: number | null;
  unrated_policy: string | null;
  min_rating: number | null;
  step_name: string | null;
  sequence_index: number;
  conclusion: string | null;
  /** 该环节消耗的 token（从关联的 execution_record.usage 解析） */
  input_tokens: number | null;
  output_tokens: number | null;
  cache_read_input_tokens: number | null;
  cache_creation_input_tokens: number | null;
  total_cost_usd: number | null;
}

export interface TodoSummaryForLoop {
  id: number;
  title: string;
  status: string;
  executor: string;
}

export interface LoopStepRawDto {
  id: number;
  loop_id: number;
  name: string;
  description: string;
  order_index: number;
  /** 关联的 step id（对应 steps 表） */
  step_id: number;
  run_mode: string;
  skip_on_source_failed: boolean;
  min_rating: number | null;
  unrated_policy: string;
  on_success: string;
  success_goto_step_id: number | null;
  on_rating_fail: string;
  fail_goto_step_id: number | null;
  enabled: boolean;
  created_at: string | null;
}

export interface LoopStepDto {
  id: number;
  loop_id: number;
  name: string;
  description: string;
  order_index: number;
  /** 关联的 step id（对应 steps 表） */
  step_id: number;
  run_mode: string;
  skip_on_source_failed: boolean;
  min_rating: number | null;
  unrated_policy: string;
  on_success: string;
  success_goto_step_id: number | null;
  on_rating_fail: string;
  fail_goto_step_id: number | null;
  enabled: boolean;
  created_at: string | null;
  todo_title: string;
  todo_executor: string;
  todo_status: string;
}

export interface CreateLoopStepRequest {
  name: string;
  description?: string;
  /** 关联的 step id（对应 steps 表） */
  step_id: number;
  run_mode?: string;
  skip_on_source_failed?: boolean;
  min_rating?: number | null;
  unrated_policy?: string;
  enabled?: boolean;
  on_success?: string;
  success_goto_step_id?: number | null;
  on_rating_fail?: string;
  fail_goto_step_id?: number | null;
}

export interface UpdateLoopStepRequest {
  name: string;
  description: string;
  /** 关联的 step id（对应 steps 表） */
  step_id: number;
  run_mode: string;
  skip_on_source_failed: boolean;
  min_rating: number | null;
  unrated_policy: string;
  enabled: boolean;
  on_success: string;
  success_goto_step_id: number | null;
  on_rating_fail: string;
  fail_goto_step_id: number | null;
}

export interface ReorderLoopStepsRequest {
  ordered_ids: number[];
}

export interface LoopDetail {
  id: number;
  name: string;
  description: string;
  workspace: string | null;
  status: string;
  color: string;
  icon: string;
  limits_config: string;
  created_at: string | null;
  updated_at: string | null;
  triggers: LoopTriggerDto[];
  steps: LoopStepDto[];
  todo_map: Record<number, TodoSummaryForLoop>;
}

export interface LoopListItem {
  id: number;
  name: string;
  description: string;
  workspace: string | null;
  status: string;
  color: string;
  icon: string;
  created_at: string | null;
  updated_at: string | null;
  trigger_count: number;
  step_count: number;
  last_execution_status: string;
  last_execution_at: string | null;
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

export interface CreateLoopRequest {
  name: string;
  description?: string;
  workspace?: string | null;
  color?: string;
  icon?: string;
  review_template_id?: number | null;
}

export interface UpdateLoopRequest {
  name: string;
  description: string;
  workspace: string | null;
  color: string;
  icon: string;
  review_template_id?: number | null;
  limits_config?: string | null;
}

export interface CreateTriggerRequest {
  trigger_type: LoopTriggerType | string;
  config?: string; // 默认 "{}"
  enabled?: boolean;
  priority?: number;
}

export interface UpdateTriggerRequest {
  trigger_type: LoopTriggerType | string;
  config: string;
  enabled: boolean;
  priority: number;
}

export interface UpdateLoopStatusRequest {
  status: LoopStatus | string;
}

export interface LoopExecutionListQuery {
  page?: number;
  limit?: number;
}

export interface LoopTriggerResponse {
  execution_id: number;
}
