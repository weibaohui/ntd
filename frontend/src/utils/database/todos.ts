import { api, unwrap } from './client';
import type { Todo, Tag, TodoTemplate, CustomTemplateStatus } from '@/types';

// Todo APIs

/**
 * 列出 todos，可选按 kind 过滤（'item' / 'step' / 不传=全部）。
 * 后端 get_todos handler 支持 `?kind=` query；不传则保持向后兼容，返回所有 todo。
 */
export async function getAllTodos(kind?: 'item' | 'step' | 'all'): Promise<Todo[]> {
  const query = kind && kind !== 'all' ? `?kind=${encodeURIComponent(kind)}` : '';
  return unwrap(await api.get(`/api/todos${query}`));
}

export async function createTodo(
  title: string,
  prompt: string = '',
  tagIds: number[] = [],
  acceptanceCriteria?: string,
  autoReviewEnabled?: boolean,
): Promise<Todo> {
  const body: Record<string, unknown> = { title, prompt, tag_ids: tagIds };
  if (acceptanceCriteria !== undefined) body.acceptance_criteria = acceptanceCriteria;
  if (autoReviewEnabled !== undefined) body.auto_review_enabled = autoReviewEnabled;
  return unwrap(await api.post('/api/todos', body));
}

export async function updateTodo(
  id: number,
  title: string,
  prompt: string,
  status: string,
  executor?: string,
  scheduler_enabled?: boolean,
  scheduler_config?: string | null,
  workspace?: string | null,
  worktree_enabled?: boolean,
  acceptance_criteria?: string | null,
  auto_review_enabled?: boolean,
): Promise<Todo> {
  const body: Record<string, unknown> = { title, prompt, status };
  if (executor !== undefined) body.executor = executor;
  if (scheduler_enabled !== undefined) body.scheduler_enabled = scheduler_enabled;
  if (scheduler_config !== undefined) body.scheduler_config = scheduler_config;
  if (workspace !== undefined) body.workspace = workspace;
  if (worktree_enabled !== undefined) body.worktree_enabled = worktree_enabled;
  if (acceptance_criteria !== undefined) body.acceptance_criteria = acceptance_criteria;
  if (auto_review_enabled !== undefined) body.auto_review_enabled = auto_review_enabled;

  return unwrap(await api.put(`/api/todos/${id}`, body));
}

export async function deleteTodo(id: number): Promise<void> {
  await api.delete(`/api/todos/${id}`);
}

export async function forceUpdateTodoStatus(id: number, status: string): Promise<Todo> {
  return unwrap(await api.put(`/api/todos/${id}/force-status`, { status }));
}

export async function updateTodoTags(todoId: number, tagIds: number[]): Promise<void> {
  await api.put(`/api/todos/${todoId}/tags`, { tag_ids: tagIds });
}

/** 批量更新事项执行器。后端提供专用接口，单次 SQL 完成。 */
export async function batchUpdateTodosExecutor(
  ids: number[],
  executor: string,
): Promise<{ updated: number[]; failed: number[] }> {
  try {
    const result = await unwrap(await api.put('/api/todos/batch-executor', { ids, executor }));
    const body = result as { updated_count: number; total: number };
    return { updated: ids.slice(0, body.updated_count), failed: ids.slice(body.updated_count) };
  } catch {
    return { updated: [], failed: ids };
  }
}

// Tag APIs

/** 单个 todo 详情（用于批量操作前取 title/prompt 等不可变字段）。 */
export async function getTodo(id: number): Promise<Todo> {
  return unwrap(await api.get(`/api/todos/${id}`));
}

export async function getAllTags(): Promise<Tag[]> {
  return unwrap(await api.get('/api/tags'));
}

export async function createTag(name: string, color: string): Promise<Tag> {
  return unwrap(await api.post('/api/tags', { name, color }));
}

export async function deleteTag(id: number): Promise<void> {
  await api.delete(`/api/tags/${id}`);
}

// Todo Template APIs

export async function getTodoTemplates(): Promise<TodoTemplate[]> {
  return unwrap(await api.get('/api/todo-templates'));
}

export async function createTodoTemplate(title: string, prompt: string | null, category: string, sort_order?: number): Promise<TodoTemplate> {
  return unwrap(await api.post('/api/todo-templates', { title, prompt, category, sort_order }));
}

