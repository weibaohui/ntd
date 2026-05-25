import { EXECUTORS } from '../../types';
import type { SkillMeta } from '../../types';

export const EXECUTOR_COLORS: Record<string, string> = {};
EXECUTORS.forEach(e => { EXECUTOR_COLORS[e.value] = e.color; });
EXECUTOR_COLORS['claude_code'] = EXECUTOR_COLORS['claudecode'];
EXECUTOR_COLORS['claude'] = EXECUTOR_COLORS['claudecode'];
EXECUTOR_COLORS['cbc'] = EXECUTOR_COLORS['codebuddy'];
EXECUTOR_COLORS['atom'] = EXECUTOR_COLORS['atomcode'];

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatTime(iso: string | null): string {
  if (!iso) return '-';
  try {
    const d = new Date(iso);
    return d.toLocaleDateString('zh-CN') + ' ' + d.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
  } catch {
    return iso;
  }
}

export function normalizeExecutor(name: string): string {
  return name.toLowerCase().replace(/[_\s-]/g, '');
}

export interface SkillTreeNode {
  key: string;
  name: string;
  type: 'category' | 'skill';
  executor: string;
  color: string;
  data: SkillMeta | null;
  children?: SkillTreeNode[];
  depth: number;
}

export interface ExportTask {
  id: string;
  executor: string;
  skillName: string;
  status: 'pending' | 'exporting' | 'completed' | 'failed';
  progress: number;
  error?: string;
  blobUrl?: string;
}
