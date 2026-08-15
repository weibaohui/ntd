// ─── Core Todo types ────────────────────────────────────────

export interface Todo {
  id: number;
  title: string;
  prompt: string;
  status: 'pending' | 'running' | 'completed' | 'failed';
  created_at: string;
  updated_at: string;
  deleted_at: string | null;
  tag_ids: number[];
  executor?: string;
  /** 关联的专家/团队名称（WorkBuddy 专家系统） */
  expert_name?: string | null;
  /** 任务级执行模型（覆盖执行器默认）。null = 用执行器默认模型。 */
  model?: string | null;
  /** 事项级技能名列表（需求 055）。工艺安装时从环节 skills 写入；执行时以 /skill-name 注入 prompt 尾部。 */
  skills?: string[];
  scheduler_enabled?: boolean;
  scheduler_config?: string | null;
  scheduler_timezone?: string | null;
  scheduler_next_run_at?: string | null;
  task_id?: string | null;
  workspace_path?: string | null;
  workspace_id?: number | null;
  webhook_enabled?: boolean;
  acceptance_criteria?: string | null;
  /** 已废弃：UI 层面不再展示该开关，事项执行后不再触发自动评审。保留字段用于 API 向下兼容。 */
  auto_review_enabled?: boolean;
  /** 0 = normal todo, 1 = 已废弃 (评审模板已迁出至 review_templates 表), 2 = review instance child, 3 = 异常处理载体 todo（需求 035）. */
  todo_type?: 0 | 1 | 2 | 3;
  /** For review instances: the original todo that was reviewed. */
  parent_todo_id?: number | null;
  /** For review instances: the review_template used to generate this instance. */
  review_template_id?: number | null;
  /** Action 类型标记（如 blackboard/title_optimize），用于卡片来源提示，不影响执行逻辑。 */
  action_type?: string | null;
  /** Action 键值，与 action_type 配合唯一标识一个 action 模板 todo。 */
  action_key?: string | null;
  /** 归档时间戳（UTC）。null/undefined=未归档；非空=已归档，从日常视图隐藏但数据保留。 */
  archived_at?: string | null;
}

// ─── 事项中心（Todo Center）类型 ────────────────────────────

/** 事项中心五类驱动分类（computed_bucket），由后端按事实字段推导，不落库。 */
export type ComputedBucket = 'manual' | 'time_driven' | 'event_driven' | 'loop_driven' | 'archived';

/** 引用该事项的 Loop 摘要（事项中心 Loop 驱动卡片「所属 Loop」用）。 */
export interface LoopRefSummary {
  loop_id: number;
  loop_name: string;
  /** 该环路所基于的工艺模板 ID（后端 LEFT JOIN process_templates，未绑定时缺省） */
  process_template_id?: number;
  /** 该环路所基于的工艺模板名称（取 process_templates.display_name） */
  process_template_name?: string;
  /** 工艺版本（优先 loops.process_template_version 快照，缺失时后端回退模板当前版本） */
  process_template_version?: string;
}

/** 事项中心列表项：在 Todo 之上附加运行时推导/聚合字段（后端批量补算）。 */
export interface TodoCenterItem extends Todo {
  computed_bucket: ComputedBucket;
  /** 被启用 loop_steps 引用的次数（0=未被任何启用的 Loop 引用）。 */
  used_by_loop_step_count: number;
  /** 最近一次执行记录的状态，无记录则 undefined。 */
  last_execution_status?: string | null;
  /** 最近一次执行记录的时间（优先 finished_at，回退 started_at）。 */
  last_execution_at?: string | null;
  /** 引用该事项的启用 Loop 摘要。仅 Loop 驱动分类非空，供卡片展示并跳转 Loop 详情。 */
  referencing_loops?: LoopRefSummary[];
  /** 连续失败次数：从最近一次执行往前数连续 failed 的条数。0=最近非失败或无记录。 */
  consecutive_failure_count?: number;
  /** 最近一次 webhook 触发的时间（trigger_type='webhook' 的最新记录）。事件驱动卡片「最近触发」用。 */
  last_webhook_trigger_at?: string | null;
  /** 绑定的工作空间斜杠命令（command_type='todo' 绑定该 todo 的第一条）。卡片展示「绑定命令: /xxx」。 */
  bound_slash_command?: string | null;
}

/** 事项中心服务端分页响应（056）。bucket_counts 为各分类计数（应用 search/status/actionType 后、应用 bucket 前）。 */
export interface TodoCenterPage {
  items: TodoCenterItem[];
  total: number;
  page: number;
  page_size: number;
  bucket_counts: Record<string, number>;
  /** 当前工作空间内出现过的 action_type 去重列表（来源筛选下拉数据源）。 */
  action_types: string[];
}

/** 事项轻量摘要（056）：不含 prompt 大字段，看板/下拉/记录补标题用。 */
export interface TodoBrief {
  id: number;
  title: string;
  status: Todo['status'];
  executor?: string | null;
  updated_at: string;
  archived_at?: string | null;
  workspace_id?: number | null;
  tag_ids: number[];
  /** prompt 是否非空（看板「展开 prompt」的显示开关，内容按需另取）。 */
  has_prompt: boolean;
}

/** 事项列表分页响应（056，旧全量接口改造后的结构）。 */
export interface TodoListPage {
  items: Todo[];
  total: number;
  page: number;
  page_size: number;
}

export interface Tag {
  id: number;
  name: string;
  color: string;
  created_at: string;
}

export interface TodoItem {
  id?: string;
  content: string;
  status: 'pending' | 'in_progress' | 'completed';
}

export interface TodoTemplate {
  id: number;
  title: string;
  prompt: string | null;
  category: string;
  sort_order: number;
  is_system: boolean;
  source_url?: string | null;
  last_sync_at?: string | null;
  created_at: string | null;
  updated_at: string | null;
}

// 复用 database/todos.ts 中的定义，避免多处定义造成漂移
export type { Workspace } from '@/utils/database/todos';
