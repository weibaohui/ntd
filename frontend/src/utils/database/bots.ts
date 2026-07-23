import { api, unwrap } from './client';

// Agent Bot APIs
export interface AgentBot {
  id: number;
  bot_type: string;
  bot_name: string;
  app_id: string;
  bot_open_id?: string;
  /** 所有者 open_id（推送目标），扫码/首次私聊自动捕获；仅列表页展示 */
  owner_open_id?: string;
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
// Workspace API 函数 — slash-commands/settings 嵌套在 /api/v1/workspaces/{ws} 下。
// 后端 v1 用复数 workspaces（v0 用过单数 workspace），URL 写复数让拦截器加 v1 前缀。
// ============================================================================

/** 获取工作空间的斜杠命令列表 */
export async function getWorkspaceSlashCommands(workspaceId: number): Promise<WorkspaceSlashCommand[]> {
  return unwrap(await api.get(`/api/v1/workspaces/${workspaceId}/slash-commands`));
}

/** 创建工作空间的斜杠命令 */
export async function createWorkspaceSlashCommand(
  workspaceId: number,
  params: CreateWorkspaceSlashCommandParams,
): Promise<{ id: number }> {
  return unwrap(await api.post(`/api/v1/workspaces/${workspaceId}/slash-commands`, params));
}

/** 更新工作空间的斜杠命令 */
export async function updateWorkspaceSlashCommand(
  workspaceId: number,
  cmdId: number,
  params: UpdateWorkspaceSlashCommandParams,
): Promise<void> {
  await api.put(`/api/v1/workspaces/${workspaceId}/slash-commands/${cmdId}`, params);
}

/** 删除工作空间的斜杠命令 */
export async function deleteWorkspaceSlashCommand(workspaceId: number, cmdId: number): Promise<void> {
  await api.delete(`/api/v1/workspaces/${workspaceId}/slash-commands/${cmdId}`);
}

/** 获取工作空间的设置 */
export async function getWorkspaceSettings(workspaceId: number): Promise<WorkspaceSettings> {
  return unwrap(await api.get(`/api/v1/workspaces/${workspaceId}/settings`));
}

/** 更新工作空间的设置 */
export async function updateWorkspaceSettings(
  workspaceId: number,
  params: UpdateWorkspaceSettingsParams,
): Promise<void> {
  await api.put(`/api/v1/workspaces/${workspaceId}/settings`, params);
}

/** 将 Bot 移动到另一个工作空间（agent-bots 为全局路由，拦截器加 v1 前缀） */
export async function moveBotToWorkspace(botId: number, workspaceId: number): Promise<void> {
  await api.put(`/api/v1/agent-bots/${botId}/workspace`, { workspace_id: workspaceId });
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
  /** 推送目标（所有者 open_id），扫码/首次私聊自动捕获；前端只读展示 */
  owner_open_id?: string;
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
  // 推送目标（owner_open_id）由系统自动捕获，前端不再手动编辑单聊/群聊接收 ID
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
  return unwrap(await api.get('/api/v1/agent-bots'));
}

export async function deleteAgentBot(id: number): Promise<void> {
  await api.delete(`/api/v1/agent-bots/${id}`);
}

export async function updateAgentBotConfig(id: number, config: string): Promise<void> {
  await api.put(`/api/v1/agent-bots/${id}/config`, { config });
}

export async function feishuInit(): Promise<{ supported: boolean; auth_methods: string[] }> {
  return unwrap(await api.post('/api/v1/agent-bots/feishu/init'));
}

export async function feishuBegin(): Promise<FeishuBeginResponse> {
  return unwrap(await api.post('/api/v1/agent-bots/feishu/begin'));
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
  // EventSource 走浏览器原生 fetch，不经 axios 拦截器，手动写 v1 前缀
  const url = `/api/v1/agent-bots/feishu/poll-stream?${params.toString()}`;
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
  return unwrap(await api.get('/api/v1/agent-bots/feishu/push'));
}

export async function updateFeishuPush(params: UpdateFeishuPushParams): Promise<FeishuPushStatus> {
  return unwrap(await api.put('/api/v1/agent-bots/feishu/push', {
    bot_id: params.botId,
    push_level: params.pushLevel,
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
  processed?: boolean;
  chat_type?: string;
  keyword?: string;
  processed_type?: string;
  workspace_id?: number;
  bot_id?: number;
  page?: number;
  page_size?: number;
}): Promise<import('@/types').FeishuHistoryMessagesPage> {
  return unwrap(await api.get('/api/v1/feishu/history-messages', { params }));
}

export async function getFeishuMessageStats(workspaceId?: number, hours?: number): Promise<import('@/types').FeishuMessageStats> {
  const params: Record<string, unknown> = {};
  if (workspaceId !== undefined) params.workspace_id = workspaceId;
  if (hours !== undefined) params.hours = hours;
  return unwrap(await api.get('/api/v1/feishu/message-stats', { params }));
}

export async function getFeishuSenders(): Promise<FeishuSenderItem[]> {
  return unwrap(await api.get('/api/v1/feishu/senders'));
}

export async function getFeishuHistoryChats(botId?: number): Promise<import('@/types').FeishuHistoryChat[]> {
  return unwrap(await api.get('/api/v1/feishu/history-chats', { params: { bot_id: botId } }));
}

/** 新增历史拉取群：用户在前端填写群 chat_id（替代旧的 /sethome 隐式写入 group_chat_id） */
export async function createFeishuHistoryChat(botId: number, chatId: string, chatName?: string): Promise<import('@/types').FeishuHistoryChat> {
  return unwrap(await api.post('/api/v1/feishu/history-chats', { bot_id: botId, chat_id: chatId, chat_name: chatName }));
}

/** 删除历史拉取群 */
export async function deleteFeishuHistoryChat(id: number): Promise<void> {
  await api.delete(`/api/v1/feishu/history-chats/${id}`);
}

// Group Whitelist APIs

export async function getGroupWhitelist(botId: number): Promise<WhitelistEntry[]> {
  return unwrap(await api.get('/api/v1/agent-bots/feishu/group-whitelist', { params: { bot_id: botId } }));
}

export async function addGroupWhitelist(botId: number, senderOpenId: string, senderName?: string): Promise<WhitelistEntry> {
  return unwrap(await api.post('/api/v1/agent-bots/feishu/group-whitelist', {
    bot_id: botId,
    sender_open_id: senderOpenId,
    sender_name: senderName || null,
  }));
}

export async function deleteGroupWhitelist(id: number): Promise<void> {
  await api.delete(`/api/v1/agent-bots/feishu/group-whitelist/${id}`);
}

