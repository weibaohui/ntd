import type { Todo } from '../../types';

export const TIME_OPTIONS: { label: string; value: number }[] = [
  { label: '6h',  value: 6 },
  { label: '12h', value: 12 },
  { label: '24h', value: 24 },
  { label: '3d',  value: 72 },
  { label: '7d',  value: 168 },
];

export interface ColumnDef {
  status: Todo['status'];
  label: string;
  color: string;
}

export const COLUMNS: ColumnDef[] = [
  { status: 'pending',   label: '待办',     color: '#3b82f6' },
  { status: 'running',   label: '进行中',   color: '#f59e0b' },
  { status: 'completed', label: '已完成',   color: '#22c55e' },
  { status: 'failed',    label: '失败',     color: '#ef4444' },
];
