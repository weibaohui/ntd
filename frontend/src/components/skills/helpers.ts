import { EXECUTOR_COLORS, getExecutorColor } from '@/types';
import type { SkillMeta } from '@/types';
import { formatDateTime } from '@/utils/format';

export { EXECUTOR_COLORS, getExecutorColor };

export function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '-';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export { formatDateTime as formatTime };

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
