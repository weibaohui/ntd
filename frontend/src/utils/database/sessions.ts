import { api, unwrap } from './client';

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
  return unwrap(await api.get('/xyz/sessions', { params }));
}

export async function getSessionStats(): Promise<SessionStats> {
  return unwrap(await api.get('/xyz/sessions/stats'));
}

export async function getSessionDetail(sessionId: string): Promise<SessionDetail> {
  return unwrap(await api.get(`/xyz/sessions/${sessionId}`));
}

export async function deleteSession(sessionId: string): Promise<void> {
  await api.delete(`/xyz/sessions/${sessionId}`);
}
