// LoopKanban 辅助函数和常量。

import {
  CheckCircleOutlined,
  CloseCircleOutlined,
  LoadingOutlined,
  MinusCircleOutlined,
  ExclamationCircleOutlined,
} from '@ant-design/icons';

// 状态 → 颜色 + 图标
export function execStatusView(status: string): { color: string; icon: React.ReactNode; label: string } {
  switch (status) {
    case 'success':        return { color: 'green',    icon: <CheckCircleOutlined />,       label: '成功' };
    case 'failed':         return { color: 'red',      icon: <CloseCircleOutlined />,       label: '失败' };
    case 'partial':        return { color: 'orange',   icon: <CloseCircleOutlined />,       label: '部分' };
    case 'running':        return { color: 'blue',     icon: <LoadingOutlined />,           label: '运行中' };
    case 'cancelled':      return { color: 'default',  icon: <MinusCircleOutlined />,       label: '已取消' };
    case 'capped_step':    return { color: 'gold',     icon: <MinusCircleOutlined />,       label: '步数超限' };
    case 'capped_token':   return { color: 'purple',  icon: <MinusCircleOutlined />,       label: 'Token 超限' };
    case 'pending_approval': return { color: 'orange', icon: <ExclamationCircleOutlined />, label: '待审批' };
    default:               return { color: 'default',  icon: <MinusCircleOutlined />,       label: status };
  }
}

// 计算耗时
export function durationLabel(start: string, end: string | null): string {
  if (!end) return '进行中';
  const ms = new Date(end).getTime() - new Date(start).getTime();
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.floor(ms / 60_000)}m ${Math.floor((ms % 60_000) / 1000)}s`;
}

// Token 格式化：千位分隔
export function formatToken(n: number): string {
  return n.toLocaleString();
}

// 看板列定义
export interface ColumnDef {
  status: string;
  label: string;
  color: string;
}

export const COLUMNS: ColumnDef[] = [
  { status: 'running',         label: '运行中',    color: '#3b82f6' },
  { status: 'pending_approval', label: '待审批',   color: '#f59e0b' },
  { status: 'success',         label: '成功',      color: '#22c55e' },
  { status: 'partial',         label: '部分',      color: '#f97316' },
  { status: 'failed',          label: '失败',      color: '#ef4444' },
  { status: 'cancelled',       label: '已取消',    color: '#94a3b8' },
  { status: 'capped_step',     label: '步数超限',  color: '#eab308' },
  { status: 'capped_token',   label: 'Token超限', color: '#a855f7' },
];
