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
