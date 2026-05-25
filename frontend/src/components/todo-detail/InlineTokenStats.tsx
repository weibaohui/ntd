import { useState } from 'react';
import { PieChart } from '../PieChart';
import { AnimatedNumber } from '../AnimatedNumber';
import { DownOutlined, UpOutlined } from '@ant-design/icons';
import type { ExecutionSummary } from '../../types';

/** 内联 Token 统计摘要，支持展开查看详细分项 */
export function InlineTokenStats({ input, output, cacheRead, cacheCreate, totalTokens, summary }: {
  input: number; output: number; cacheRead: number; cacheCreate: number; totalTokens: number; summary: ExecutionSummary;
}) {
  const [expanded, setExpanded] = useState(false);
  const reasoningInput = input + cacheRead + cacheCreate;
  const costInput = input + cacheCreate;
  const outputRate = costInput > 0 ? (output / costInput * 100) : 0;

  const tokenSegments = [
    { value: input, color: '#3b82f6', label: '输入' },
    { value: output, color: '#22c55e', label: '输出' },
    { value: cacheRead, color: '#f59e0b', label: '缓存读' },
    { value: cacheCreate, color: '#a78bfa', label: '缓存写' },
  ];
  const extraSegments = [
    { value: reasoningInput, color: '#ec4899', label: '推理输入' },
    { value: costInput, color: '#f97316', label: '成本输入' },
    { value: outputRate, color: '#14b8a6', label: '输出率', isPercent: true as const },
  ];
  return (
    <div style={{ position: 'relative', display: 'inline-flex', alignItems: 'center' }}>
      <button
        type="button"
        aria-expanded={expanded}
        aria-label="Token 统计摘要，点击展开详情"
        onClick={() => setExpanded(!expanded)}
        style={{ display: 'inline-flex', alignItems: 'center', gap: 8, cursor: 'pointer', userSelect: 'none', fontSize: 11, color: 'var(--color-text-secondary)', background: 'none', border: 'none', padding: 0 }}
      >
        <PieChart segments={tokenSegments.filter(s => s.value > 0)} size={20} />
        <span style={{ fontWeight: 700, color: 'var(--color-text)', fontSize: 13 }}><AnimatedNumber value={totalTokens} duration={1.2} chineseFormat /></span>
        <span>Tokens</span>
        <span style={{ color: 'var(--color-border)' }}>|</span>
        <span>执行 <strong style={{ color: 'var(--color-text)' }}>{summary.total_executions}</strong> 次</span>
        <span style={{ color: 'var(--color-success)' }}>成功 {summary.success_count}</span>
        <span style={{ color: 'var(--color-error)' }}>失败 {summary.failed_count}</span>
        {summary.total_cost_usd != null && (
          <span style={{ color: 'var(--color-warning)', fontWeight: 600 }}>${summary.total_cost_usd.toFixed(4)}</span>
        )}
        {expanded ? <UpOutlined style={{ fontSize: 10 }} /> : <DownOutlined style={{ fontSize: 10 }} />}
      </button>
      {expanded && (
        <div style={{ position: 'absolute', top: '100%', left: 0, zIndex: 10, marginTop: 4, background: 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)', borderRadius: 8, padding: 10, boxShadow: '0 4px 12px rgba(0,0,0,0.15)', minWidth: 280 }}>
          <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap', fontSize: 11 }}>
            {tokenSegments.filter(s => s.value > 0).map(s => (
              <span key={s.label} style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                <span style={{ width: 8, height: 8, borderRadius: '50%', background: s.color }} />
                {s.label}: <AnimatedNumber value={s.value} duration={1.2} chineseFormat />
              </span>
            ))}
          </div>
          <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap', fontSize: 11, marginTop: 8, paddingTop: 8, borderTop: '1px solid var(--color-border-light)' }}>
            {extraSegments.map(s => (
              <span key={s.label} style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                <span style={{ width: 8, height: 8, borderRadius: '50%', background: s.color }} />
                {s.label}: {s.isPercent ? s.value.toFixed(1) + '%' : <AnimatedNumber value={s.value} duration={1.2} chineseFormat />}
              </span>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
