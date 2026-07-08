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
  /** Wiki 执行超时（秒），控制 Wiki 任务与 Wiki 对话的最长存活时间 */
  wiki_timeout_secs: number;
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
    /** Wiki 执行超时（秒），后端会钳制到 [60, 3600] 区间 */
    wiki_timeout_secs?: number;
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

/** Wiki 文件删除响应（来自 DELETE /api/workspaces/{workspaceId}/wiki/files/{slug}） */
export interface WikiFileDeleteResponse {
  slug: string;
  /** true=文件存在并已删除；false=文件本就不存在（幂等删除，仍算成功） */
  deleted: boolean;
}

/**
 * DELETE /api/workspaces/{workspaceId}/wiki/files/{slug}：删除指定 topic 文件。
 *
 * 仅限 topic：后端会拒绝删除 log（系统维护）。文件本就不存在时返回 deleted=false，
 * 前端据此区分「真正删了一篇」与「点了但文件已没了」，但两者都视为成功。
 */
export async function deleteWikiFile(workspaceId: number, slug: string): Promise<WikiFileDeleteResponse> {
  return unwrap(
    await api.delete(`/api/workspaces/${workspaceId}/wiki/files/${encodeURIComponent(slug)}`),
  );
}

/**
 * POST /api/workspaces/{workspaceId}/wiki/chat：发起一次 Wiki 对话
 *
 * 非流式：等待执行器完成后一次性返回结果。
 * 不创建 Todo、不持久化对话历史。
 *
 * HTTP 超时随 per-workspace 的 wiki_timeout_secs 动态调整（后端会钳制到 [60,3600]），
 * 并额外加 10 秒缓冲，避免 HTTP 请求在执行器即将完成时先于后端超时失败。
 * 取不到配置时回退到 5 分钟（与历史行为一致）。
 */
export async function chatWithWiki(
  workspaceId: number,
  message: string,
  executor?: string,
): Promise<WikiChatResponse> {
  // 读取 per-workspace 超时配置，推算 HTTP 超时；失败回退默认 300 秒
  const timeoutMs = await resolveWikiChatHttpTimeoutMs(workspaceId);
  return unwrap(
    await api.post(`/api/workspaces/${workspaceId}/wiki/chat`, {
      message,
      executor,
    }, {
      timeout: timeoutMs,
    }),
  );
}

/** 默认 Wiki 执行超时（秒），与后端 DEFAULT_WIKI_TIMEOUT_SECS 保持一致。 */
const DEFAULT_WIKI_TIMEOUT_SECS = 300;
/** HTTP 超时相对后端执行超时的额外缓冲（毫秒），给后端收尾 + 网络往返留余量。 */
const WIKI_CHAT_HTTP_BUFFER_MS = 10_000;

/**
 * 推算 Wiki 对话 HTTP 请求超时（毫秒）。
 *
 * 取黑板配置中的 wiki_timeout_secs（后端已钳制到 [60,3600]），换算成毫秒后加缓冲；
 * 配置缺失/请求失败时回退默认值，保证可用性优先于精确性。
 */
async function resolveWikiChatHttpTimeoutMs(workspaceId: number): Promise<number> {
  try {
    const board = await getBlackboard(workspaceId);
    const secs = board.wiki_timeout_secs ?? DEFAULT_WIKI_TIMEOUT_SECS;
    return secs * 1000 + WIKI_CHAT_HTTP_BUFFER_MS;
  } catch {
    // 配置读取失败不阻断对话：回退默认超时，让用户至少能正常用
    return DEFAULT_WIKI_TIMEOUT_SECS * 1000 + WIKI_CHAT_HTTP_BUFFER_MS;
  }
}
