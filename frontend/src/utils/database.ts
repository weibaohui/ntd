import axios from 'axios';
import type { Todo, Tag, ExecutionRecord, ExecutionSummary, ExecutionRecordsPage, ExecutionLogsPage, ExecutorSkills, SkillComparison, PaginatedInvocations, FeishuHistoryMessagesPage, FeishuHistoryChat, FeishuMessageStats, TodoTemplate, CustomTemplateStatus } from '../types';

interface ApiResp<T> {
  code: number;
  data: T | null;
  message: string;
}

export async function checkBackendHealth(): Promise<boolean> {
  try {
    const res = await api.get('/health', { timeout: 3000 });
    return res.status === 200;
  } catch {
    return false;
  }
}

const api = axios.create({
  baseURL: '',
  headers: { 'Content-Type': 'application/json' },
});

api.interceptors.response.use(
  (res) => {
    // Skip code check for non-object responses (blob, text, arraybuffer, etc.)
    if (typeof res.data !== 'object' || res.data === null || res.data instanceof Blob) {
      return res;
    }
    const body = res.data as ApiResp<unknown>;
    if (body && body.code !== 0) {
      return Promise.reject(new Error(body.message || `Error ${body.code}`));
    }
    return res;
  },
  (error) => {
    if (error.response?.data?.message) {
      return Promise.reject(new Error(error.response.data.message));
    }
    return Promise.reject(error);
  },
);

function unwrap<T>(res: { data: ApiResp<T> }): T {
  if (res.data.data === null || res.data.data === undefined) {
    throw new Error(res.data.message || 'API 返回数据为空');
  }
  return res.data.data;
}

// Todo APIs

export async function getAllTodos(): Promise<Todo[]> {
  return unwrap(await api.get<ApiResp<Todo[]>>('/xyz/todos'));
}

export async function createTodo(title: string, prompt: string = '', tagIds: number[] = []): Promise<Todo> {
  return unwrap(await api.post<ApiResp<Todo>>('/xyz/todos', { title, prompt, tag_ids: tagIds }));
}

export async function updateTodo(
  id: number,
  title: string,
  prompt: string,
  status: string,
  executor?: string,
  scheduler_enabled?: boolean,
  scheduler_config?: string | null,
  workspace?: string | null,
  worktree_enabled?: boolean,
): Promise<Todo> {
  const body: Record<string, unknown> = { title, prompt, status };
  if (executor !== undefined) body.executor = executor;
  if (scheduler_enabled !== undefined) body.scheduler_enabled = scheduler_enabled;
  if (scheduler_config !== undefined) body.scheduler_config = scheduler_config;
  if (workspace !== undefined) body.workspace = workspace;
  if (worktree_enabled !== undefined) body.worktree_enabled = worktree_enabled;

  return unwrap(await api.put<ApiResp<Todo>>(`/xyz/todos/${id}`, body));
}

export async function deleteTodo(id: number): Promise<void> {
  await api.delete(`/xyz/todos/${id}`);
}

export async function forceUpdateTodoStatus(id: number, status: string): Promise<Todo> {
  return unwrap(await api.put<ApiResp<Todo>>(`/xyz/todos/${id}/force-status`, { status }));
}

export async function updateTodoTags(todoId: number, tagIds: number[]): Promise<void> {
  await api.put(`/xyz/todos/${todoId}/tags`, { tag_ids: tagIds });
}

// Tag APIs

export async function getAllTags(): Promise<Tag[]> {
  return unwrap(await api.get<ApiResp<Tag[]>>('/xyz/tags'));
}

export async function createTag(name: string, color: string): Promise<Tag> {
  return unwrap(await api.post<ApiResp<Tag>>('/xyz/tags', { name, color }));
}

export async function deleteTag(id: number): Promise<void> {
  await api.delete(`/xyz/tags/${id}`);
}

// Todo Template APIs

export async function getTodoTemplates(): Promise<TodoTemplate[]> {
  return unwrap(await api.get<ApiResp<TodoTemplate[]>>('/xyz/todo-templates'));
}

