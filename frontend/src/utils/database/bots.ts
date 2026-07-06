import { api, unwrap } from './client';

// Agent Bot APIs
export interface AgentBot {
  id: number;
  bot_type: string;
  bot_name: string;
  app_id: string;
  bot_open_id?: string;
  domain?: string;
  enabled: boolean;
  config: string;
  created_at: string;
  /** Bot 所属的工作空间 ID */
  workspace_id: number;
}

// ============================================================================
// Workspace 斜杠命令类型（阶段7）
// ============================================================================

export interface WorkspaceSlashCommand {
  id: number;
  workspace_id: number;
  slash_command: string;
  command_type: 'todo' | 'loop';
  todo_id: number;
  loop_id: number | null;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface CreateWorkspaceSlashCommandParams {
  slash_command: string;
  command_type?: 'todo' | 'loop';
  todo_id: number;
  loop_id?: number | null;
  enabled?: boolean;
}

export interface UpdateWorkspaceSlashCommandParams {
  slash_command?: string;
  command_type?: 'todo' | 'loop';
  todo_id?: number;
  loop_id?: number | null;
  enabled?: boolean;
}

// ============================================================================
// Workspace 设置类型（阶段7）
// ============================================================================

export interface WorkspaceSettings {
  workspace_id: number;
  default_response_type: 'todo' | 'loop' | 'executor';
  default_response_todo_id: number | null;
  default_response_loop_id: number | null;
  default_response_executor: string | null;
  updated_at: string | null;
}

export interface UpdateWorkspaceSettingsParams {
  default_response_type?: 'todo' | 'loop' | 'executor';
  default_response_todo_id?: number;
  default_response_loop_id?: number;
  default_response_executor?: string;
}

// ============================================================================
// Workspace API 函数（阶段8）
// ============================================================================

/** 获取工作空间的斜杠命令列表 */
export async function getWorkspaceSlashCommands(workspaceId: number): Promise<WorkspaceSlashCommand[]> {
  return unwrap(await api.get(`/api/workspace/${workspaceId}/slash-commands`));
}

/** 创建工作空间的斜杠命令 */
export async function createWorkspaceSlashCommand(
  workspaceId: number,
  params: CreateWorkspaceSlashCommandParams,
): Promise<{ id: number }> {
  return unwrap(await api.post(`/api/workspace/${workspaceId}/slash-commands`, params));
}

/** 更新工作空间的斜杠命令 */
export async function updateWorkspaceSlashCommand(
  workspaceId: number,
  cmdId: number,
  params: UpdateWorkspaceSlashCommandParams,
): Promise<void> {
  await api.put(`/api/workspace/${workspaceId}/slash-commands/${cmdId}`, params);
}

/** 删除工作空间的斜杠命令 */
export async function deleteWorkspaceSlashCommand(workspaceId: number, cmdId: number): Promise<void> {
  await api.delete(`/api/workspace/${workspaceId}/slash-commands/${cmdId}`);
}

/** 获取工作空间的设置 */
export async function getWorkspaceSettings(workspaceId: number): Promise<WorkspaceSettings> {
  return unwrap(await api.get(`/api/workspace/${workspaceId}/settings`));
}

/** 更新工作空间的设置 */
export async function updateWorkspaceSettings(
  workspaceId: number,
  params: UpdateWorkspaceSettingsParams,
): Promise<void> {
  await api.put(`/api/workspace/${workspaceId}/settings`, params);
}

/** 将 Bot 移动到另一个工作空间（阶段6级联） */
export async function moveBotToWorkspace(botId: number, workspaceId: number): Promise<void> {
  await api.put(`/api/agent-bots/${botId}/workspace`, { workspace_id: workspaceId });
}

export interface FeishuBeginResponse {
  device_code: string;
  qr_url: string;
  user_code: string;
  interval: number;
  expire_in: number;
}

export interface FeishuPollResponse {
  success: boolean;
  app_id?: string;
  app_secret?: string;
  domain?: string;
  open_id?: string;
  bot_name?: string;
  bot_id?: number;
  error?: string;
}

export type FeishuPushLevel = 'disabled' | 'result_only' | 'all';

export interface FeishuPushStatus {
  bot_id: number;
  push_level: FeishuPushLevel;
  p2p_receive_id: string;
  group_chat_id: string;
  receive_id_type: string;
  p2p_response_enabled: boolean;
  group_response_enabled: boolean;
  p2p_debounce_secs: number;
  group_debounce_secs: number;
}

export interface UpdateFeishuPushParams {
  botId: number;
  pushLevel?: FeishuPushLevel;
  p2pReceiveId?: string;
  groupChatId?: string;
  receiveIdType?: string;
  p2pResponseEnabled?: boolean;
  groupResponseEnabled?: boolean;
  p2pDebounceSecs?: number;
  groupDebounceSecs?: number;
}

export interface FeishuSenderItem {
  sender_open_id: string;
  sender_type: string | null;
  sender_nickname: string | null;
  count: number;
}

export interface WhitelistEntry {
  id: number;
  bot_id: number;
  sender_open_id: string;
  sender_name: string | null;
  created_at: string | null;
}

export async function getAgentBots(): Promise<AgentBot[]> {
  return unwrap(await api.get('/api/agent-bots'));
}

export async function deleteAgentBot(id: number): Promise<void> {
  await api.delete(`/api/agent-bots/${id}`);
}

export async function updateAgentBotConfig(id: number, config: string): Promise<void> {
  await api.put(`/api/agent-bots/${id}/config`, { config });
}

export async function feishuInit(): Promise<{ supported: boolean; auth_methods: string[] }> {
  return unwrap(await api.post('/api/agent-bots/feishu/init'));
}

export async function feishuBegin(): Promise<FeishuBeginResponse> {
  return unwrap(await api.post('/api/agent-bots/feishu/begin'));
}

/**
 * 通过 SSE 方式轮询飞书设备授权，支持页面关闭后继续执行
 * @param device_code 飞书设备码，从 feishuBegin 获取
 * @param interval 轮询间隔（秒），默认 5
 * @param expire_in 过期时间（秒），默认 1800
 * @param onMessage 授权结果回调，接收 FeishuPollResponse
 * @param onError 错误回调，接收错误信息字符串
 * @param workspaceId 创建 bot 时归属的工作空间 ID
 * @returns EventSource 实例，调用方负责管理其生命周期（关闭连接）
 */
export function feishuPollSSE(
  device_code: string,
  interval: number = 5,
  expire_in: number = 1800,
  onMessage: (data: FeishuPollResponse) => void,
  onError?: (error: string) => void,
  workspaceId?: number,
): EventSource {
  const params = new URLSearchParams({
    device_code,
    interval: String(interval),
    expire_in: String(expire_in),
  });
  if (workspaceId !== undefined) {
    params.set('workspace_id', String(workspaceId));
  }
  const url = `/api/agent-bots/feishu/poll-stream?${params.toString()}`;
  const eventSource = new EventSource(url);

  eventSource.addEventListener('result', (event) => {
    try {
      const data = JSON.parse(event.data) as FeishuPollResponse;
      onMessage(data);
    } catch (e) {
      onError?.('Failed to parse response');
    } finally {
      eventSource.close();
    }
  });

  // 处理服务端业务错误（fail 事件，避免与 EventSource transport error 混淆）
  eventSource.addEventListener('fail', (e: MessageEvent) => {
    onError?.(e.data as string || 'Unknown error');
    eventSource.close();
  });

  eventSource.addEventListener('ping', () => {
    // 心跳，保持连接
  });

  // EventSource transport error（网络断开等）
  eventSource.addEventListener('error', () => {
    // EventSource 的 error 事件不携带自定义 data，直接关闭连接
    eventSource.close();
  });

  return eventSource;
}

export async function getFeishuPush(): Promise<FeishuPushStatus[]> {
  return unwrap(await api.get('/api/agent-bots/feishu/push'));
}

export async function updateFeishuPush(params: UpdateFeishuPushParams): Promise<FeishuPushStatus> {
  return unwrap(await api.put('/api/agent-bots/feishu/push', {
    bot_id: params.botId,
    push_level: params.pushLevel,
    p2p_receive_id: params.p2pReceiveId,
    group_chat_id: params.groupChatId,
    receive_id_type: params.receiveIdType,
    p2p_response_enabled: params.p2pResponseEnabled,
    group_response_enabled: params.groupResponseEnabled,
    p2p_debounce_secs: params.p2pDebounceSecs,
    group_debounce_secs: params.groupDebounceSecs,
  }));
}

// Feishu History APIs

export async function getFeishuHistoryMessages(params?: {
  chat_id?: string;
  sender_open_id?: string;
  is_history?: boolean;
  page?: number;
  page_size?: number;
}): Promise<import('@/types').FeishuHistoryMessagesPage> {
  return unwrap(await api.get('/api/feishu/history-messages', { params }));
}

export async function getFeishuMessageStats(hours?: number): Promise<import('@/types').FeishuMessageStats> {
  const params = hours !== undefined ? { hours } : undefined;
  return unwrap(await api.get('/api/feishu/message-stats', { params }));
}

export async function getFeishuSenders(): Promise<FeishuSenderItem[]> {
  return unwrap(await api.get('/api/feishu/senders'));
}

export async function getFeishuHistoryChats(): Promise<import('@/types').FeishuHistoryChat[]> {
  return unwrap(await api.get('/api/feishu/history-chats'));
}

// Group Whitelist APIs

export async function getGroupWhitelist(botId: number): Promise<WhitelistEntry[]> {
  return unwrap(await api.get('/api/agent-bots/feishu/group-whitelist', { params: { bot_id: botId } }));
}

export async function addGroupWhitelist(botId: number, senderOpenId: string, senderName?: string): Promise<WhitelistEntry> {
  return unwrap(await api.post('/api/agent-bots/feishu/group-whitelist', {
    bot_id: botId,
    sender_open_id: senderOpenId,
    sender_name: senderName || null,
  }));
}

export async function deleteGroupWhitelist(id: number): Promise<void> {
  await api.delete(`/api/agent-bots/feishu/group-whitelist/${id}`);
}

