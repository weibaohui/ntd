import { api, unwrap } from './client';

/** 黑板响应（来自 GET /api/workspaces/{workspaceId}/blackboard） */
export interface BlackboardResponse {
  workspace_id: number;
  pending_record_ids: string; // JSON 数组字符串，如 "[12, 34, 56]"
}

/** GET /api/workspaces/{workspaceId}/blackboard：获取黑板数据（含 pending_record_ids） */
export async function getBlackboard(workspaceId: number): Promise<BlackboardResponse> {
  return unwrap(await api.get(`/api/workspaces/${workspaceId}/blackboard`));
}

/** PATCH /api/workspaces/{workspaceId}/blackboard：更新黑板 per-workspace 配置 */
export async function updateBlackboardConfig(
  workspaceId: number,
  config: {
    blackboard_debounce_secs?: number;
    blackboard_debounce_count?: number;
    wiki_prompt?: string;
  },
): Promise<void> {
  await api.patch(`/api/workspaces/${workspaceId}/blackboard`, config);
}
