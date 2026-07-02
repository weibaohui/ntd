import { api, unwrap } from './client';
import type { ExecutorSkills, SkillComparison, SkillVersionUpdate, PaginatedInvocations } from '@/types';

export async function getSkillsList(): Promise<ExecutorSkills[]> {
  return unwrap(await api.get('/api/skills'));
}

export async function getSkillsComparison(): Promise<SkillComparison[]> {
  return unwrap(await api.get('/api/skills/compare'));
}

export async function getSkillVersionUpdates(): Promise<SkillVersionUpdate[]> {
  return unwrap(await api.get('/api/skills/version-update'));
}

export async function syncSkill(sourceExecutor: string, skillName: string, targetExecutors: string[]): Promise<string> {
  return unwrap(await api.post('/api/skills/sync', {
    source_executor: sourceExecutor,
    skill_name: skillName,
    target_executors: targetExecutors,
  }));
}

export async function deleteSkill(executor: string, skillName: string): Promise<string> {
  return unwrap(await api.delete('/api/skills', {
    params: { executor, skill_name: skillName },
  }));
}

export async function getSkillInvocations(params?: { page?: number; limit?: number; skill_name?: string; executor?: string }): Promise<PaginatedInvocations> {
  return unwrap(await api.get('/api/skills/invocations', { params }));
}

export async function recordSkillInvocation(data: { skill_name: string; executor: string; todo_id: number; status: string; duration_ms?: number }): Promise<number> {
  return unwrap(await api.post('/api/skills/invocations', data));
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
  return unwrap(await api.get('/api/skills/content', {
    params: { executor, skill_name: skillName },
  }));
}

export interface SkillFileContent {
  path: string;
  content: string;
}

export async function getSkillFileContent(executor: string, skillName: string, path: string): Promise<SkillFileContent> {
  return unwrap(await api.get('/api/skills/file', {
    params: { executor, skill_name: skillName, path },
  }));
}

export async function exportSkill(executor: string, skillName: string): Promise<Blob> {
  const response = await api.get('/api/skills/export', {
    params: { executor, skill_name: skillName },
    responseType: 'blob',
  });
  return response.data;
}

export interface ImportResult {
  skill_name: string;
  imported_files: number;
  message: string;
}

export async function importSkill(executor: string, file: File, skillName?: string, flatten?: boolean): Promise<ImportResult> {
  const params: Record<string, string> = { executor };
  if (skillName) params.skill_name = skillName;
  if (flatten !== undefined) params.flatten = String(flatten);

  const response = await api.post('/api/skills/import', await file.arrayBuffer(), {
    params,
    headers: { 'Content-Type': 'application/zip' },
  });
  return response.data.data as ImportResult;
}

// Config APIs

export async function getConfig(): Promise<import('@/types').Config> {
  return unwrap(await api.get('/api/config'));
}

export async function updateConfig(config: import('@/types').Config): Promise<import('@/types').Config> {
  return unwrap(await api.put('/api/config', config));
}

// Executor Config APIs

export async function getExecutors(): Promise<import('@/types').ExecutorConfig[]> {
  return unwrap(await api.get('/api/executors'));
}

export async function updateExecutor(name: string, data: { path?: string; enabled?: boolean; display_name?: string; session_dir?: string }): Promise<import('@/types').ExecutorConfig> {
  return unwrap(await api.put(`/api/executors/${encodeURIComponent(name)}`, data));
}

export async function detectExecutor(name: string): Promise<{ binary_found: boolean; path_resolved: string | null }> {
  return unwrap(await api.post(`/api/executors/${encodeURIComponent(name)}/detect`));
}

export async function repairExecutor(name: string): Promise<{ binary_found: boolean; path_resolved: string | null; path_updated: boolean; old_path: string | null; new_path: string | null }> {
  return unwrap(await api.post(`/api/executors/${encodeURIComponent(name)}/resolve`));
}

export async function testExecutor(name: string): Promise<{ test_passed: boolean; output: string | null; error: string | null }> {
  return unwrap(await api.post(`/api/executors/${encodeURIComponent(name)}/test`));
}

export interface ExecutorBatchDetectResult {
  results: {
    name: string;
    display_name: string;
    binary_found: boolean;
    path_resolved: string | null;
    enabled: boolean;
  }[];
  total: number;
  found_count: number;
}

export async function detectAllExecutors(): Promise<ExecutorBatchDetectResult> {
  return unwrap(await api.post('/api/executors/detect-all'));
}

// Version API
export interface VersionInfo {
  version: string;
  git_sha: string;
  git_describe: string;
}

export async function getVersion(): Promise<VersionInfo> {
  return unwrap(await api.get('/api/version'));
}

/** 从 npm 获取 @weibaohui/nothing-todo 的最新版本号 */
export async function getLatestVersion(): Promise<{ latest: string | null; error?: string }> {
  return unwrap(await api.get('/api/version/latest'));
}

/** 执行 npm 升级并重启服务 */
export async function upgradeVersion(): Promise<{
  upgraded: boolean;
  restarted: boolean;
  npmOutput?: string;
  restartMessage?: string;
}> {
  return unwrap(await api.post('/api/version/upgrade'));
}

// Auto-update settings API
export interface AutoUpdateSettings {
  auto_update_enabled: boolean;
  auto_update_interval: string;
  auto_update_hour: number;
  auto_update_last_check_at: string | null;
}

/** 获取自动更新配置 */
export async function getAutoUpdateSettings(): Promise<AutoUpdateSettings> {
  return unwrap(await api.get('/api/config'));
}

/** 更新自动更新配置 */
export async function updateAutoUpdateSettings(settings: {
  auto_update_enabled?: boolean;
  auto_update_interval?: string;
  auto_update_hour?: number;
}): Promise<void> {
  await api.put('/api/config', settings);
}
