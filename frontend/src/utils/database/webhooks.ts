import { api, unwrap } from './client';

export interface Webhook {
  id: number;
  name: string;
  enabled: boolean;
  /** 绑定的默认 todo（仅 webhook_type = "todo" 时有效） */
  default_todo_id: number | null;
  /** 绑定的 loop（仅 webhook_type = "loop" 时有效） */
  loop_id: number | null;
  /** 类型: "todo" | "loop" */
  webhook_type: 'todo' | 'loop';
  created_at: string;
  updated_at: string;
}

export interface WebhookRecord {
  id: number;
  webhook_id: number | null;
  webhook_name: string | null;
  method: string;
  path: string;
  query_params: string | null;
  body: string | null;
  content_type: string | null;
  triggered_todo_id: number | null;
  triggered_todo_title: string | null;
  status_code: number | null;
  response_body: string | null;
  created_at: string;
}

export interface WebhookRecordsPage {
  records: WebhookRecord[];
  total: number;
  limit: number;
  offset: number;
}

export async function getWebhooks(): Promise<Webhook[]> {
  return unwrap(await api.get('/api/webhooks'));
}

export async function getWebhook(id: number): Promise<Webhook> {
  return unwrap(await api.get(`/api/webhooks/${id}`));
}

/**
 * 创建 webhook。
 * webhookType = "loop" 时需要传 loopId，defaultTodoId 留空；
 * webhookType = "todo" 时需要传 defaultTodoId，loopId 留空。
 */
export async function createWebhook(
  name: string,
  enabled: boolean,
  webhookType: 'todo' | 'loop' = 'todo',
  defaultTodoId?: number,
  loopId?: number,
): Promise<Webhook> {
  return unwrap(await api.post('/api/webhooks', {
    name,
    enabled,
    webhook_type: webhookType,
    default_todo_id: webhookType === 'todo' ? (defaultTodoId ?? null) : null,
    loop_id: webhookType === 'loop' ? (loopId ?? null) : null,
  }));
}

/**
 * 更新 webhook。
 */
export async function updateWebhook(
  id: number,
  name: string,
  enabled: boolean,
  webhookType: 'todo' | 'loop' = 'todo',
  defaultTodoId?: number,
  loopId?: number,
): Promise<Webhook> {
  return unwrap(await api.put(`/api/webhooks/${id}`, {
    name,
    enabled,
    webhook_type: webhookType,
    default_todo_id: webhookType === 'todo' ? (defaultTodoId ?? null) : null,
    loop_id: webhookType === 'loop' ? (loopId ?? null) : null,
  }));
}

export async function deleteWebhook(id: number): Promise<void> {
  await api.delete(`/api/webhooks/${id}`);
}

export async function getWebhookRecords(params?: { limit?: number; offset?: number }): Promise<WebhookRecordsPage> {
  return unwrap(await api.get('/api/webhook-records', { params }));
}

export async function getWebhookRecord(id: number): Promise<WebhookRecord> {
  return unwrap(await api.get(`/api/webhook-records/${id}`));
}
