import { api, unwrap } from './client';
import type { UsageStatsResponse } from '@/types';

export async function getUsageStats(since?: string, until?: string): Promise<UsageStatsResponse> {
  const params: Record<string, unknown> = {};
  if (since !== undefined) params.since = since;
  if (until !== undefined) params.until = until;
  return unwrap(await api.get('/api/v1/usage-stats', { params }));
}

export async function refreshUsageStats(): Promise<UsageStatsResponse> {
  return unwrap(await api.post('/api/v1/usage-stats/refresh'));
}

export interface UsageStatsSettings {
  auto_usage_stats_enabled: boolean;
  auto_usage_stats_cron: string;
}

export async function getUsageStatsSettings(): Promise<UsageStatsSettings> {
  return unwrap(await api.get('/api/v1/usage-stats/settings'));
}

export async function updateUsageStatsSettings(enabled: boolean, cron: string): Promise<string> {
  return unwrap(await api.put('/api/v1/usage-stats/settings', { enabled, cron }));
}
