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
  /** 0 = normal todo, 1 = 已废弃 (评审模板已迁出至 review_templates 表), 2 = review instance child. */
  todo_type?: 0 | 1 | 2;
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

/** 事项中心列表项：在 Todo 之上附加运行时推导/聚合字段（后端批量补算）。 */
export interface TodoCenterItem extends Todo {
  computed_bucket: ComputedBucket;
  /** 被启用 loop_steps 引用的次数（0=未被任何启用的 Loop 引用）。 */
  used_by_loop_step_count: number;
  /** 最近一次执行记录的状态，无记录则 undefined。 */
  last_execution_status?: string | null;
  /** 最近一次执行记录的时间（优先 finished_at，回退 started_at）。 */
  last_execution_at?: string | null;
}

/** 环节 — 从 todo 提升而来的独立实体，不再寄生在 Todo 上。 */
export interface StepSummary {
  id: number;
  title: string;
  prompt: string;
  executor?: string;
  acceptance_criteria?: string | null;
  source_todo_id?: number;
  /** 被多少个 loop step 引用 */
  used_by_loop_step_count: number;
  /** 标签 ID 列表（单选，复用 Todo 的标签体系） */
  tag_ids: number[];
  created_at?: string;
  updated_at?: string;
}

export interface Tag {
  id: number;
  name: string;
  color: string;
  created_at: string;
}

export interface TodoTag {
  todo_id: number;
  tag_id: number;
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

export interface CustomTemplateStatus {
  subscribed: boolean;
  source_url: string | null;
  last_sync_at: string | null;
  auto_sync_enabled: boolean;
  auto_sync_cron: string;
  templates: TodoTemplate[];
}

// 复用 database/todos.ts 中的定义，避免多处定义造成漂移
export type { ProjectDirectory } from '@/utils/database/todos';
