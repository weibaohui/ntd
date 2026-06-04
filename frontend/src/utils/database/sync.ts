import { api, unwrap } from './client';

// ============ Types ============

export interface CloudConfig {
  server_url: string;
  token?: string;
  device_id?: number;
  last_sync_at?: string;
  default_conflict_mode: 'overwrite' | 'skip' | 'rename';
}

export interface DeviceResponse {
  id: number;
  device_name: string;
  last_seen_at?: string;
  created_at?: string;
}

export interface SyncStatusResponse {
  connected: boolean;
  authenticated: boolean;
  device_id?: number;
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

// ============ Device APIs ============

export async function cloudCreateDevice(deviceName: string): Promise<DeviceResponse> {
  return unwrap(await api.post('/api/cloud/devices', { device_name: deviceName }));
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
