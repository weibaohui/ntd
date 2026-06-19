// 环节（steps 表）相关 API 客户端。
//
// 环节是独立实体，不再寄生在 todos 表上：
// - GET /api/steps              列出环节 + 各自的 loop 引用计数
// - GET /api/steps/candidates   loop 编辑器选环节用的精简候选
// - GET /api/steps/:id          单个环节详情 + 引用计数
// - POST /api/todos/:id/promote   事项 → 环节（复制到 steps 表，原 todo 保留）
//
// 后端位于 handlers/todo.rs。

import { api, unwrap } from './client';
import type { StepSummary } from '@/types';

/** 列出所有环节 + 各自的 loop stage 引用计数。 */
export async function listSteps(): Promise<StepSummary[]> {
  return unwrap(await api.get('/api/steps'));
}

/** loop 编辑器选环节用的精简候选列表。 */
export async function listStepCandidates(): Promise<StepSummary[]> {
  return unwrap(await api.get('/api/steps/candidates'));
}

/** 单个环节详情, 返回 StepSummary (含 used_by_loop_stage_count)。 */
export async function getStep(id: number): Promise<StepSummary> {
  return unwrap(await api.get(`/api/steps/${id}`));
}

/** 事项提升为环节。复制数据到 steps 表，原 todo 保留。返回新建的 StepSummary。 */
export async function promoteTodoToStep(id: number): Promise<StepSummary> {
  return unwrap(await api.post(`/api/todos/${id}/promote`, {}));
}

/** 更新环节基本信息。 */
export async function updateStep(
  id: number,
  data: { title: string; prompt?: string; executor?: string | null; acceptance_criteria?: string | null },
): Promise<StepSummary> {
  return unwrap(await api.put(`/api/steps/${id}`, data));
}