export async function createTodoTemplate(title: string, prompt: string | null, category: string, sort_order?: number): Promise<TodoTemplate> {
  return unwrap(await api.post<ApiResp<TodoTemplate>>('/xyz/todo-templates', { title, prompt, category, sort_order }));
}

export async function updateTodoTemplate(id: number, title?: string, prompt?: string | null, category?: string, sort_order?: number): Promise<TodoTemplate> {
  const body: Record<string, unknown> = {};
  if (title !== undefined) body.title = title;
  if (prompt !== undefined) body.prompt = prompt;
  if (category !== undefined) body.category = category;
  if (sort_order !== undefined) body.sort_order = sort_order;
  return unwrap(await api.put<ApiResp<TodoTemplate>>(`/xyz/todo-templates/${id}`, body));
}

export async function deleteTodoTemplate(id: number): Promise<void> {
  await api.delete(`/xyz/todo-templates/${id}`);
}

export async function copyTodoTemplate(id: number): Promise<TodoTemplate> {
  return unwrap(await api.post<ApiResp<TodoTemplate>>(`/xyz/todo-templates/${id}/copy`, {}));
}

// Custom Template APIs (remote URL subscription)

export async function getCustomTemplateStatus(): Promise<CustomTemplateStatus> {
  return unwrap(await api.get<ApiResp<CustomTemplateStatus>>('/xyz/custom-templates/status'));
}

export async function subscribeCustomTemplate(url: string): Promise<CustomTemplateStatus> {
  return unwrap(await api.post<ApiResp<CustomTemplateStatus>>('/xyz/custom-templates/subscribe', { url }));
}

export async function unsubscribeCustomTemplate(): Promise<void> {
  await api.post('/xyz/custom-templates/unsubscribe', {});
}

export async function syncCustomTemplate(): Promise<CustomTemplateStatus> {
  return unwrap(await api.post<ApiResp<CustomTemplateStatus>>('/xyz/custom-templates/sync', {}));
}

export async function updateCustomTemplateAutoSync(enabled: boolean, cron: string): Promise<void> {
  await api.put('/xyz/custom-templates/auto-sync', { enabled, cron });
}

// Project Directory APIs

export interface ProjectDirectory {
  id: number;
  path: string;
  name: string | null;
  created_at: string;
  updated_at: string;
}

export async function getProjectDirectories(): Promise<ProjectDirectory[]> {
  return unwrap(await api.get<ApiResp<ProjectDirectory[]>>('/xyz/project-directories'));
}

export async function createProjectDirectory(path: string, name?: string): Promise<ProjectDirectory> {
  return unwrap(await api.post<ApiResp<ProjectDirectory>>('/xyz/project-directories', { path, name }));
}

export async function updateProjectDirectory(id: number, name?: string): Promise<void> {
  await api.put(`/xyz/project-directories/${id}`, { name });
}

export async function deleteProjectDirectory(id: number): Promise<void> {
  await api.delete(`/xyz/project-directories/${id}`);
}

// Execution APIs

export async function getExecutionRecords(todoId: number, page?: number, limit?: number, status?: string): Promise<ExecutionRecordsPage> {
  const params: Record<string, unknown> = { todo_id: todoId };
  if (page !== undefined) params.page = page;
  if (limit !== undefined) params.limit = limit;
  if (status !== undefined) params.status = status;
  return unwrap(await api.get<ApiResp<ExecutionRecordsPage>>(`/xyz/execution-records`, { params }));
}

export async function getExecutionRecord(recordId: number): Promise<ExecutionRecord> {
  return unwrap(await api.get<ApiResp<ExecutionRecord>>(`/xyz/execution-records/${recordId}`));
}

export async function getExecutionLogs(recordId: number, page?: number, perPage?: number): Promise<ExecutionLogsPage> {
  const params: Record<string, unknown> = {};
  if (page !== undefined) params.page = page;
  if (perPage !== undefined) params.per_page = perPage;
  return unwrap(await api.get<ApiResp<ExecutionLogsPage>>(`/xyz/execution-records/${recordId}/logs`, { params }));
}

export async function getExecutionRecordsBySession(sessionId: string): Promise<ExecutionRecord[]> {
  return unwrap(await api.get<ApiResp<ExecutionRecord[]>>(`/xyz/execution-records/session/${encodeURIComponent(sessionId)}`));
}

