import { api, unwrap } from './client';
import type { ExecutionRecord, ExecutionSummary, ExecutionRecordsPage, ExecutionLogsPage } from '../../types';

export async function getExecutionRecords(todoId: number, page?: number, limit?: number, status?: string): Promise<ExecutionRecordsPage> {
  const params: Record<string, unknown> = { todo_id: todoId };
  if (page !== undefined) params.page = page;
  if (limit !== undefined) params.limit = limit;
  if (status !== undefined) params.status = status;
  return unwrap(await api.get('/xyz/execution-records', { params }));
}

export async function getExecutionRecord(recordId: number): Promise<ExecutionRecord> {
  return unwrap(await api.get(`/xyz/execution-records/${recordId}`));
}

export async function getExecutionLogs(recordId: number, page?: number, perPage?: number): Promise<ExecutionLogsPage> {
  const params: Record<string, unknown> = {};
  if (page !== undefined) params.page = page;
  if (perPage !== undefined) params.per_page = perPage;
  return unwrap(await api.get(`/xyz/execution-records/${recordId}/logs`, { params }));
}

export async function getExecutionRecordsBySession(sessionId: string): Promise<ExecutionRecord[]> {
  return unwrap(await api.get(`/xyz/execution-records/session/${encodeURIComponent(sessionId)}`));
}

export async function executeTodo(todoId: number, executor?: string, params?: Record<string, string>): Promise<{ task_id: string }> {
  return unwrap(await api.post('/xyz/execute', { todo_id: todoId, executor, params }));
}

export async function getExecutionSummary(todoId: number): Promise<ExecutionSummary> {
  return unwrap(await api.get(`/xyz/todos/${todoId}/summary`));
}

export async function getRecentCompletedTodos(hours?: number): Promise<import('../../types').RecentCompletedTodo[]> {
  const p = hours !== undefined ? { hours } : undefined;
  return unwrap(await api.get('/xyz/todos/recent-completed', { params: p }));
}

export async function getDashboardStats(hours?: number): Promise<import('../../types').DashboardStats> {
  const p = hours !== undefined ? { hours } : undefined;
  return unwrap(await api.get('/xyz/dashboard-stats', { params: p }));
}

export async function stopExecution(recordId: number): Promise<void> {
  await api.post('/xyz/execute/stop', { record_id: recordId });
}

export async function forceFailExecution(recordId: number): Promise<void> {
  await api.post('/xyz/execute/force-fail', { record_id: recordId });
}

export async function getRunningExecutionRecords(): Promise<ExecutionRecord[]> {
  return unwrap(await api.get('/xyz/execution-records/running'));
}

export async function resumeExecutionRecord(recordId: number, message?: string): Promise<{ task_id: string; record_id: number }> {
  return unwrap(await api.post(`/xyz/execution-records/${recordId}/resume`, { message }));
}

// Smart Create API

export interface SmartCreateResult {
  task_id: string;
  record_id: number;
  todo_id: number;
  todo_title: string;
}

export async function smartCreate(content: string): Promise<SmartCreateResult> {
  return unwrap(await api.post('/xyz/smart-create', { content }));
}
