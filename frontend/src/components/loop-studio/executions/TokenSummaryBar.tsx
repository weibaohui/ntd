// Token 汇总条组件：展示本次 loop execution 的 token 总消耗。

import type { LoopExecutionTokenSummary } from '@/types/loop';
import { formatToken, formatCost } from './helpers';

// Token 徽章组件
function TokenBadge({ label, value, color }: { label: string; value: string; color: string }) {
  return (
    <span style={{
      display: 'inline-flex', alignItems: 'center', gap: 3,
      padding: '1px 6px', borderRadius: 4,
      background: `${color}0f`,
      fontSize: 11, fontWeight: 500, color,
    }}>
      {label}: {value}
    </span>
  );
}

// Token 汇总条组件：展示本次 loop execution 的 token 总消耗
export function TokenSummaryBar({ summary }: { summary: LoopExecutionTokenSummary }) {
  // 只在有实际消耗时才显示
  const hasTokens = summary.total_input_tokens > 0 || summary.total_output_tokens > 0;
  if (!hasTokens && summary.total_cost_usd <= 0) return null;
  return (
    <div style={{
      display: 'flex',
      alignItems: 'center',
      gap: 8, flexWrap: 'wrap',
      padding: '8px 12px',
      marginBottom: 8,
      background: 'var(--color-bg-hover)',
      borderRadius: 8,
      border: '1px solid var(--color-border)',
      fontSize: 12,
    }}>
      <span style={{ fontWeight: 600, color: 'var(--color-text)' }}>Token 消耗汇总</span>
      <TokenBadge label="输入" value={`${formatToken(summary.total_input_tokens)}`} color="#1677ff" />
      <TokenBadge label="输出" value={`${formatToken(summary.total_output_tokens)}`} color="#52c41a" />
      {summary.total_cache_read_input_tokens > 0 && (
        <TokenBadge label="缓存读取" value={`${formatToken(summary.total_cache_read_input_tokens)}`} color="#722ed1" />
      )}
      {summary.total_cache_creation_input_tokens > 0 && (
        <TokenBadge label="缓存创建" value={`${formatToken(summary.total_cache_creation_input_tokens)}`} color="#eb2f96" />
      )}
      {summary.total_cost_usd > 0 && (
        <span style={{
          padding: '2px 6px', borderRadius: 4,
          background: 'var(--color-warning-bg)', color: 'var(--color-warning)',
          fontWeight: 600, fontSize: 12,
        }}>
          费用 {formatCost(summary.total_cost_usd)}
        </span>
      )}
    </div>
  );
}
