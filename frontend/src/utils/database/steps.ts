// 环节（steps 表）相关 API 客户端。
//
// 环节是独立实体，不再寄生在 todos 表上：
// - GET  /api/steps               列出环节 + 各自的 loop 引用计数
// - POST /api/steps               直建环节（todo/step 拆开后，绕开 createTodo+promote）
// - GET  /api/steps/candidates    loop 编辑器选环节用的精简候选
// - GET  /api/steps/:id           单个环节详情 + 引用计数
// - POST /api/todos/:id/promote   事项 → 环节（保留入口，老 todo 升 step 时仍在用）
//
// 后端位于 handlers/step_.rs（直建）+ handlers/todo.rs（promote 保留）。

import { api, unwrap } from './client';
import type { StepSummary } from '@/types';

/** 列出所有环节 + 各自的 loop step 引用计数。 */
export async function listSteps(): Promise<StepSummary[]> {
  return unwrap(await api.get('/api/steps'));
}

/** loop 编辑器选环节用的精简候选列表。 */
export async function listStepCandidates(): Promise<StepSummary[]> {
  return unwrap(await api.get('/api/steps/candidates'));
}

/** 单个环节详情, 返回 StepSummary (含 used_by_loop_step_count)。 */
export async function getStep(id: number): Promise<StepSummary> {
  return unwrap(await api.get(`/api/steps/${id}`));
}

/**
 * 直建环节。
 * 取代旧的「先 createTodo 再 promoteTodoToStep」两步走 —— 那条路径会留孤儿 todo，
 * 且 promote 后的 step id 与原 todo id 不一致，前端选中错 id 触发 404。
 * todo 与 step 已彻底拆开，新环节必须直接写 steps 表。
 */
export async function createStep(input: {
  title: string;
  prompt?: string;
  executor?: string;
  acceptance_criteria?: string;
}): Promise<StepSummary> {
  return unwrap(await api.post('/api/steps', input));
}

/** 事项提升为环节。复制数据到 steps 表，原 todo 保留。返回新建的 StepSummary。
 *  仅用于"老 todo 升级为 step"的存量迁移场景；新建环节请用 createStep。 */
export async function promoteTodoToStep(id: number): Promise<StepSummary> {
  return unwrap(await api.post(`/api/todos/${id}/promote`, {}));
}

/** 更新环节基本信息（部分更新：只传需要变更的字段即可）。 */
export async function updateStep(
  id: number,
  data: { title?: string; prompt?: string; executor?: string | null; acceptance_criteria?: string | null; color?: string },
): Promise<StepSummary> {
  return unwrap(await api.put(`/api/steps/${id}`, data));
}

/** 删除环节。若被 loop 引用，后端会返回外键错误。 */
export async function deleteStep(id: number): Promise<void> {
  await api.delete(`/api/steps/${id}`);
}

/** 批量更新环节执行器。后端提供专用接口，单次 SQL 完成。 */
export async function batchUpdateStepsExecutor(
  ids: number[],
  executor: string,
): Promise<{ updated: number[]; failed: number[] }> {
  try {
    const result = await unwrap(await api.put('/api/steps/batch-executor', { ids, executor }));
    const body = result as { updated_count: number; total: number };
    return { updated: ids.slice(0, body.updated_count), failed: ids.slice(body.updated_count) };
  } catch {
    return { updated: [], failed: ids };
  }
}