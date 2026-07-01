// Loop 执行记录相关的辅助函数和常量。

import {
  CheckCircleOutlined,
  CloseCircleOutlined,
  LoadingOutlined,
  MinusCircleOutlined,
  ExclamationCircleOutlined,
} from '@ant-design/icons';

// 状态 → 颜色 + 图标 + 标签，与 LoopListPanel.executionIcon 保持一致
// 兼容旧记录：旧版只支持步数限制，status = "capped" 视为步数超限。
export function execStatusView(status: string): { color: string; icon: React.ReactNode; label: string } {
  switch (status) {
    case 'success': return { color: 'green', icon: <CheckCircleOutlined />, label: '成功' };
    case 'failed': return { color: 'red', icon: <CloseCircleOutlined />, label: '失败' };
    case 'partial': return { color: 'orange', icon: <CloseCircleOutlined />, label: '部分' };
    case 'running': return { color: 'blue', icon: <LoadingOutlined />, label: '运行中' };
    case 'cancelled': return { color: 'default', icon: <MinusCircleOutlined />, label: '已取消' };
    case 'capped':
    case 'capped_step': return { color: 'gold', icon: <MinusCircleOutlined />, label: '步数超限' };
    case 'capped_token': return { color: 'purple', icon: <MinusCircleOutlined />, label: 'Token 超限' };
    case 'pending_approval': return { color: 'orange', icon: <ExclamationCircleOutlined />, label: '等待审批' };
    default: return { color: 'default', icon: <MinusCircleOutlined />, label: status };
  }
}

// 计算耗时标签
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

// Cost 格式化：美元小数
export function formatCost(cost: number): string {
  if (cost < 0.01) return '$<0.01';
  return `$${cost.toFixed(4)}`;
}
