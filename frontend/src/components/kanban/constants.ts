import type { Todo } from '@/types';

// 时间分段选项 TIME_OPTIONS 已迁移至共享组件 common/TimeRangeSegmented.tsx
// （TIME_RANGE_OPTIONS 全站唯一事实源，需求 031），本文件不再导出，
// 避免与 MemorialBoard/KanbanBoard 各持一份的历史重复问题重演。

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
