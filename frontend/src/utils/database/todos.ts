import { api, unwrap } from './client';
import type { Todo, Tag, TodoTemplate, CustomTemplateStatus } from '../../types';
import type { TodoHookItem } from './hooks';

// Todo APIs

export async function getAllTodos(): Promise<Todo[]> {
  return unwrap(await api.get('/api/todos'));
}

export async function createTodo(
  title: string,
  prompt: string = '',
  tagIds: number[] = [],
  hooks: TodoHookItem[] = [],
): Promise<Todo> {
  return unwrap(await api.post('/api/todos', { title, prompt, tag_ids: tagIds, hooks }));
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
  hooks?: TodoHookItem[],
): Promise<Todo> {
  const body: Record<string, unknown> = { title, prompt, status };
  if (executor !== undefined) body.executor = executor;
  if (scheduler_enabled !== undefined) body.scheduler_enabled = scheduler_enabled;
  if (scheduler_config !== undefined) body.scheduler_config = scheduler_config;
  if (workspace !== undefined) body.workspace = workspace;
  if (worktree_enabled !== undefined) body.worktree_enabled = worktree_enabled;
  if (hooks !== undefined) body.hooks = hooks;

  return unwrap(await api.put(`/api/todos/${id}`, body));
}

export async function updateTodoHooks(id: number, hooks: TodoHookItem[]): Promise<Todo> {
  return unwrap(await api.put(`/api/todos/${id}`, { hooks }));
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

// Tag APIs

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
}

export async function getProjectDirectories(): Promise<ProjectDirectory[]> {
  return unwrap(await api.get('/api/project-directories'));
}

export async function createProjectDirectory(path: string, name: string): Promise<ProjectDirectory> {
  // 后端要求 name 必填；调用方需要保证传入非空字符串
  return unwrap(await api.post('/api/project-directories', { path, name }));
}

// 兜底用：路径存在时直接返回已有记录不更新名称，不存在时创建（name 可选）
export async function upsertProjectDirectoryIfNotExists(path: string, name?: string): Promise<ProjectDirectory> {
  return unwrap(await api.post('/api/project-directories/upsert-if-not-exists', { path, name: name || null }));
}

export async function updateProjectDirectory(id: number, name: string): Promise<void> {
  // 与新增保持一致：名称必填
  await api.put(`/api/project-directories/${id}`, { name });
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
