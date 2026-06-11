import { api, unwrap } from './client';
import type { Todo, Tag, TodoTemplate, CustomTemplateStatus } from '@/types';
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
  acceptanceCriteria?: string,
): Promise<Todo> {
  const body: Record<string, unknown> = { title, prompt, tag_ids: tagIds, hooks };
  if (acceptanceCriteria !== undefined) body.acceptance_criteria = acceptanceCriteria;
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
  hooks?: TodoHookItem[],
  acceptance_criteria?: string | null,
): Promise<Todo> {
  const body: Record<string, unknown> = { title, prompt, status };
  if (executor !== undefined) body.executor = executor;
  if (scheduler_enabled !== undefined) body.scheduler_enabled = scheduler_enabled;
  if (scheduler_config !== undefined) body.scheduler_config = scheduler_config;
  if (workspace !== undefined) body.workspace = workspace;
  if (worktree_enabled !== undefined) body.worktree_enabled = worktree_enabled;
  if (hooks !== undefined) body.hooks = hooks;
  if (acceptance_criteria !== undefined) body.acceptance_criteria = acceptance_criteria;

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

// 创建项目目录：后端要求 name 必填，调用方需保证传入非空字符串。
// 返回完整 ProjectDirectory 对象（含 id），供调用方更新本地状态。
export async function createProjectDirectory(path: string, name: string): Promise<ProjectDirectory> {
  return unwrap(await api.post('/api/project-directories', { path, name })); // POST 创建，后端会做 trim+唯一约束校验
}

// 更新项目目录名称：与新增保持一致约束（名称必填），避免出现"无名项目"。
// path 不可变，仅允许修改 name。
export async function updateProjectDirectory(id: number, name: string): Promise<void> {
  await api.put(`/api/project-directories/${id}`, { name }); // PUT 全量替换 name 字段
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
