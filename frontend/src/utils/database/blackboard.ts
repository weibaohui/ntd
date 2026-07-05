import { api, unwrap } from './client';

/** 黑板响应（来自 GET /api/workspaces/{workspaceId}/blackboard） */
export interface BlackboardResponse {
  workspace_id: number;
  pending_record_ids: string; // JSON 数组字符串，如 "[12, 34, 56]"
  blackboard_debounce_secs: number;
  blackboard_debounce_count: number;
  wiki_prompt: string;
  /** Wiki 对话使用的执行器名称，null/undefined 表示使用默认值 claudecode */
  wiki_chat_executor?: string | null;
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
    /** 单阶段 Wiki 维护提示词，与后端 UpdateBlackboardConfigRequest.wiki_prompt 对齐 */
    wiki_prompt?: string;
    /** Wiki 对话执行器，空字符串表示清空回退到默认值 */
    wiki_chat_executor?: string;
  },
): Promise<void> {
  await api.patch(`/api/workspaces/${workspaceId}/blackboard`, config);
}

/** Wiki 对话响应（来自 POST /api/workspaces/{workspaceId}/wiki/chat） */
export interface WikiChatResponse {
  /** 执行器返回的结果文本 */
  content: string;
  /** 本次任务的唯一标识（形如 "wiki-chat-{uuid}"），用于日志追踪 */
  task_id: string;
  /** 是否执行成功 */
  success: boolean;
  /** 执行时长（秒） */
  duration_secs: number;
}

/**
 * POST /api/workspaces/{workspaceId}/wiki/chat：发起一次 Wiki 对话
 *
 * 非流式：等待执行器完成后一次性返回结果。
 * 不创建 Todo、不持久化对话历史。
 *
 * 超时单独设为 5 分钟：执行器（如 claude code）可能需要较长时间完成，
 * 远超 axios 默认的 15 秒。
 */
export async function chatWithWiki(
  workspaceId: number,
  message: string,
  executor?: string,
): Promise<WikiChatResponse> {
  return unwrap(
    await api.post(`/api/workspaces/${workspaceId}/wiki/chat`, {
      message,
      executor,
    }, {
      timeout: 300000,
    }),
  );
}
