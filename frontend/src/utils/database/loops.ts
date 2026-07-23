// Loop Studio API 客户端。
//
// 后端 v1 路由（backend/src/handlers/loop_.rs 的 v1_routes）嵌套在
// /api/v1/workspaces/{ws}/loops 下，workspaceId 必须在 URL 路径段中显式传入。
// 所有函数的第一个参数都是 workspaceId（即 project_directories.id）。

import { api, unwrap } from './client';
import type {
  CreateLoopRequest,
  CreateLoopStepRequest,
  CreateTriggerRequest,
  LoopDetail,
  LoopExecutionDetail,
  LoopExecutionListQuery,
  LoopExecutionListResponse,
  LoopListItem,
  LoopStepDto,
  LoopTriggerDto,
  LoopTriggerResponse,
  UpdateLoopRequest,
  UpdateLoopStatusRequest,
  UpdateLoopStepRequest,
  UpdateTriggerRequest,
} from '@/types/loop';

// ====== Loop 主体 ======

/** 列出指定工作空间下的所有 loop,按更新时间倒序。 */
export async function listLoops(workspaceId: number | null): Promise<LoopListItem[]> {
  // null workspace 时仍发请求（后端可能在 URL 用 0 或报错），让调用方处理失败
  const ws = workspaceId ?? 0;
  return unwrap(await api.get(`/api/v1/workspaces/${ws}/loops`));
}

// ====== Loop 聚合统计(dashboard「自动化」Tab)======

/** Loop 聚合统计,与后端 models::LoopStats 对齐。 */
export interface LoopStats {
  total_loops: number;
  active_loops: number;
  total_executions: number;
  success_executions: number;
  failed_executions: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cost_usd: number;
  /** 触发类型分布(按 loop_executions.trigger_type GROUP BY)。 */
  trigger_type_distribution: LoopTriggerTypeCount[];
}

/** Loop 触发类型分布项。 */
export interface LoopTriggerTypeCount {
  trigger_type: string;
  count: number;
  success_count: number;
  failed_count: number;
}

/** GET /api/v1/workspaces/{ws}/loops/stats:按工作空间聚合 loop 统计。hours 缺省或 0 表示全时段。 */
export async function getLoopStats(workspaceId: number, hours?: number): Promise<LoopStats> {
  const qs = hours && hours > 0 ? `?hours=${hours}` : '';
  return unwrap(await api.get(`/api/v1/workspaces/${workspaceId}/loops/stats${qs}`));
}

/** 单个 loop 详情,含 triggers/steps/hooks/todo_map。 */
export async function getLoop(workspaceId: number, id: number): Promise<LoopDetail> {
  return unwrap(await api.get(`/api/v1/workspaces/${workspaceId}/loops/${id}`));
}

/** 新建 loop,后端强制 status=paused。 */
export async function createLoop(workspaceId: number, req: CreateLoopRequest): Promise<LoopListItem> {
  return unwrap(await api.post(`/api/v1/workspaces/${workspaceId}/loops`, req));
}

/** 全量更新 loop 基本字段。 */
export async function updateLoop(workspaceId: number, id: number, req: UpdateLoopRequest): Promise<LoopListItem> {
  return unwrap(await api.put(`/api/v1/workspaces/${workspaceId}/loops/${id}`, req));
}

/** 删除 loop,级联清子表。 */
export async function deleteLoop(workspaceId: number, id: number): Promise<void> {
  await api.delete(`/api/v1/workspaces/${workspaceId}/loops/${id}`);
}

/** 切换 loop 状态(enabled/paused)。 */
export async function updateLoopStatus(
  workspaceId: number,
  id: number,
  req: UpdateLoopStatusRequest,
): Promise<LoopListItem> {
  return unwrap(await api.put(`/api/v1/workspaces/${workspaceId}/loops/${id}/status`, req));
}

/** 复制 loop(返回新 loop)。 */
export async function duplicateLoop(workspaceId: number, id: number): Promise<LoopListItem> {
  return unwrap(await api.post(`/api/v1/workspaces/${workspaceId}/loops/${id}/duplicate`, {}));
}

/** 手动触发 loop,返回新创建的 execution_id。 */
export async function triggerLoop(workspaceId: number, id: number): Promise<LoopTriggerResponse> {
  return unwrap(await api.post(`/api/v1/workspaces/${workspaceId}/loops/${id}/trigger`, {}));
}

// ====== Triggers ======

export async function createTrigger(
  workspaceId: number,
  loopId: number,
  req: CreateTriggerRequest,
): Promise<LoopTriggerDto> {
  return unwrap(await api.post(`/api/v1/workspaces/${workspaceId}/loops/${loopId}/triggers`, req));
}

export async function updateTrigger(
  workspaceId: number,
  loopId: number,
  triggerId: number,
  req: UpdateTriggerRequest,
): Promise<LoopTriggerDto> {
  return unwrap(await api.put(`/api/v1/workspaces/${workspaceId}/loops/${loopId}/triggers/${triggerId}`, req));
}

