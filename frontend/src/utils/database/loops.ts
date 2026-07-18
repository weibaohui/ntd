// Loop Studio API 客户端。
//
// 后端路由在 backend/src/handlers/loop_.rs：
// - GET    /api/loops                        列表(带 trigger/step/exec 计数)
// - POST   /api/loops                        新建(draft)
// - GET    /api/loops/{id}                   详情(loop + triggers + steps + todo_map)
// - PUT    /api/loops/{id}                   全量更新基本字段
// - DELETE /api/loops/{id}                   删除(级联清子表)
// - PUT    /api/loops/{id}/status            切换 enabled/paused
// - POST   /api/loops/{id}/duplicate         复制
// - POST   /api/loops/{id}/trigger           手动触发
// - GET/POST/PUT/DELETE /api/loops/{id}/triggers[/tid]
// - GET    /api/loops/{id}/executions        运行历史(分页)
// - GET    /api/loops/{id}/executions/{eid}  单次执行详情

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

/** 列出所有 loop,按更新时间倒序。可选按工作空间 ID 过滤。 */
export async function listLoops(workspace_id?: number | null): Promise<LoopListItem[]> {
  const params: Record<string, string> = {};
  if (workspace_id != null) params.workspace_id = String(workspace_id);
  const qs = Object.keys(params).length ? `?${new URLSearchParams(params).toString()}` : '';
  return unwrap(await api.get(`/api/loops${qs}`));
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

/** GET /api/loops/stats?hours=N:全 loop 聚合统计。hours 缺省或 0 表示全时段。 */
export async function getLoopStats(hours?: number): Promise<LoopStats> {
  const qs = hours && hours > 0 ? `?hours=${hours}` : '';
  return unwrap(await api.get(`/api/loops/stats${qs}`));
}

/** 单个 loop 详情,含 triggers/steps/hooks/todo_map。 */
export async function getLoop(id: number): Promise<LoopDetail> {
  return unwrap(await api.get(`/api/loops/${id}`));
}

/** 新建 loop,后端强制 status=paused。 */
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

/** 切换 loop 状态(enabled/paused)。 */
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

// ====== Steps ======

export async function createLoopStep(
  loopId: number,
  req: CreateLoopStepRequest,
): Promise<LoopStepDto> {
  return unwrap(await api.post(`/api/loops/${loopId}/steps`, req));
}

export async function updateLoopStep(
  loopId: number,
  stepId: number,
  req: UpdateLoopStepRequest,
): Promise<LoopStepDto> {
  return unwrap(await api.put(`/api/loops/${loopId}/steps/${stepId}`, req));
}

export async function deleteLoopStep(loopId: number, stepId: number): Promise<void> {
  await api.delete(`/api/loops/${loopId}/steps/${stepId}`);
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
  if (query.hours) params.hours = String(query.hours);
  const qs = Object.keys(params).length ? `?${new URLSearchParams(params).toString()}` : '';
  return unwrap(await api.get(`/api/loops/${loopId}/executions${qs}`));
}

/** 单次执行详情(含 step_executions)。 */
export async function getExecution(
  loopId: number,
  executionId: number,
): Promise<LoopExecutionDetail> {
  return unwrap(await api.get(`/api/loops/${loopId}/executions/${executionId}`));
}

/** 通过执行 ID 直接获取执行详情（无需 loop_id），供消息历史跳转使用。 */
export async function getExecutionById(
  executionId: number,
): Promise<LoopExecutionDetail> {
  return unwrap(await api.get(`/api/loop-executions/${executionId}`));
}

/**
 * 人工审批环节执行。
 * POST /api/loops/{loopId}/executions/{executionId}/steps/{stepExecutionId}/approve
 */
export async function approveStepExecution(
  loopId: number,
  executionId: number,
  stepExecutionId: number,
  rating: number,
  comment?: string,
): Promise<{ step_execution_id: number; rating: number; status: string }> {
  return unwrap(await api.post(
    `/api/loops/${loopId}/executions/${executionId}/steps/${stepExecutionId}/approve`,
    { rating, comment },
  ));
}

// ─── 批量操作（占位实现） ────────────────────────────────────────

/**
 * 批量强停环路。
 *
 * 后端目前没有「批量强停 loop」接口（handlers/loop_.rs 仅提供单条
 * execution_record 的 stop API），这里只放前端占位：弹提示 + 返回空结果。
 * 后续接入真实接口时，把函数体换成单次 POST 即可，外部签名保持不变。
 *
 * 期望后端契约：
 *   POST /api/loops/batch-stop
 *   body: { loop_ids: number[] }
 *   response: { stopped: number[], failed: number[] }
 */
export async function forceStopLoops(
  loopIds: number[],
): Promise<{ stopped: number[]; failed: number[] }> {
  // 占位：开发中提示由调用方弹（utils 内部不依赖 message 上下文）。
  // 直接返回"全部失败"，强制调用方走失败分支走提示。
  return { stopped: [], failed: [...loopIds] };
}

/** 批量移动环路到其他工作空间。 */
export async function batchMoveLoopsWorkspace(
  ids: number[],
  workspace_id: number,
): Promise<{ updated_count: number; total: number }> {
  return unwrap(await api.put('/api/loops/batch-workspace', { ids, workspace_id }));
}

/** 批量复制环路到其他工作空间。 */
export async function batchCopyLoopsWorkspace(
  ids: number[],
  workspace_id: number,
): Promise<{ updated_count: number; total: number }> {
  return unwrap(await api.post('/api/loops/batch-copy-workspace', { ids, workspace_id }));
}


// ====== Loop 导入导出 ======

/** 导出单个环路为 YAML */
export async function exportLoop(id: number): Promise<string> {
  const response = await fetch(`/api/loops/${id}/export`);
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  return response.text();
}

/** 导出全库所有环路为单个 YAML，对齐 Todo「导出全部」 */
export async function exportAllLoops(): Promise<string> {
  const response = await fetch('/api/loops/export');
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  return response.text();
}

