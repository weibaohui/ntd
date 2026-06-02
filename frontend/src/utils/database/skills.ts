import { api, unwrap } from './client';
import type { ExecutorSkills, SkillComparison, PaginatedInvocations } from '../../types';

export async function getSkillsList(): Promise<ExecutorSkills[]> {
  return unwrap(await api.get('/api/skills'));
}

export async function getSkillsComparison(): Promise<SkillComparison[]> {
  return unwrap(await api.get('/api/skills/compare'));
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

export async function getConfig(): Promise<import('../../types').Config> {
  return unwrap(await api.get('/api/config'));
}

export async function updateConfig(config: import('../../types').Config): Promise<import('../../types').Config> {
  return unwrap(await api.put('/api/config', config));
}

// Executor Config APIs

export async function getExecutors(): Promise<import('../../types').ExecutorConfig[]> {
  return unwrap(await api.get('/api/executors'));
}

export async function updateExecutor(name: string, data: { path?: string; enabled?: boolean; display_name?: string; session_dir?: string }): Promise<import('../../types').ExecutorConfig> {
  return unwrap(await api.put(`/api/executors/${encodeURIComponent(name)}`, data));
}

export async function detectExecutor(name: string): Promise<{ binary_found: boolean; path_resolved: string | null }> {
  return unwrap(await api.post(`/api/executors/${encodeURIComponent(name)}/detect`));
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
