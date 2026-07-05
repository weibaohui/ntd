import { api } from './client';

/** PATCH /api/workspaces/{workspaceId}/blackboard：更新黑板 per-workspace 配置 */
export async function updateBlackboardConfig(
  workspaceId: number,
  config: {
    blackboard_debounce_secs?: number;
    blackboard_debounce_count?: number;
    wiki_index_prompt?: string;
    wiki_page_prompt?: string;
  },
): Promise<void> {
  await api.patch(`/api/workspaces/${workspaceId}/blackboard`, config);
}
