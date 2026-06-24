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
  workspace?: string | null;
  worktree_enabled?: boolean;
  acceptance_criteria?: string | null;
  /** Whether to spawn an auto-review child todo after this one finishes. Default true. */
  auto_review_enabled?: boolean;
  /** 0 = normal todo, 1 = 已废弃 (评审模板已迁出至 review_templates 表), 2 = review instance child. */
  todo_type?: 0 | 1 | 2;
  /** For review instances: the original todo that was reviewed. */
  parent_todo_id?: number | null;
  /** For review instances: the review_template used to generate this instance. */
  review_template_id?: number | null;
  /** 事项 vs 环节。'item' = 一次性事项(默认), 'step' = 可复用的环节(loop 编排引用).
   * 后端 v3 migration 引入; 前端不传时按 'item' 兜底. */
  kind?: 'item' | 'step';
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