export async function updateTodoTemplate(id: number, title?: string, prompt?: string | null, category?: string, sort_order?: number): Promise<TodoTemplate> {
  const body: Record<string, unknown> = {};
  if (title !== undefined) body.title = title;
  if (prompt !== undefined) body.prompt = prompt;
  if (category !== undefined) body.category = category;
  if (sort_order !== undefined) body.sort_order = sort_order;
  return unwrap(await api.put(`/api/todo-templates/${id}`, body));
}

export async function deleteTodoTemplate(id: number): Promise<void> {
  await api.delete(`/api/todo-templates/${id}`);
}

export async function copyTodoTemplate(id: number): Promise<TodoTemplate> {
  return unwrap(await api.post(`/api/todo-templates/${id}/copy`, {}));
}

// Custom Template APIs (remote URL subscription)

export async function getCustomTemplateStatus(): Promise<CustomTemplateStatus> {
  return unwrap(await api.get('/api/custom-templates/status'));
}

export async function subscribeCustomTemplate(url: string): Promise<CustomTemplateStatus> {
  return unwrap(await api.post('/api/custom-templates/subscribe', { url }));
}

export async function unsubscribeCustomTemplate(): Promise<void> {
  await api.post('/api/custom-templates/unsubscribe', {});
}

export async function syncCustomTemplate(): Promise<CustomTemplateStatus> {
  return unwrap(await api.post('/api/custom-templates/sync', {}));
}

export async function updateCustomTemplateAutoSync(enabled: boolean, cron: string): Promise<void> {
  await api.put('/api/custom-templates/auto-sync', { enabled, cron });
}

// Project Directory APIs

export interface ProjectDirectory {
  id: number;
  path: string;
  name: string | null;
  created_at: string;
  updated_at: string;
  // issue #643: 项目目录级 git worktree 开关。
  // 后端从 v2 schema migration 开始携带这两个字段；旧库会是 false（migration 默认值）。
  git_worktree_enabled?: boolean;
  auto_cleanup?: boolean;
}

export async function getProjectDirectories(): Promise<ProjectDirectory[]> {
  return unwrap(await api.get('/api/project-directories'));
}

// 创建项目目录：后端要求 name 必填，调用方需保证传入非空字符串。
// 返回完整 ProjectDirectory 对象（含 id），供调用方更新本地状态。
// issue #643 修复：create 接口在后端并不消费 gitWorktreeEnabled / autoCleanup 字段，
// 发送它们只会让前端误以为「新建时就能决定策略」，实际上策略需要在 update 时设置。
// 这里彻底删除 options 参数与对应 body 字段，调用方需要时改走 updateProjectDirectory。
export async function createProjectDirectory(
  path: string,
  name: string,
): Promise<ProjectDirectory> {
  return unwrap(await api.post('/api/project-directories', { path, name }));
}

// 更新项目目录。`name` 必填；worktree 开关可选（不传=保持现状）。
// 后端在 handler 区分 `None`/`Some` 两种语义，前端用 hasOwnProperty 表达"我故意没传"。
export async function updateProjectDirectory(
  id: number,
  name: string,
  options?: { gitWorktreeEnabled?: boolean; autoCleanup?: boolean },
): Promise<void> {
  const body: Record<string, unknown> = { name };
  if (options?.gitWorktreeEnabled !== undefined) {
    body.git_worktree_enabled = options.gitWorktreeEnabled;
  }
  if (options?.autoCleanup !== undefined) {
    body.auto_cleanup = options.autoCleanup;
  }
  await api.put(`/api/project-directories/${id}`, body);
}

export async function deleteProjectDirectory(id: number): Promise<void> {
  await api.delete(`/api/project-directories/${id}`);
}

// Scheduler APIs

export async function updateScheduler(
  id: number,
  scheduler_enabled: boolean,
  scheduler_config: string | null,
): Promise<Todo> {
  return unwrap(await api.put(`/api/todos/${id}/scheduler`, { scheduler_enabled, scheduler_config }));
}

export async function getSchedulerTodos(): Promise<Todo[]> {
  return unwrap(await api.get('/api/scheduler/todos'));
}

export async function getRunningTodos(): Promise<Todo[]> {
  return unwrap(await api.get('/api/running-todos'));
}
