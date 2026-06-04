import { api, unwrap } from './client';

// ============ Types ============

export interface CloudConfig {
  server_url: string;
  sync_token?: string;
  has_token?: boolean;
  last_sync_at?: string;
  default_conflict_mode: 'overwrite' | 'skip' | 'rename';
}

export interface SyncStatusResponse {
  connected: boolean;
  authenticated: boolean;
  last_sync_at?: string;
  server_url: string;
}

export interface SyncRecord {
  id: number;
  direction: 'push' | 'pull';
  conflict_mode: string;
  status: 'success' | 'failed' | 'dry_run';
  data_type: string;
  details?: string;
  error_message?: string;
  created_at?: string;
}

// ============ Sync Status APIs ============

export async function getCloudSyncStatus(): Promise<SyncStatusResponse> {
  return unwrap(await api.get('/api/cloud/sync/status'));
}

// ============ Config APIs ============

export async function getCloudConfig(): Promise<CloudConfig> {
  return unwrap(await api.get('/api/cloud/config'));
}

export async function saveCloudConfig(config: Partial<CloudConfig>): Promise<void> {
  return unwrap(await api.post('/api/cloud/config', config));
}

// ============ Sync Records APIs ============

export async function getSyncRecords(params?: { limit?: number; offset?: number }): Promise<SyncRecord[]> {
  return unwrap(await api.get('/api/cloud/sync/records', { params }));
}

// ============ Sync APIs ============

export interface SyncResult {
  success: boolean;
  direction: string;
  conflict_mode: string;
  dry_run: boolean;
  pushed_count: number;
  pulled_count: number;
  conflicts_count: number;
  errors: string[];
}

export async function syncPush(params?: { conflict_mode?: string; dry_run?: boolean }): Promise<SyncResult> {
  return unwrap(await api.get('/api/cloud/sync/push', { params }));
}

export async function syncPull(params?: { conflict_mode?: string; dry_run?: boolean }): Promise<SyncResult> {
  return unwrap(await api.get('/api/cloud/sync/pull', { params }));
}