export async function executeTodo(todoId: number, message: string, executor?: string): Promise<{ task_id: string }> {
  return unwrap(await api.post<ApiResp<{ task_id: string }>>('/xyz/execute', { todo_id: todoId, message, executor }));
}

export async function getExecutionSummary(todoId: number): Promise<ExecutionSummary> {
  return unwrap(await api.get<ApiResp<ExecutionSummary>>(`/xyz/todos/${todoId}/summary`));
}

export async function getRecentCompletedTodos(hours?: number): Promise<import('../types').RecentCompletedTodo[]> {
  const params = hours !== undefined ? { hours } : undefined;
  return unwrap(await api.get<ApiResp<import('../types').RecentCompletedTodo[]>>('/xyz/todos/recent-completed', { params }));
}

export async function getDashboardStats(hours?: number): Promise<import('../types').DashboardStats> {
  const params = hours !== undefined ? { hours } : undefined;
  return unwrap(await api.get<ApiResp<import('../types').DashboardStats>>('/xyz/dashboard-stats', { params }));
}

export async function stopExecution(recordId: number): Promise<void> {
  await api.post('/xyz/execute/stop', { record_id: recordId });
}

export async function forceFailExecution(recordId: number): Promise<void> {
  await api.post('/xyz/execute/force-fail', { record_id: recordId });
}

export async function getRunningExecutionRecords(): Promise<ExecutionRecord[]> {
  return unwrap(await api.get<ApiResp<ExecutionRecord[]>>('/xyz/execution-records/running'));
}

export async function resumeExecutionRecord(recordId: number, message?: string): Promise<{ task_id: string; record_id: number }> {
  return unwrap(await api.post<ApiResp<{ task_id: string; record_id: number }>>(`/xyz/execution-records/${recordId}/resume`, { message }));
}

// Smart Create API

export interface SmartCreateResult {
  task_id: string;
  record_id: number;
  todo_id: number;
  todo_title: string;
}

export async function smartCreate(content: string): Promise<SmartCreateResult> {
  return unwrap(await api.post<ApiResp<SmartCreateResult>>('/xyz/smart-create', { content }));
}

// Scheduler APIs

export async function updateScheduler(
  id: number,
  scheduler_enabled: boolean,
  scheduler_config: string | null,
): Promise<Todo> {
  return unwrap(await api.put<ApiResp<Todo>>(`/xyz/todos/${id}/scheduler`, { scheduler_enabled, scheduler_config }));
}

export async function getSchedulerTodos(): Promise<Todo[]> {
  return unwrap(await api.get<ApiResp<Todo[]>>('/xyz/scheduler/todos'));
}

export async function getRunningTodos(): Promise<Todo[]> {
  return unwrap(await api.get<ApiResp<Todo[]>>('/xyz/running-todos'));
}

// Backup APIs

export async function exportBackup(): Promise<string> {
  const res = await api.get('/xyz/backup/export', {
    headers: { 'Accept': 'application/x-yaml' },
    responseType: 'text',
    transformResponse: [(data) => data],
  });
  if (typeof res.data === 'string') return res.data;
  return JSON.stringify(res.data);
}

export async function importBackup(yamlContent: string): Promise<string> {
  return unwrap(await api.post<ApiResp<string>>('/xyz/backup/import', yamlContent, {
    headers: { 'Content-Type': 'application/x-yaml' },
  }));
}

export async function mergeBackup(tags: { name: string; color: string }[], todos: { title: string; prompt: string; status: string; executor?: string; scheduler_enabled: boolean; scheduler_config?: string; tag_names: string[]; workspace?: string }[]): Promise<string> {
  return unwrap(await api.post<ApiResp<string>>('/xyz/backup/merge', { tags, todos }));
}

export async function exportSelectedBackup(todoIds: number[]): Promise<string> {
  const res = await api.post('/xyz/backup/export-selected', { todo_ids: todoIds }, {
    headers: { 'Accept': 'application/x-yaml' },
    responseType: 'text',
    transformResponse: [(data: unknown) => data],
  });
  if (typeof res.data === 'string') return res.data;
  return JSON.stringify(res.data);
}

