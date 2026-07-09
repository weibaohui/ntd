// Loop Studio 类型定义。
//
// 与 backend/src/models/loop_.rs 一一对应：
// - LoopDto = 环路主表 DTO
// - LoopTriggerDto = 触发器 DTO (cron / feishu / manual)
// - LoopExecutionDto = 单次执行记录
//
// 前端组件用这些类型组装 LoopStudio 页面。

export type LoopStatus = 'enabled' | 'paused';

export type LoopTriggerType =
  | 'manual'
  | 'cron'
  | 'feishu_message'
  | 'feishu_command'
  | 'todo_completed'
  | 'todo_state_changed'
  | 'tag_added';

export type LoopRunMode = 'sequential';

export type LoopUnratedPolicy = 'skip' | 'continue';

export type LoopOnSuccessPolicy = 'next' | 'goto' | 'end';
export type LoopOnRatingFailPolicy = 'break' | 'skip' | 'goto' | 'end';

export type ReviewType = 'ai' | 'human';

export type LoopExecutionStatus = 'running' | 'success' | 'partial' | 'failed' | 'cancelled' | 'capped_step' | 'capped_token';

export interface LoopDto {
  id: number;
  name: string;
  description: string;
  /**
   * 工作空间 ID（project_directories.id），唯一键。
   * 组件间统一传 id，path 不再上送 API；前端展示用 project_directories.name/path 自行查询。
   */
  workspace_id: number | null;
  webhook_enabled: boolean;
  status: string;
  /** 标签 ID 列表（单选，复用 Todo 的标签体系） */
  tag_ids: number[];
  icon: string;
  limits_config: string;
  /** 异常处理 Todo ID */
  abnormal_handler_todo_id: number | null;
  /** 异常处理触发条件 JSON 数组 */
  abnormal_handler_trigger_on: string;
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
  /** 待人工审批的环节数 */
  pending_approval_count: number;
  /** 该次执行的 Token 消耗汇总（从 step_executions 的 usage 聚合计算） */
  token_summary?: LoopExecutionTokenSummary;
  /** 执行失败时的错误说明（仅在 status=failed 时有值） */
  error_message?: string | null;
}

export interface LoopStepExecutionDto {
  id: number;
  loop_execution_id: number;
  /** 关联的 loop_step id（FK 到 loop_steps 表） */
  loop_step_id: number;
  /** 关联的 todo id（FK 到 todos 表） */
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
  /** 人工审批状态: null | 'pending' | 'approved' */
  approval_status: string | null;
  /** 审批人的备注/意见 */
  approval_comment: string | null;
  output_tokens: number | null;
  cache_read_input_tokens: number | null;
  cache_creation_input_tokens: number | null;
  total_cost_usd: number | null;
}

export interface LoopStepRawDto {
  id: number;
  loop_id: number;
  name: string;
  description: string;
  order_index: number;
  /** 关联的 todo id */
  todo_id: number;
  run_mode: string;
  skip_on_source_failed: boolean;
  min_rating: number | null;
  unrated_policy: string;
  on_success: string;
  success_goto_step_id: number | null;
  on_rating_fail: string;
  fail_goto_step_id: number | null;
  /** 评审类型: 'ai' = AI 自动评审, 'human' = 人工审批 */
  review_type: string;
  enabled: boolean;
  created_at: string | null;
}

export interface LoopStepDto {
  id: number;
  loop_id: number;
  name: string;
  description: string;
  order_index: number;
  /** 关联的 todo id */
  todo_id: number;
  run_mode: string;
  skip_on_source_failed: boolean;
  min_rating: number | null;
  unrated_policy: string;
  on_success: string;
  success_goto_step_id: number | null;
  on_rating_fail: string;
  fail_goto_step_id: number | null;
  /** 评审类型: 'ai' = AI 自动评审, 'human' = 人工审批 */
  review_type: string;
  enabled: boolean;
  created_at: string | null;
  /** 关联的 todo title */
  todo_title: string;
  /** 关联的 todo executor */
  todo_executor: string;
  /** 关联的 todo 归档时间。非空=已归档，Loop 详情图上标记，提醒环节指向已隐藏事项。 */
  todo_archived_at?: string | null;
}

