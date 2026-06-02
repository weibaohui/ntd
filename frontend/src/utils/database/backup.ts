import { api, unwrap } from './client';

// Backup APIs

export async function exportBackup(): Promise<string> {
  const res = await api.get('/api/backup/export', {
    headers: { 'Accept': 'application/x-yaml' },
    responseType: 'text',
    transformResponse: [(data) => data],
  });
  if (typeof res.data === 'string') return res.data;
  return JSON.stringify(res.data);
}

export async function importBackup(yamlContent: string): Promise<string> {
  return unwrap(await api.post('/api/backup/import', yamlContent, {
    headers: { 'Content-Type': 'application/x-yaml' },
  }));
}

export async function mergeBackup(tags: { name: string; color: string }[], todos: { title: string; prompt: string; status: string; executor?: string; scheduler_enabled: boolean; scheduler_config?: string; tag_names: string[]; workspace?: string }[]): Promise<string> {
  return unwrap(await api.post('/api/backup/merge', { tags, todos }));
}

export async function exportSelectedBackup(todoIds: number[]): Promise<string> {
  const res = await api.post('/api/backup/export-selected', { todo_ids: todoIds }, {
    headers: { 'Accept': 'application/x-yaml' },
    responseType: 'text',
    transformResponse: [(data: unknown) => data],
  });
  if (typeof res.data === 'string') return res.data;
  return JSON.stringify(res.data);
}

// Database Backup APIs

export async function triggerLocalBackup(): Promise<string> {
  return unwrap(await api.post('/api/backup/database/trigger'));
}

export async function optimizeDatabase(): Promise<string> {
  return unwrap(await api.post('/api/backup/database/optimize'));
}

export async function getDatabaseBackupStatus(): Promise<{
  auto_backup_enabled: boolean;
  auto_backup_cron: string;
  auto_backup_max_files: number;
  last_backup: string | null;
  files: { name: string; size: number; created_at: string }[];
}> {
  return unwrap(await api.get('/api/backup/database/status'));
}

export async function updateAutoBackup(enabled: boolean, cron: string, maxFiles?: number): Promise<string> {
  const body: Record<string, unknown> = { enabled, cron };
  if (maxFiles !== undefined) {
    body.max_files = maxFiles;
  }
  return unwrap(await api.put('/api/backup/database/auto', body));
}

export async function deleteBackupFile(filename: string): Promise<string> {
  return unwrap(await api.delete('/api/backup/database/file', { data: { filename } }));
}

export function downloadBackupFileUrl(filename: string): string {
  return `/api/backup/database/file?filename=${encodeURIComponent(filename)}`;
}

// Log Cleanup APIs

export async function getLogCleanupStatus(): Promise<{
  cleanup_days: number | null;
}> {
  return unwrap(await api.get('/api/backup/log-cleanup/status'));
}

export async function updateLogCleanup(days: number | null): Promise<string> {
  return unwrap(await api.put('/api/backup/log-cleanup', { days }));
}

export async function triggerLogCleanup(): Promise<string> {
  return unwrap(await api.post('/api/backup/log-cleanup/trigger'));
}

// Todo Backup APIs

export async function getTodoBackupStatus(): Promise<{
  auto_backup_enabled: boolean;
  auto_backup_cron: string;
  auto_backup_max_files: number;
  last_backup: string | null;
  files: { name: string; size: number; created_at: string }[];
}> {
  return unwrap(await api.get('/api/backup/todo/status'));
}

export async function triggerTodoBackup(): Promise<string> {
  return unwrap(await api.post('/api/backup/todo/trigger'));
}

export async function updateTodoAutoBackup(enabled: boolean, cron: string, maxFiles?: number): Promise<string> {
  const body: Record<string, unknown> = { enabled, cron };
  if (maxFiles !== undefined) {
    body.max_files = maxFiles;
  }
  return unwrap(await api.put('/api/backup/todo/auto', body));
}

export async function deleteTodoBackupFile(filename: string): Promise<string> {
  return unwrap(await api.delete('/api/backup/todo/file', { data: { filename } }));
}

export function downloadTodoBackupFileUrl(filename: string): string {
  return `/api/backup/todo/file?filename=${encodeURIComponent(filename)}`;
}

// Skill Backup APIs

export interface ExecutorSkillInfo {
  executor: string;
  skills_count: number;
  skills_dir_exists: boolean;
}

export async function getSkillBackupStatus(): Promise<{
  auto_backup_enabled: boolean;
  auto_backup_cron: string;
  auto_backup_max_files: number;
  last_backup: string | null;
  files: { name: string; size: number; created_at: string }[];
  executor_skills: ExecutorSkillInfo[];
}> {
  return unwrap(await api.get('/api/backup/skills/status'));
}

export async function triggerSkillBackup(): Promise<string> {
  return unwrap(await api.post('/api/backup/skills/trigger'));
}

export async function updateSkillAutoBackup(enabled: boolean, cron: string, maxFiles?: number): Promise<string> {
  const body: Record<string, unknown> = { enabled, cron };
  if (maxFiles !== undefined) {
    body.max_files = maxFiles;
  }
  return unwrap(await api.put('/api/backup/skills/auto', body));
}

export async function deleteSkillBackupFile(filename: string): Promise<string> {
  return unwrap(await api.delete('/api/backup/skills/file', { data: { filename } }));
}

export function downloadSkillBackupFileUrl(filename: string): string {
  return `/api/backup/skills/file?filename=${encodeURIComponent(filename)}`;
}
