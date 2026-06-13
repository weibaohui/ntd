import { api, unwrap } from './client';
import type { ExecutionRecord, ExecutionSummary, ExecutionRecordsPage, ExecutionLogsPage, RunningBoardData } from '@/types';

export async function getExecutionRecords(todoId: number, page?: number, limit?: number, status?: string): Promise<ExecutionRecordsPage> {
  const params: Record<string, unknown> = { todo_id: todoId };
  if (page !== undefined) params.page = page;
  if (limit !== undefined) params.limit = limit;
  if (status !== undefined) params.status = status;
  return unwrap(await api.get('/api/execution-records', { params }));
}

export async function getExecutionRecord(recordId: number): Promise<ExecutionRecord> {
  return unwrap(await api.get(`/api/execution-records/${recordId}`));
}

export async function getExecutionLogs(recordId: number, page?: number, perPage?: number): Promise<ExecutionLogsPage> {
  const params: Record<string, unknown> = {};
  if (page !== undefined) params.page = page;
  if (perPage !== undefined) params.per_page = perPage;
  return unwrap(await api.get(`/api/execution-records/${recordId}/logs`, { params }));
}

export async function getExecutionRecordsBySession(sessionId: string): Promise<ExecutionRecord[]> {
  return unwrap(await api.get(`/api/execution-records/session/${encodeURIComponent(sessionId)}`));
}

/**
 * 执行指定 todo 任务。
 * 返回 task_id 用于 WebSocket 事件追踪，record_id 用于 UI 层立即获取并选中新创建的执行记录。
 */
export async function executeTodo(todoId: number, executor?: string, params?: Record<string, string>): Promise<{ task_id: string; record_id: number }> {
  return unwrap(await api.post('/api/execute', { todo_id: todoId, executor, params }));
}

export async function getExecutionSummary(todoId: number): Promise<ExecutionSummary> {
  return unwrap(await api.get(`/api/todos/${todoId}/summary`));
}

export async function getRecentCompletedTodos(hours?: number): Promise<import('@/types').RecentCompletedTodo[]> {
  const p = hours !== undefined ? { hours } : undefined;
  return unwrap(await api.get('/api/todos/recent-completed', { params: p }));
}

export async function getDashboardStats(hours?: number): Promise<import('@/types').DashboardStats> {
  const p = hours !== undefined ? { hours } : undefined;
  return unwrap(await api.get('/api/dashboard-stats', { params: p }));
}

export async function stopExecution(recordId: number): Promise<void> {
  await api.post('/api/execute/stop', { record_id: recordId });
}

export async function forceFailExecution(recordId: number): Promise<void> {
  await api.post('/api/execute/force-fail', { record_id: recordId });
}

export async function getRunningExecutionRecords(): Promise<ExecutionRecord[]> {
  return unwrap(await api.get('/api/execution-records/running'));
}

export async function resumeExecutionRecord(recordId: number, message?: string): Promise<{ task_id: string; record_id: number }> {
  return unwrap(await api.post(`/api/execution-records/${recordId}/resume`, { message }));
}

/**
 * 给一条执行结果评分（0-100）。仅针对已结束的记录（success/failed）；
 * running 记录后端会拒绝。传 null 表示清除评分。
 */
export async function rateExecutionRecord(
  recordId: number,
  rating: number | null,
): Promise<ExecutionRecord> {
  return unwrap(await api.put(`/api/execution-records/${recordId}/rating`, { rating }));
}

// Smart Create API

export interface SmartCreateResult {
  task_id: string;
  record_id: number;
  todo_id: number;
  todo_title: string;
}

export async function smartCreate(content: string): Promise<SmartCreateResult> {
  return unwrap(await api.post('/api/smart-create', { content }));
}

export async function getRunningBoardData(page?: number, limit?: number): Promise<RunningBoardData> {
  const params: Record<string, unknown> = {};
  if (page !== undefined) params.page = page;
  if (limit !== undefined) params.limit = limit;
  return unwrap(await api.get('/api/running-board', { params }));
}
