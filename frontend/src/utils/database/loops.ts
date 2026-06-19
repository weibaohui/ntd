// Loop Studio API 客户端。
//
// 后端路由在 backend/src/handlers/loop_.rs：
// - GET    /api/loops                        列表(带 trigger/stage/exec 计数)
// - POST   /api/loops                        新建(draft)
// - GET    /api/loops/{id}                   详情(loop + triggers + stages + hooks + todo_map)
// - PUT    /api/loops/{id}                   全量更新基本字段
// - DELETE /api/loops/{id}                   删除(级联清子表)
// - PUT    /api/loops/{id}/status            切换 draft/enabled/paused
// - POST   /api/loops/{id}/duplicate         复制
// - POST   /api/loops/{id}/trigger           手动触发
// - GET/POST/PUT/DELETE /api/loops/{id}/triggers[/tid]
// - GET/POST/PUT/DELETE /api/loops/{id}/hooks[/hid]
// - GET    /api/loops/{id}/executions        运行历史(分页)
// - GET    /api/loops/{id}/executions/{eid}  单次执行详情

import { api, unwrap } from './client';
import type {
  CreateHookRequest,
  CreateLoopRequest,
  CreateTriggerRequest,
  LoopDetail,
  LoopExecutionDetail,
  LoopExecutionListQuery,
  LoopExecutionListResponse,
  LoopHookDto,
  LoopListItem,
  LoopTriggerDto,
  LoopTriggerResponse,
  UpdateHookRequest,
  UpdateLoopRequest,
  UpdateLoopStatusRequest,
  UpdateTriggerRequest,
} from '@/types/loop';

// ====== Loop 主体 ======

/** 列出所有 loop,按更新时间倒序。 */
export async function listLoops(): Promise<LoopListItem[]> {
  return unwrap(await api.get('/api/loops'));
}

/** 单个 loop 详情,含 triggers/stages/hooks/todo_map。 */
export async function getLoop(id: number): Promise<LoopDetail> {
  return unwrap(await api.get(`/api/loops/${id}`));
}

/** 新建 loop,后端强制 status=draft。 */
export async function createLoop(req: CreateLoopRequest): Promise<LoopListItem> {
  return unwrap(await api.post('/api/loops', req));
}

/** 全量更新 loop 基本字段。 */
export async function updateLoop(id: number, req: UpdateLoopRequest): Promise<LoopListItem> {
  return unwrap(await api.put(`/api/loops/${id}`, req));
}

/** 删除 loop,级联清子表。 */
export async function deleteLoop(id: number): Promise<void> {
  await api.delete(`/api/loops/${id}`);
}

/** 切换 loop 状态(draft/enabled/paused)。 */
export async function updateLoopStatus(
  id: number,
  req: UpdateLoopStatusRequest,
): Promise<LoopListItem> {
  return unwrap(await api.put(`/api/loops/${id}/status`, req));
}

/** 复制 loop(返回新 loop)。 */
export async function duplicateLoop(id: number): Promise<LoopListItem> {
  return unwrap(await api.post(`/api/loops/${id}/duplicate`, {}));
}

/** 手动触发 loop,返回新创建的 execution_id。 */
export async function triggerLoop(id: number): Promise<LoopTriggerResponse> {
  return unwrap(await api.post(`/api/loops/${id}/trigger`, {}));
}

// ====== Triggers ======

export async function listTriggers(loopId: number): Promise<LoopTriggerDto[]> {
  return unwrap(await api.get(`/api/loops/${loopId}/triggers`));
}

export async function createTrigger(
  loopId: number,
  req: CreateTriggerRequest,
): Promise<LoopTriggerDto> {
  return unwrap(await api.post(`/api/loops/${loopId}/triggers`, req));
}

export async function updateTrigger(
  loopId: number,
  triggerId: number,
  req: UpdateTriggerRequest,
): Promise<LoopTriggerDto> {
  return unwrap(await api.put(`/api/loops/${loopId}/triggers/${triggerId}`, req));
}

export async function deleteTrigger(loopId: number, triggerId: number): Promise<void> {
  await api.delete(`/api/loops/${loopId}/triggers/${triggerId}`);
}

// ====== Hooks ======

export async function listHooks(loopId: number): Promise<LoopHookDto[]> {
  return unwrap(await api.get(`/api/loops/${loopId}/hooks`));
}

export async function createHook(loopId: number, req: CreateHookRequest): Promise<LoopHookDto> {
  return unwrap(await api.post(`/api/loops/${loopId}/hooks`, req));
}

export async function updateHook(
  loopId: number,
  hookId: number,
  req: UpdateHookRequest,
): Promise<LoopHookDto> {
  return unwrap(await api.put(`/api/loops/${loopId}/hooks/${hookId}`, req));
}

export async function deleteHook(loopId: number, hookId: number): Promise<void> {
  await api.delete(`/api/loops/${loopId}/hooks/${hookId}`);
}

// ====== Executions ======

/** 分页列出 loop 运行历史。 */
export async function listExecutions(
  loopId: number,
  query: LoopExecutionListQuery = {},
): Promise<LoopExecutionListResponse> {
  const params: Record<string, string> = {};
  if (query.page) params.page = String(query.page);
  if (query.limit) params.limit = String(query.limit);
  const qs = Object.keys(params).length ? `?${new URLSearchParams(params).toString()}` : '';
  return unwrap(await api.get(`/api/loops/${loopId}/executions${qs}`));
}

/** 单次执行详情(含 stage_executions)。 */
export async function getExecution(
  loopId: number,
  executionId: number,
): Promise<LoopExecutionDetail> {
  return unwrap(await api.get(`/api/loops/${loopId}/executions/${executionId}`));
}