export async function deleteTrigger(workspaceId: number, loopId: number, triggerId: number): Promise<void> {
  await api.delete(`/api/v1/workspaces/${workspaceId}/loops/${loopId}/triggers/${triggerId}`);
}

// ====== Steps ======

export async function createLoopStep(
  workspaceId: number,
  loopId: number,
  req: CreateLoopStepRequest,
): Promise<LoopStepDto> {
  return unwrap(await api.post(`/api/v1/workspaces/${workspaceId}/loops/${loopId}/steps`, req));
}

export async function updateLoopStep(
  workspaceId: number,
  loopId: number,
  stepId: number,
  req: UpdateLoopStepRequest,
): Promise<LoopStepDto> {
  return unwrap(await api.put(`/api/v1/workspaces/${workspaceId}/loops/${loopId}/steps/${stepId}`, req));
}

export async function deleteLoopStep(workspaceId: number, loopId: number, stepId: number): Promise<void> {
  await api.delete(`/api/v1/workspaces/${workspaceId}/loops/${loopId}/steps/${stepId}`);
}

// ====== Executions ======

/** 分页列出 loop 运行历史。 */
export async function listExecutions(
  workspaceId: number,
  loopId: number,
  query: LoopExecutionListQuery = {},
): Promise<LoopExecutionListResponse> {
  const params: Record<string, string> = {};
  if (query.page) params.page = String(query.page);
  if (query.limit) params.limit = String(query.limit);
  if (query.hours) params.hours = String(query.hours);
  const qs = Object.keys(params).length ? `?${new URLSearchParams(params).toString()}` : '';
  return unwrap(await api.get(`/api/v1/workspaces/${workspaceId}/loops/${loopId}/executions${qs}`));
}

/** 单次执行详情(含 step_executions)。 */
export async function getExecution(
  workspaceId: number,
  loopId: number,
  executionId: number,
): Promise<LoopExecutionDetail> {
  return unwrap(await api.get(`/api/v1/workspaces/${workspaceId}/loops/${loopId}/executions/${executionId}`));
}

/** 通过执行 ID 直接获取执行详情（无需 loop_id），供消息历史跳转使用。 */
export async function getExecutionById(
  workspaceId: number,
  executionId: number,
): Promise<LoopExecutionDetail> {
  return unwrap(await api.get(`/api/v1/workspaces/${workspaceId}/loop-executions/${executionId}`));
}

/**
 * 人工审批环节执行。
 * POST /api/v1/workspaces/{ws}/loops/{loopId}/executions/{executionId}/steps/{stepExecutionId}/approve
 */
export async function approveStepExecution(
  workspaceId: number,
  loopId: number,
  executionId: number,
  stepExecutionId: number,
  rating: number,
  comment?: string,
): Promise<{ step_execution_id: number; rating: number; status: string }> {
  return unwrap(await api.post(
    `/api/v1/workspaces/${workspaceId}/loops/${loopId}/executions/${executionId}/steps/${stepExecutionId}/approve`,
    { rating, comment },
  ));
}

// ─── 批量操作 ───────────────────────────────────────────────────

/**
 * 批量强停环路（占位实现）。
 *
 * 后端目前没有「批量强停 loop」接口，这里只放前端占位：直接返回"全部失败"。
 */
export async function forceStopLoops(
  loopIds: number[],
): Promise<{ stopped: number[]; failed: number[] }> {
  return { stopped: [], failed: [...loopIds] };
}

/** 批量移动环路到其他工作空间。workspaceId 为源空间，workspace_id 为目标空间。 */
export async function batchMoveLoopsWorkspace(
  workspaceId: number,
  ids: number[],
  workspace_id: number,
): Promise<{ updated_count: number; total: number }> {
  return unwrap(await api.post(`/api/v1/workspaces/${workspaceId}/loops/batch/workspace`, { ids, workspace_id }));
}

/** 批量复制环路到其他工作空间。workspaceId 为源空间，workspace_id 为目标空间。 */
export async function batchCopyLoopsWorkspace(
  workspaceId: number,
  ids: number[],
  workspace_id: number,
): Promise<{ updated_count: number; total: number }> {
  return unwrap(await api.post(`/api/v1/workspaces/${workspaceId}/loops/batch/copy-workspace`, { ids, workspace_id }));
}


// ====== Loop 导入导出 ======

/** 导出单个环路为 YAML（原生 fetch，不走 axios 拦截器，手动写 v1 路径） */
export async function exportLoop(workspaceId: number, id: number): Promise<string> {
  const response = await fetch(`/api/v1/workspaces/${workspaceId}/loops/${id}/export`);
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  return response.text();
}

/** 导出指定工作空间的所有环路为单个 YAML（原生 fetch，手动写 v1 路径） */
export async function exportAllLoops(workspaceId: number): Promise<string> {
  const response = await fetch(`/api/v1/workspaces/${workspaceId}/loops/export`);
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  return response.text();
}
