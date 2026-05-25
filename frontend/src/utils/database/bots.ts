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

export interface CreateFeishuHistoryChatParams {
  bot_id: number;
  chat_id: string;
  chat_name?: string;
}

export interface UpdateFeishuHistoryChatParams {
  chat_name?: string;
  enabled?: boolean;
  polling_interval_secs?: number;
}

export interface WhitelistEntry {
  id: number;
  bot_id: number;
  sender_open_id: string;
  sender_name: string | null;
  created_at: string | null;
}

export async function getAgentBots(): Promise<AgentBot[]> {
  return unwrap(await api.get('/xyz/agent-bots'));
}

export async function deleteAgentBot(id: number): Promise<void> {
  await api.delete(`/xyz/agent-bots/${id}`);
}

export async function updateAgentBotConfig(id: number, config: string): Promise<void> {
  await api.put(`/xyz/agent-bots/${id}/config`, { config });
}

export async function feishuInit(): Promise<{ supported: boolean; auth_methods: string[] }> {
  return unwrap(await api.post('/xyz/agent-bots/feishu/init'));
}

export async function feishuBegin(): Promise<FeishuBeginResponse> {
  return unwrap(await api.post('/xyz/agent-bots/feishu/begin'));
}

export async function feishuPoll(device_code: string, interval?: number, expire_in?: number): Promise<FeishuPollResponse> {
  return unwrap(await api.post('/xyz/agent-bots/feishu/poll', {
    device_code,
    interval,
    expire_in,
  }));
}

export async function getFeishuPush(): Promise<FeishuPushStatus[]> {
  return unwrap(await api.get('/xyz/agent-bots/feishu/push'));
}

export async function updateFeishuPush(params: UpdateFeishuPushParams): Promise<FeishuPushStatus> {
  return unwrap(await api.put('/xyz/agent-bots/feishu/push', {
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
}): Promise<import('../../types').FeishuHistoryMessagesPage> {
  return unwrap(await api.get('/xyz/feishu/history-messages', { params }));
}

export async function getFeishuMessageStats(hours?: number): Promise<import('../../types').FeishuMessageStats> {
  const params = hours !== undefined ? { hours } : undefined;
  return unwrap(await api.get('/xyz/feishu/message-stats', { params }));
}

export async function getFeishuSenders(): Promise<FeishuSenderItem[]> {
  return unwrap(await api.get('/xyz/feishu/senders'));
}

export async function getFeishuHistoryChats(): Promise<import('../../types').FeishuHistoryChat[]> {
  return unwrap(await api.get('/xyz/feishu/history-chats'));
}

export async function createFeishuHistoryChat(params: CreateFeishuHistoryChatParams): Promise<import('../../types').FeishuHistoryChat> {
  return unwrap(await api.post('/xyz/feishu/history-chats', params));
}

export async function updateFeishuHistoryChat(id: number, params: UpdateFeishuHistoryChatParams): Promise<void> {
  await api.put(`/xyz/feishu/history-chats/${id}`, params);
}

export async function deleteFeishuHistoryChat(id: number): Promise<void> {
  await api.delete(`/xyz/feishu/history-chats/${id}`);
}

// Group Whitelist APIs

export async function getGroupWhitelist(botId: number): Promise<WhitelistEntry[]> {
  return unwrap(await api.get('/xyz/agent-bots/feishu/group-whitelist', { params: { bot_id: botId } }));
}

export async function addGroupWhitelist(botId: number, senderOpenId: string, senderName?: string): Promise<WhitelistEntry> {
  return unwrap(await api.post('/xyz/agent-bots/feishu/group-whitelist', {
    bot_id: botId,
    sender_open_id: senderOpenId,
    sender_name: senderName || null,
  }));
}

export async function deleteGroupWhitelist(id: number): Promise<void> {
  await api.delete(`/xyz/agent-bots/feishu/group-whitelist/${id}`);
}
