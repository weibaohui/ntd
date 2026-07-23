import { api, unwrap } from './client';
import type { ExecutionRecord, ExecutionSummary, ExecutionRecordsPage, ExecutionLogsPage, RunningBoardData } from '@/types';

// 所有 execution 端点嵌套在 /api/v1/workspaces/{ws}/executions 下（后端 ADR-7）。
// workspaceId 提升到 URL 路径段，调用方必须显式传入。

export async function getExecutionRecords(
  workspaceId: number,
  todoId?: number,
  page?: number,
  limit?: number,
  status?: string,
  stepId?: number,
): Promise<ExecutionRecordsPage> {
  const params: Record<string, unknown> = {};
  if (todoId !== undefined) params.todo_id = todoId;
  if (stepId !== undefined) params.step_id = stepId;
  if (page !== undefined) params.page = page;
  if (limit !== undefined) params.limit = limit;
  if (status !== undefined) params.status = status;
  return unwrap(await api.get(`/api/workspaces/${workspaceId}/executions`, { params }));
}

export async function getExecutionRecord(workspaceId: number, recordId: number): Promise<ExecutionRecord> {
  return unwrap(await api.get(`/api/workspaces/${workspaceId}/executions/${recordId}`));
}

export async function getExecutionLogs(workspaceId: number, recordId: number, page?: number, perPage?: number): Promise<ExecutionLogsPage> {
  const params: Record<string, unknown> = {};
  if (page !== undefined) params.page = page;
  if (perPage !== undefined) params.per_page = perPage;
  return unwrap(await api.get(`/api/workspaces/${workspaceId}/executions/${recordId}/logs`, { params }));
}

export async function getExecutionRecordsBySession(workspaceId: number, sessionId: string): Promise<ExecutionRecord[]> {
  return unwrap(await api.get(`/api/workspaces/${workspaceId}/executions/session/${encodeURIComponent(sessionId)}`));
}

/**
 * 执行指定 todo 任务。
 * 返回 task_id 用于 WebSocket 事件追踪，record_id 用于 UI 层立即获取并选中新创建的执行记录。
 */
export async function executeTodo(
  workspaceId: number,
  todoId: number,
  executor?: string,
  params?: Record<string, string>,
  model?: string | null,
): Promise<{ task_id: string; record_id: number }> {
  const body: Record<string, unknown> = { todo_id: todoId, executor, params };
  // model：undefined=不传（沿用执行器/任务级配置）；null/空串=显式清除本次执行模型
  if (model !== undefined) body.model = model;
  return unwrap(await api.post(`/api/workspaces/${workspaceId}/executions`, body));
}

/** 获取 todo 执行摘要，路径为 /todos/{id}/summary（嵌套在 workspace todos 下）。 */
export async function getExecutionSummary(workspaceId: number, todoId: number): Promise<ExecutionSummary> {
  return unwrap(await api.get(`/api/workspaces/${workspaceId}/todos/${todoId}/summary`));
}

export async function getRecentCompletedTodos(workspaceId: number, hours?: number): Promise<import('@/types').RecentCompletedTodo[]> {
  const p: Record<string, number> = {};
  if (hours !== undefined) p.hours = hours;
  return unwrap(await api.get(`/api/workspaces/${workspaceId}/todos/recent-completed`, { params: Object.keys(p).length > 0 ? p : undefined }));
}

/** GET /api/v1/workspaces/{ws}/stats/dashboard — 仪表盘聚合统计，workspace 隔离。 */
export async function getDashboardStats(workspaceId: number, hours?: number): Promise<import('@/types').DashboardStats> {
  const p = hours !== undefined ? { hours } : undefined;
  return unwrap(await api.get(`/api/workspaces/${workspaceId}/stats/dashboard`, { params: p }));
}

export async function stopExecution(workspaceId: number, recordId: number): Promise<void> {
  await api.post(`/api/workspaces/${workspaceId}/executions/${recordId}/stop`, { record_id: recordId });
}

export async function forceFailExecution(workspaceId: number, recordId: number): Promise<void> {
  await api.post(`/api/workspaces/${workspaceId}/executions/${recordId}/force-fail`, { record_id: recordId });
}

export async function getRunningExecutionRecords(workspaceId: number): Promise<ExecutionRecord[]> {
  return unwrap(await api.get(`/api/workspaces/${workspaceId}/executions/running`));
}

export async function resumeExecutionRecord(workspaceId: number, recordId: number, message?: string): Promise<{ task_id: string; record_id: number }> {
  return unwrap(await api.post(`/api/workspaces/${workspaceId}/executions/${recordId}/resume`, { message }));
}

/**
 * 给一条执行结果评分（0-100）。仅针对已结束的记录（success/failed）；
 * running 记录后端会拒绝。传 null 表示清除评分。
 */
export async function rateExecutionRecord(
  workspaceId: number,
  recordId: number,
  rating: number | null,
): Promise<ExecutionRecord> {
  return unwrap(await api.put(`/api/workspaces/${workspaceId}/executions/${recordId}/rating`, { rating }));
}

// Smart Create API — POST /api/v1/workspaces/{ws}/todos/smart（嵌套在 todos 下）

export interface SmartCreateResult {
  task_id: string;
  record_id: number;
  todo_id: number;
  todo_title: string;
}

export async function smartCreate(workspaceId: number, content: string): Promise<SmartCreateResult> {
  return unwrap(await api.post(`/api/workspaces/${workspaceId}/todos/smart`, { content }));
}

export async function getRunningBoardData(
  workspaceId: number,
  page?: number,
  limit?: number,
  hours?: number,
): Promise<RunningBoardData> {
  const params: Record<string, unknown> = {};
  if (page !== undefined) params.page = page;
  if (limit !== undefined) params.limit = limit;
  if (hours !== undefined) params.hours = hours;
  return unwrap(await api.get(`/api/workspaces/${workspaceId}/executions/running-board`, { params }));
}
