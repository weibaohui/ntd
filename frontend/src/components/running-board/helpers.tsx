// RunningBoard 辅助函数和常量。

import {
  ClockCircleOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
  LoadingOutlined,
  EyeOutlined,
  TrophyOutlined,
} from '@ant-design/icons';
import type { RunningBoardColumn } from '@/types';

// 格式化下次运行时间
export function formatNextRunAt(nextRunAt: string | null): string {
  if (!nextRunAt) return '-';
  const now = Date.now();
  const target = new Date(nextRunAt).getTime();
  const diff = target - now;
  if (diff < 0) return '即将触发';
  if (diff < 60_000) return `${Math.floor(diff / 1000)}s 后`;
  if (diff < 3600_000) return `${Math.floor(diff / 60_000)}m 后`;
  return `${(diff / 3600_000).toFixed(1)}h 后`;
}

// 列图标映射
export const COLUMN_ICONS: Record<RunningBoardColumn, React.ReactNode> = {
  scheduled: <ClockCircleOutlined />,
  running: <LoadingOutlined />,
  completed: <CheckCircleOutlined />,
  reviewing: <EyeOutlined />,
  review_passed: <TrophyOutlined />,
  failed: <CloseCircleOutlined />,
};
