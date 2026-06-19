// 环节（kind=expert）相关 API 客户端。
//
// 设计与 todo CRUD 共享底层（todos.kind 列），但语义独立：
// - GET /api/steps              列出环节 + 各自的 loop 引用计数
// - GET /api/steps/candidates   loop 编辑器选环节用的精简候选
// - GET /api/steps/:id          单个环节详情 + 引用计数
// - POST /api/todos/:id/promote   事项 → 环节
// - POST /api/todos/:id/demote    环节 → 事项（被 loop 引用时拒绝）
//
// 后端位于 handlers/todo.rs，实现走 db/todo.rs 的 list_experts / promote_to_expert 等。

import { api, unwrap } from './client';
import type { StepSummary, Todo } from '@/types';

/** 列出所有环节 + 各自的 loop stage 引用计数（按 updated_at 倒序）。 */
export async function listSteps(): Promise<StepSummary[]> {
  return unwrap(await api.get('/api/steps'));
}

/** loop 编辑器选环节用的精简候选列表（字段与 Todo 一致, 无 used_by 计数）。 */
export async function listStepCandidates(): Promise<Todo[]> {
  return unwrap(await api.get('/api/steps/candidates'));
}

/** 单个环节详情, 返回 StepSummary (含 used_by_loop_stage_count)。 */
export async function getStep(id: number): Promise<StepSummary> {
  return unwrap(await api.get(`/api/steps/${id}`));
}

/** 事项提升为环节。返回 void, 失败抛 ApiError。 */
export async function promoteTodoToStep(id: number): Promise<void> {
  await api.post(`/api/todos/${id}/promote`, {});
}

/** 环节降级为事项。被 loop_stages 引用时后端返回 400。 */
export async function demoteTodoToItem(id: number): Promise<void> {
  await api.post(`/api/todos/${id}/demote`, {});
}