export async function triggerLocalBackup(): Promise<string> {
  return unwrap(await api.post<ApiResp<string>>('/xyz/backup/database/trigger'));
}

export async function optimizeDatabase(): Promise<string> {
  return unwrap(await api.post<ApiResp<string>>('/xyz/backup/database/optimize'));
}

export async function getDatabaseBackupStatus(): Promise<{
  auto_backup_enabled: boolean;
  auto_backup_cron: string;
  auto_backup_max_files: number;
  last_backup: string | null;
  files: { name: string; size: number; created_at: string }[];
}> {
  return unwrap(await api.get<ApiResp<{
    auto_backup_enabled: boolean;
    auto_backup_cron: string;
    auto_backup_max_files: number;
    last_backup: string | null;
    files: { name: string; size: number; created_at: string }[];
  }>>('/xyz/backup/database/status'));
}

export async function updateAutoBackup(enabled: boolean, cron: string, maxFiles?: number): Promise<string> {
  const body: Record<string, unknown> = { enabled, cron };
  if (maxFiles !== undefined) {
    body.max_files = maxFiles;
  }
  return unwrap(await api.put<ApiResp<string>>('/xyz/backup/database/auto', body));
}

export async function deleteBackupFile(filename: string): Promise<string> {
  return unwrap(await api.delete<ApiResp<string>>('/xyz/backup/database/file', { data: { filename } }));
}

export function downloadBackupFileUrl(filename: string): string {
  return `/xyz/backup/database/file?filename=${encodeURIComponent(filename)}`;
}

// Config APIs

export async function getConfig(): Promise<import('../types').Config> {
  return unwrap(await api.get<ApiResp<import('../types').Config>>('/xyz/config'));
}

export async function updateConfig(config: import('../types').Config): Promise<import('../types').Config> {
  return unwrap(await api.put<ApiResp<import('../types').Config>>('/xyz/config', config));
}

// Executor Config APIs

export async function getExecutors(): Promise<import('../types').ExecutorConfig[]> {
  return unwrap(await api.get<ApiResp<import('../types').ExecutorConfig[]>>('/xyz/executors'));
}

export async function updateExecutor(name: string, data: { path?: string; enabled?: boolean; display_name?: string; session_dir?: string }): Promise<import('../types').ExecutorConfig> {
  return unwrap(await api.put<ApiResp<import('../types').ExecutorConfig>>(`/xyz/executors/${encodeURIComponent(name)}`, data));
}

export async function detectExecutor(name: string): Promise<{ binary_found: boolean; path_resolved: string | null }> {
  return unwrap(await api.post<ApiResp<{ binary_found: boolean; path_resolved: string | null }>>(`/xyz/executors/${encodeURIComponent(name)}/detect`));
}

export async function testExecutor(name: string): Promise<{ test_passed: boolean; output: string | null; error: string | null }> {
  const result = unwrap(await api.post<ApiResp<{ test_passed: boolean; output: string | null; error: string | null }>>(`/xyz/executors/${encodeURIComponent(name)}/test`));
  return result;
}

// Skills APIs

export async function getSkillsList(): Promise<ExecutorSkills[]> {
  return unwrap(await api.get<ApiResp<ExecutorSkills[]>>('/xyz/skills'));
}

export async function getSkillsComparison(): Promise<SkillComparison[]> {
  return unwrap(await api.get<ApiResp<SkillComparison[]>>('/xyz/skills/compare'));
}

export async function syncSkill(sourceExecutor: string, skillName: string, targetExecutors: string[]): Promise<string> {
  return unwrap(await api.post<ApiResp<string>>('/xyz/skills/sync', {
    source_executor: sourceExecutor,
    skill_name: skillName,
    target_executors: targetExecutors,
  }));
}

export async function getSkillInvocations(params?: { page?: number; limit?: number; skill_name?: string; executor?: string }): Promise<PaginatedInvocations> {
  return unwrap(await api.get<ApiResp<PaginatedInvocations>>('/xyz/skills/invocations', { params }));
}

