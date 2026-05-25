import { api, unwrap } from './client';

export interface Webhook {
  id: number;
  name: string;
  enabled: boolean;
  default_todo_id: number | null;
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
  return unwrap(await api.get('/xyz/webhooks'));
}

export async function getWebhook(id: number): Promise<Webhook> {
  return unwrap(await api.get(`/xyz/webhooks/${id}`));
}

export async function createWebhook(name: string, enabled: boolean, defaultTodoId?: number): Promise<Webhook> {
  return unwrap(await api.post('/xyz/webhooks', {
    name,
    enabled,
    default_todo_id: defaultTodoId ?? null,
  }));
}

export async function updateWebhook(id: number, name: string, enabled: boolean, defaultTodoId?: number): Promise<Webhook> {
  return unwrap(await api.put(`/xyz/webhooks/${id}`, {
    name,
    enabled,
    default_todo_id: defaultTodoId ?? null,
  }));
}

export async function deleteWebhook(id: number): Promise<void> {
  await api.delete(`/xyz/webhooks/${id}`);
}

export async function getWebhookRecords(params?: { limit?: number; offset?: number }): Promise<WebhookRecordsPage> {
  return unwrap(await api.get('/xyz/webhook-records', { params }));
}

export async function getWebhookRecord(id: number): Promise<WebhookRecord> {
  return unwrap(await api.get(`/xyz/webhook-records/${id}`));
}
