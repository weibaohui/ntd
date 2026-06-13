import { EXECUTOR_COLORS, getExecutorColor } from '@/types';
import type { SkillMeta } from '@/types';

export { EXECUTOR_COLORS, getExecutorColor };

export function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '-';
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

export function splitSkillName(name: string): { category: string | null; shortName: string } {
  if (!name.includes('/')) return { category: null, shortName: name };
  const parts = name.split('/');
  return { category: parts[0], shortName: parts.slice(1).join('/') };
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
