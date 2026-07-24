import { api, unwrap } from './client';
import type { ExecutorSkills, SkillComparison, SkillVersionUpdate } from '@/types';

export async function getSkillsList(): Promise<ExecutorSkills[]> {
  return unwrap(await api.get('/api/v1/skills'));
}

export async function getSkillsComparison(): Promise<SkillComparison[]> {
  return unwrap(await api.get('/api/v1/skills/compare'));
}

export async function getSkillVersionUpdates(): Promise<SkillVersionUpdate[]> {
  return unwrap(await api.get('/api/v1/skills/version-update'));
}

export async function syncSkill(sourceExecutor: string, skillName: string, targetExecutors: string[]): Promise<string> {
  return unwrap(await api.post('/api/v1/skills/sync', {
    source_executor: sourceExecutor,
    skill_name: skillName,
    target_executors: targetExecutors,
  }));
}

export async function deleteSkill(executor: string, skillName: string): Promise<string> {
  return unwrap(await api.delete('/api/v1/skills', {
    params: { executor, skill_name: skillName },
  }));
}

// 注：「调用追踪」tab 已移除，因此原 getSkillInvocations 随之删除。
// Dashboard 上的「技能调用次数」「成功率」走 db/dashboard.rs 聚合统计，
// 与本页分页接口两条独立路径，POST /api/v1/skills/invocations 仍保留
// 给执行器调用上报用。

// Skill content & files
export interface SkillFileInfo {
  path: string;
  size: number;
  /** 可选——marketplace 的 bundled 技能文件元信息不包含此字段 */
  modified_at?: string;
}

export interface SkillContent {
  skill_name: string;
  executor: string;
  content: string;
  files: SkillFileInfo[];
}

export async function getSkillContent(executor: string, skillName: string): Promise<SkillContent> {
  return unwrap(await api.get('/api/v1/skills/content', {
    params: { executor, skill_name: skillName },
  }));
}

export interface SkillFileContent {
  path: string;
  content: string;
}

export async function getSkillFileContent(executor: string, skillName: string, path: string): Promise<SkillFileContent> {
  return unwrap(await api.get('/api/v1/skills/file', {
    params: { executor, skill_name: skillName, path },
  }));
}

export async function exportSkill(executor: string, skillName: string): Promise<Blob> {
  const response = await api.get('/api/v1/skills/export', {
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

  const response = await api.post('/api/v1/skills/import', await file.arrayBuffer(), {
    params,
    headers: { 'Content-Type': 'application/zip' },
  });
  return response.data.data as ImportResult;
}

// Config APIs

export async function getConfig(): Promise<import('@/types').Config> {
  return unwrap(await api.get('/api/v1/config'));
}

export async function updateConfig(config: import('@/types').Config): Promise<import('@/types').Config> {
  return unwrap(await api.put('/api/v1/config', config));
}

// Executor Config APIs

export async function getExecutors(): Promise<import('@/types').ExecutorConfig[]> {
  return unwrap(await api.get('/api/v1/executors'));
}

export async function updateExecutor(name: string, data: { path?: string; enabled?: boolean; display_name?: string; session_dir?: string; default_model?: string }): Promise<import('@/types').ExecutorConfig> {
  return unwrap(await api.put(`/api/v1/executors/${encodeURIComponent(name)}`, data));
}

/** 拉取执行器支持的模型列表（调其 models 子命令，用作默认模型下拉选项）。 */
export async function getExecutorModels(name: string): Promise<string[]> {
  return unwrap(await api.get(`/api/v1/executors/${encodeURIComponent(name)}/models`));
}

export async function detectExecutor(name: string): Promise<{ binary_found: boolean; path_resolved: string | null }> {
  return unwrap(await api.post(`/api/v1/executors/${encodeURIComponent(name)}/detect`));
}

export async function repairExecutor(name: string): Promise<{ binary_found: boolean; path_resolved: string | null; path_updated: boolean; old_path: string | null; new_path: string | null }> {
  return unwrap(await api.post(`/api/v1/executors/${encodeURIComponent(name)}/resolve`));
}

export async function testExecutor(name: string): Promise<{ test_passed: boolean; output: string | null; error: string | null }> {
  return unwrap(await api.post(`/api/v1/executors/${encodeURIComponent(name)}/test`));
}

/** 获取系统默认执行器 */
export async function getDefaultExecutor(): Promise<import('@/types').ExecutorConfig | null> {
  return unwrap(await api.get('/api/v1/executors/default'));
}

/** 设置指定执行器为系统默认执行器 */
export async function setDefaultExecutor(name: string): Promise<import('@/types').ExecutorConfig> {
  return unwrap(await api.put(`/api/v1/executors/${encodeURIComponent(name)}/default`));
}

// Version API
export interface VersionInfo {
  version: string;
  git_sha: string;
  git_describe: string;
}

export async function getVersion(): Promise<VersionInfo> {
  return unwrap(await api.get('/api/v1/version'));
}

/** 从 npm 获取 @weibaohui/ntd 的最新版本号 */
export async function getLatestVersion(): Promise<{ latest: string | null; error?: string }> {
  return unwrap(await api.get('/api/v1/version/latest'));
}

/** 执行 npm 升级并重启服务 */
export async function upgradeVersion(): Promise<{
  upgraded: boolean;
  restarted: boolean;
  npmOutput?: string;
  restartMessage?: string;
}> {
  return unwrap(await api.post('/api/v1/version/upgrade'));
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
  return unwrap(await api.get('/api/v1/config'));
}

/** 更新自动更新配置 */
export async function updateAutoUpdateSettings(settings: {
  auto_update_enabled?: boolean;
  auto_update_interval?: string;
  auto_update_hour?: number;
}): Promise<void> {
  await api.put('/api/v1/config', settings);
}