export interface CreateLoopStepRequest {
  name: string;
  description?: string;
  /** 关联的 todo id */
  todo_id: number;
  run_mode?: string;
  skip_on_source_failed?: boolean;
  min_rating?: number | null;
  unrated_policy?: string;
  enabled?: boolean;
  on_success?: string;
  success_goto_step_id?: number | null;
  on_rating_fail?: string;
  fail_goto_step_id?: number | null;
  /** 评审类型: 'ai' | 'human'，默认 'ai' */
  review_type?: string;
}

export interface UpdateLoopStepRequest {
  name: string;
  description: string;
  /** 关联的 todo id */
  todo_id: number;
  run_mode: string;
  skip_on_source_failed: boolean;
  min_rating: number | null;
  unrated_policy: string;
  enabled: boolean;
  on_success: string;
  success_goto_step_id: number | null;
  on_rating_fail: string;
  fail_goto_step_id: number | null;
  /** 评审类型: 'ai' | 'human'，默认 'ai' */
  review_type?: string;
}

export interface ApproveStepExecutionRequest {
  rating: number;
  comment?: string;
}

export interface ReorderLoopStepsRequest {
  ordered_ids: number[];
}

export interface LoopDetail {
  id: number;
  name: string;
  description: string;
  /** 工作空间 ID（project_directories.id，唯一键）。组件间统一以 id 传递，path 不再上送。 */
  workspace_id: number | null;
  webhook_enabled: boolean;
  status: string;
  /** 标签 ID 列表（单选，复用 Todo 的标签体系） */
  tag_ids: number[];
  icon: string;
  limits_config: string;
  /** Review template to use for auto-review on loop steps. null = use default. */
  review_template_id: number | null;
  created_at: string | null;
  updated_at: string | null;
  triggers: LoopTriggerDto[];
  steps: LoopStepDto[];
  /** 待人工审批的环节执行数 */
  pending_approval_count: number;
  /** 异常处理 Todo ID */
  abnormal_handler_todo_id?: number | null;
  /** 异常处理触发条件 JSON 字符串 */
  abnormal_handler_trigger_on?: string;
}

export interface LoopListItem {
  id: number;
  name: string;
  description: string;
  /** 工作空间 ID（project_directories.id，唯一键）。组件间统一以 id 传递，path 不再上送。 */
  workspace_id: number | null;
  webhook_enabled: boolean;
  status: string;
  /** 标签 ID 列表（单选，复用 Todo 的标签体系） */
  tag_ids: number[];
  icon: string;
  created_at: string | null;
  updated_at: string | null;
  trigger_count: number;
  step_count: number;
  last_execution_status: string;
  last_execution_at: string | null;
  /** 待人工审批的环节执行数 */
  pending_approval_count: number;
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
  /**
   * 工作空间 ID（project_directories.id），必填、唯一键。
   * 不再接受路径——避免 path 不唯一带来的歧义。
   */
  workspace_id: number;
  webhook_enabled?: boolean;
  tag_ids?: number[];
  /** 创建时可选预绑定单个标签；省略时后端按空标签处理。 */
  icon?: string;
  review_template_id?: number | null;
  /** 限制条件 JSON 字符串 */
  limits_config?: string | null;
  /** 异常处理 Todo ID */
  abnormal_handler_todo_id?: number | null;
  /** 异常处理触发条件 JSON 数组 */
  abnormal_handler_trigger_on?: string;
}

export interface UpdateLoopRequest {
  name: string;
  description: string;
  /**
   * 工作空间 ID（project_directories.id）。
   * undefined = 保持原工作空间；显式赋值（包含 null）= 迁移/清空。
   * 不接受路径——handler 一律按 id 解析 cwd 路径写入两列。
   */
  workspace_id?: number | null;
  webhook_enabled: boolean;
  icon: string;
  review_template_id?: number | null;
  limits_config?: string | null;
  /** 异常处理 Todo ID */
  abnormal_handler_todo_id?: number | null;
  /** 异常处理触发条件 JSON 数组 */
  abnormal_handler_trigger_on?: string;
  /** 可选更新标签 ID（传空数组清除标签，省略不更新）。合并到同一请求避免多次 API 调用导致的部分提交风险。 */
  tag_ids?: number[] | null;
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
  /** 按最近 N 小时过滤 */
  hours?: number;
}

export interface LoopTriggerResponse {
  execution_id: number;
}
