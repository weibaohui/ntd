import type { TodoHookItem } from '@/utils/database/hooks';

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
  hooks?: TodoHookItem[];
  acceptance_criteria?: string | null;
  /** Whether to spawn an auto-review child todo after this one finishes. Default true. */
  auto_review_enabled?: boolean;
  /** 0 = normal todo, 1 = reviewer template, 2 = review instance child. */
  todo_type?: 0 | 1 | 2;
  /** For review instances: the original todo that was reviewed. */
  parent_todo_id?: number | null;
  /** 事项 vs 环节。'item' = 一次性事项(默认), 'expert' = 可复用的环节(loop 编排引用).
   * 后端 v3 migration 引入; 前端不传时按 'item' 兜底. */
  kind?: 'item' | 'expert';
}

/** 环节视图 — 在 Todo 基础上叠加"被多少个 loop 引用"复用度指标. */
export interface StepSummary extends Todo {
  /** 被多少个 loop stage 引用; 0 = 没有任何 loop 在用(孤儿环节). */
  used_by_loop_stage_count: number;
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