export async function recordSkillInvocation(data: { skill_name: string; executor: string; todo_id: number; status: string; duration_ms?: number }): Promise<number> {
  return unwrap(await api.post<ApiResp<number>>('/xyz/skills/invocations', data));
}

// Skill content & files
export interface SkillFileInfo {
  path: string;
  size: number;
  modified_at: string;
}

export interface SkillContent {
  skill_name: string;
  executor: string;
  content: string;
  files: SkillFileInfo[];
}

export async function getSkillContent(executor: string, skillName: string): Promise<SkillContent> {
  return unwrap(await api.get<ApiResp<SkillContent>>('/xyz/skills/content', {
    params: { executor, skill_name: skillName },
  }));
}

// Export skill as .zip (returns blob)
export async function exportSkill(executor: string, skillName: string): Promise<Blob> {
  const response = await api.get('/xyz/skills/export', {
    params: { executor, skill_name: skillName },
    responseType: 'blob',
  });
  return response.data;
}

// Import skill from file
export interface ImportResult {
  skill_name: string;
  imported_files: number;
  message: string;
}

export async function importSkill(executor: string, file: File, skillName?: string, flatten?: boolean): Promise<ImportResult> {
  const params: Record<string, string> = { executor };
  if (skillName) params.skill_name = skillName;
  if (flatten !== undefined) params.flatten = String(flatten);

  const response = await api.post<ApiResp<ImportResult>>('/xyz/skills/import', await file.arrayBuffer(), {
    params,
    headers: { 'Content-Type': 'application/zip' },
  });
  return response.data.data as ImportResult;
}

// Version API
export interface VersionInfo {
  version: string;
  git_sha: string;
  git_describe: string;
}

export async function getVersion(): Promise<VersionInfo> {
  return unwrap(await api.get<ApiResp<VersionInfo>>('/xyz/version'));
}

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

export async function getAgentBots(): Promise<AgentBot[]> {
  return unwrap(await api.get<ApiResp<AgentBot[]>>('/xyz/agent-bots'));
}

export async function deleteAgentBot(id: number): Promise<void> {
  await api.delete(`/xyz/agent-bots/${id}`);
}

export async function updateAgentBotConfig(id: number, config: string): Promise<void> {
  await api.put(`/xyz/agent-bots/${id}/config`, { config });
}

export async function feishuInit(): Promise<{ supported: boolean; auth_methods: string[] }> {
  return unwrap(await api.post<ApiResp<{ supported: boolean; auth_methods: string[] }>>('/xyz/agent-bots/feishu/init'));
}

export async function feishuBegin(): Promise<FeishuBeginResponse> {
  return unwrap(await api.post<ApiResp<FeishuBeginResponse>>('/xyz/agent-bots/feishu/begin'));
}

