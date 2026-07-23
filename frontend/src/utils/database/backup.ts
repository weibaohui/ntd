import { api, unwrap } from './client';

// Backup APIs

export async function mergeBackup(
  tags: { name: string; color: string }[],
  // workspace_id 逐条携带用户选定的工作空间；全局 workspace_id 传 null，由后端按每条解析
  todos: { title: string; prompt: string; status: string; executor?: string; scheduler_enabled: boolean; scheduler_config?: string; tag_names: string[]; workspace_path?: string; workspace_id?: number | null }[],
  workspace_id?: number | null,
): Promise<string> {
  return unwrap(await api.post('/api/backup/merge', { tags, todos, workspace_id }));
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

// URL builder 返回给 <a href> / window.open，不经 axios 拦截器，手动写 v1 前缀
export function downloadBackupFileUrl(filename: string): string {
  return `/api/v1/backup/database/file?filename=${encodeURIComponent(filename)}`;
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
  return `/api/v1/backup/skills/file?filename=${encodeURIComponent(filename)}`;
}

// Loop Import/Export APIs

export interface LoopImportPreviewLoop {
  name: string;
  /** 导出文件里的原始工作空间 ID（可能为空） */
  workspace_id?: number | null;
  /** 导出文件里的原始工作空间路径，展示用 */
  workspace_path?: string | null;
  /** 解析后的工作空间 ID（0=未匹配，需用户逐条指定） */
  resolved_workspace_id: number;
  /** 解析后的工作空间名称（未匹配时为 null） */
  resolved_workspace_name?: string | null;
  /** 原始 workspace_id 在当前库是否存在 */
  source_matched: boolean;
}

export interface LoopImportPreview {
  valid: boolean;
  pseudo_ids: string[];
  summary: {
    loops: number;
    steps: number;
    todos: number;
    review_templates: number;
    tags: number;
    triggers: number;
  };
  conflicts: { type: string; name: string; action: string }[];
  warnings: { type: string; message: string }[];
  /** 每条 loop 的工作空间匹配情况，供前端逐条渲染/指派 */
  loops: LoopImportPreviewLoop[];
}

/**
 * 预览 loop 导入数据（原生 fetch，不经 axios 拦截器，手动写 v1 路径）。
 * workspaceId 为导入目标空间的上下文（v1 workspace-scoped）。
 */
export async function previewLoopImport(workspaceId: number, yaml: string): Promise<LoopImportPreview> {
  const response = await fetch(`/api/v1/workspaces/${workspaceId}/loops/import/preview`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-yaml' },
    body: yaml,
  });
  if (!response.ok) {
    const err = await response.json().catch(() => ({ message: response.statusText }));
    throw new Error(err.message || `HTTP ${response.status}`);
  }
  // 后端统一返回 {code, data, message} 包裹体，这里解包出 data
  const body = await response.json() as { code: number; data: LoopImportPreview | null; message: string };
  if (body.code !== 0 || !body.data) {
    throw new Error(body.message || `Error ${body.code}`);
  }
  return body.data;
}

export interface LoopImportResult {
  success: boolean;
  created: {
    loops: number;
    todos: number;
    review_templates: number;
    tags: number;
    triggers: number;
    steps: number;
  };
  warnings: { type: string; message: string }[];
}

/**
 * 合并导入 loops。
 * workspaceId 为 URL 路径段（v1 workspace-scoped 上下文空间）。
 * workspace_id 全局传 null（逐条由 workspace_overrides 指定）。
 * workspace_overrides: loop name → workspace_id（用户逐行选择，仅含已指定的非空项）。
 * skip_names: 用户选择「跳过」的同名环路名集合，后端不创建/覆盖、同名保留原样。
 */
export async function mergeLoops(
  workspaceId: number,
  yaml: string,
  workspace_id: number | null,
  workspace_overrides?: Record<string, number>,
  skip_names?: string[],
): Promise<{ success: boolean; created: LoopImportResult['created']; updated: LoopImportResult['created']; skipped: string[]; warnings: { type: string; message: string }[] }> {
  return unwrap(await api.post(`/api/workspaces/${workspaceId}/loops/merge`, { yaml, workspace_id, workspace_overrides, skip_names }));
}
