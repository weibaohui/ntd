import { api, unwrap } from './client';

// Backup APIs

export async function mergeBackup(
  // workspace_id 逐条携带用户选定的工作空间；全局 workspace_id 传 null，由后端按每条解析
  todos: { title: string; prompt: string; status: string; executor?: string; scheduler_enabled: boolean; scheduler_config?: string; workspace_path?: string; workspace_id?: number | null }[],
  workspace_id?: number | null,
): Promise<string> {
  return unwrap(await api.post('/api/v1/backup/merge', { todos, workspace_id }));
}

// Database Backup APIs

export async function triggerLocalBackup(): Promise<string> {
  return unwrap(await api.post('/api/v1/backup/database/trigger'));
}

export async function optimizeDatabase(): Promise<string> {
  return unwrap(await api.post('/api/v1/backup/database/optimize'));
}

export async function getDatabaseBackupStatus(): Promise<{
  auto_backup_enabled: boolean;
  auto_backup_cron: string;
  auto_backup_max_files: number;
  last_backup: string | null;
  files: { name: string; size: number; created_at: string }[];
}> {
  return unwrap(await api.get('/api/v1/backup/database/status'));
}

export async function updateAutoBackup(enabled: boolean, cron: string, maxFiles?: number): Promise<string> {
  const body: Record<string, unknown> = { enabled, cron };
  if (maxFiles !== undefined) {
    body.max_files = maxFiles;
  }
  return unwrap(await api.put('/api/v1/backup/database/auto', body));
}

export async function deleteBackupFile(filename: string): Promise<string> {
  return unwrap(await api.delete('/api/v1/backup/database/file', { data: { filename } }));
}

// URL builder 返回给 <a href> / window.open，不经 axios 拦截器，手动写 v1 前缀
export function downloadBackupFileUrl(filename: string): string {
  return `/api/v1/backup/database/file?filename=${encodeURIComponent(filename)}`;
}

// Log Cleanup APIs

export async function getLogCleanupStatus(): Promise<{
  cleanup_days: number | null;
}> {
  return unwrap(await api.get('/api/v1/backup/log-cleanup/status'));
}

export async function updateLogCleanup(days: number | null): Promise<string> {
  return unwrap(await api.put('/api/v1/backup/log-cleanup', { days }));
}

export async function triggerLogCleanup(): Promise<string> {
  return unwrap(await api.post('/api/v1/backup/log-cleanup/trigger'));
}

// Todo Backup APIs

export async function getTodoBackupStatus(): Promise<{
  auto_backup_enabled: boolean;
  auto_backup_cron: string;
  auto_backup_max_files: number;
  last_backup: string | null;
  files: { name: string; size: number; created_at: string }[];
}> {
  return unwrap(await api.get('/api/v1/backup/todo/status'));
}

export async function triggerTodoBackup(): Promise<string> {
  return unwrap(await api.post('/api/v1/backup/todo/trigger'));
}

export async function updateTodoAutoBackup(enabled: boolean, cron: string, maxFiles?: number): Promise<string> {
  const body: Record<string, unknown> = { enabled, cron };
  if (maxFiles !== undefined) {
    body.max_files = maxFiles;
  }
  return unwrap(await api.put('/api/v1/backup/todo/auto', body));
}

export async function deleteTodoBackupFile(filename: string): Promise<string> {
  return unwrap(await api.delete('/api/v1/backup/todo/file', { data: { filename } }));
}

export function downloadTodoBackupFileUrl(filename: string): string {
  return `/api/v1/backup/todo/file?filename=${encodeURIComponent(filename)}`;
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
  return unwrap(await api.get('/api/v1/backup/skills/status'));
}

export async function triggerSkillBackup(): Promise<string> {
  return unwrap(await api.post('/api/v1/backup/skills/trigger'));
}

export async function updateSkillAutoBackup(enabled: boolean, cron: string, maxFiles?: number): Promise<string> {
  const body: Record<string, unknown> = { enabled, cron };
  if (maxFiles !== undefined) {
    body.max_files = maxFiles;
  }
  return unwrap(await api.put('/api/v1/backup/skills/auto', body));
}

export async function deleteSkillBackupFile(filename: string): Promise<string> {
  return unwrap(await api.delete('/api/v1/backup/skills/file', { data: { filename } }));
}

export function downloadSkillBackupFileUrl(filename: string): string {
  return `/api/v1/backup/skills/file?filename=${encodeURIComponent(filename)}`;
}