export async function feishuPoll(device_code: string, interval?: number, expire_in?: number): Promise<FeishuPollResponse> {
  return unwrap(await api.post<ApiResp<FeishuPollResponse>>('/xyz/agent-bots/feishu/poll', {
    device_code,
    interval,
    expire_in,
  }));
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

export async function getFeishuPush(): Promise<FeishuPushStatus[]> {
  return unwrap(await api.get<ApiResp<FeishuPushStatus[]>>('/xyz/agent-bots/feishu/push'));
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

export async function updateFeishuPush(params: UpdateFeishuPushParams): Promise<FeishuPushStatus> {
  return unwrap(await api.put<ApiResp<FeishuPushStatus>>('/xyz/agent-bots/feishu/push', {
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
}): Promise<FeishuHistoryMessagesPage> {
  return unwrap(await api.get<ApiResp<FeishuHistoryMessagesPage>>('/xyz/feishu/history-messages', { params }));
}

export async function getFeishuMessageStats(hours?: number): Promise<FeishuMessageStats> {
  const params = hours !== undefined ? { hours } : undefined;
  return unwrap(await api.get<ApiResp<FeishuMessageStats>>('/xyz/feishu/message-stats', { params }));
}

export interface FeishuSenderItem {
  sender_open_id: string;
  sender_type: string | null;
  sender_nickname: string | null;
  count: number;
}

export async function getFeishuSenders(): Promise<FeishuSenderItem[]> {
  return unwrap(await api.get<ApiResp<FeishuSenderItem[]>>('/xyz/feishu/senders'));
}

export async function getFeishuHistoryChats(): Promise<FeishuHistoryChat[]> {
  return unwrap(await api.get<ApiResp<FeishuHistoryChat[]>>('/xyz/feishu/history-chats'));
}

export interface CreateFeishuHistoryChatParams {
  bot_id: number;
  chat_id: string;
  chat_name?: string;
}

export async function createFeishuHistoryChat(params: CreateFeishuHistoryChatParams): Promise<FeishuHistoryChat> {
  return unwrap(await api.post<ApiResp<FeishuHistoryChat>>('/xyz/feishu/history-chats', params));
}

export interface UpdateFeishuHistoryChatParams {
  chat_name?: string;
  enabled?: boolean;
  polling_interval_secs?: number;
}

export async function updateFeishuHistoryChat(id: number, params: UpdateFeishuHistoryChatParams): Promise<void> {
  await api.put(`/xyz/feishu/history-chats/${id}`, params);
}

export async function deleteFeishuHistoryChat(id: number): Promise<void> {
  await api.delete(`/xyz/feishu/history-chats/${id}`);
}

// Group Whitelist APIs

export interface WhitelistEntry {
  id: number;
  bot_id: number;
  sender_open_id: string;
  sender_name: string | null;
  created_at: string | null;
}

export async function getGroupWhitelist(botId: number): Promise<WhitelistEntry[]> {
  return unwrap(await api.get<ApiResp<WhitelistEntry[]>>('/xyz/agent-bots/feishu/group-whitelist', { params: { bot_id: botId } }));
}

export async function addGroupWhitelist(botId: number, senderOpenId: string, senderName?: string): Promise<WhitelistEntry> {
  return unwrap(await api.post<ApiResp<WhitelistEntry>>('/xyz/agent-bots/feishu/group-whitelist', {
    bot_id: botId,
    sender_open_id: senderOpenId,
    sender_name: senderName || null,
  }));
}

export async function deleteGroupWhitelist(id: number): Promise<void> {
  await api.delete(`/xyz/agent-bots/feishu/group-whitelist/${id}`);
}

// ─── Session APIs ──────────────────────────────────────────

export interface SessionInfo {
  session_id: string;
  source: string;
  project_path: string;
  status: string;
  executor: string;
  model: string;
  git_branch: string | null;
  message_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
  first_prompt: string | null;
  created_at: string | null;
  last_active_at: string | null;
  file_size: number;
  version: string | null;
  subagent_count: number;
}

export interface SessionListResponse {
  sessions: SessionInfo[];
  total: number;
  page: number;
  page_size: number;
}

export interface SessionStats {
  total_sessions: number;
  active_sessions: number;
  today_sessions: number;
  total_input_tokens: number;
  total_output_tokens: number;
  by_source: Record<string, number>;
  by_executor: Record<string, number>;
  by_project: Record<string, number>;
}

export interface SessionMessage {
  role: string;
  content_preview: string;
  model: string | null;
  input_tokens: number | null;
  output_tokens: number | null;
  timestamp: string | null;
  stop_reason: string | null;
}

export interface SubAgentInfo {
  agent_type: string;
  description: string;
  message_count: number;
}

export interface SessionDetail {
  info: SessionInfo;
  messages: SessionMessage[];
  subagents: SubAgentInfo[];
}

export async function listSessions(params: {
  page?: number;
  page_size?: number;
  status?: string;
  source?: string;
  executor?: string;
  project?: string;
  search?: string;
}): Promise<SessionListResponse> {
  return unwrap(await api.get<ApiResp<SessionListResponse>>('/xyz/sessions', { params }));
}

export async function getSessionStats(): Promise<SessionStats> {
  return unwrap(await api.get<ApiResp<SessionStats>>('/xyz/sessions/stats'));
}

export async function getSessionDetail(sessionId: string): Promise<SessionDetail> {
  return unwrap(await api.get<ApiResp<SessionDetail>>(`/xyz/sessions/${sessionId}`));
}

export async function deleteSession(sessionId: string): Promise<void> {
  await api.delete(`/xyz/sessions/${sessionId}`);
}
