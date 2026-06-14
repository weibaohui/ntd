import { Card, Empty } from 'antd';
import { BarChartOutlined, ThunderboltOutlined } from '@ant-design/icons';
import { PieChart, PieChartLegend } from '@/components/PieChart';
import { TrendChart, ContributionHeatmap } from './DashboardCharts';
import { AnimatedNumber } from '@/components/AnimatedNumber';
// STATUS_COLORS 走全应用共享的 @/constants；dashboard 自己的 constants.ts
// 只保留 dashboard 私有的标签/触发类型等常量，避免再次出现双源 STATUS_COLORS。
import { TRIGGER_LABELS, TRIGGER_COLORS, STATUS_LABELS } from './constants';
import { STATUS_COLORS } from '@/constants';
import type { DashboardStats } from '@/types';

interface ChartCardProps {
  stats: DashboardStats | null;
}

interface StatusChartCardProps extends ChartCardProps {
  totalTodos: number;
}

export function StatusChartCard({ stats, totalTodos }: StatusChartCardProps) {
  const statusSegments = [
    { value: stats?.pending_todos ?? 0, color: STATUS_COLORS.pending, label: STATUS_LABELS.pending },
    { value: stats?.running_todos ?? 0, color: STATUS_COLORS.running, label: STATUS_LABELS.running },
    { value: stats?.completed_todos ?? 0, color: STATUS_COLORS.completed, label: STATUS_LABELS.completed },
    { value: stats?.failed_todos ?? 0, color: STATUS_COLORS.failed, label: STATUS_LABELS.failed },
  ].filter((s) => s.value > 0);

  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><BarChartOutlined /><span>任务状态分布</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '16px 20px' } }}
    >
      {statusSegments.length > 0 ? (
        <div style={{ display: 'flex', alignItems: 'center', gap: 24, flexWrap: 'wrap' }}>
          <PieChart segments={statusSegments} size={140} centerText={<AnimatedNumber value={totalTodos} duration={1.2} chineseFormat />} centerSubtext="总计" />
          <PieChartLegend segments={statusSegments} />
        </div>
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无任务" />
      )}
    </Card>
  );
}

export function TokenChartCard({ stats }: ChartCardProps) {
  const tokenSegments = stats
    ? [
        { value: stats.total_input_tokens, color: '#3b82f6', label: '输入 Tokens' },
        { value: stats.total_output_tokens, color: '#22c55e', label: '输出 Tokens' },
        { value: stats.total_cache_read_tokens, color: '#f59e0b', label: '缓存读' },
        { value: stats.total_cache_creation_tokens, color: '#a78bfa', label: '缓存写' },
      ].filter((s) => s.value > 0)
    : [];

  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><ThunderboltOutlined /><span>Token 消耗</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '16px 20px' } }}
    >
      {tokenSegments.length > 0 ? (
        <div style={{ display: 'flex', alignItems: 'center', gap: 24, flexWrap: 'wrap' }}>
          <PieChart
            segments={tokenSegments}
            size={140}
            centerText={
              stats
                ? <AnimatedNumber value={stats.total_input_tokens + stats.total_output_tokens + stats.total_cache_read_tokens + stats.total_cache_creation_tokens} duration={1.2} chineseFormat />
                : <AnimatedNumber value={0} duration={1.2} chineseFormat />
            }
            centerSubtext="Tokens"
          />
          <PieChartLegend segments={tokenSegments} chineseFormat />
        </div>
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无 Token 数据" />
      )}
    </Card>
  );
}

export function TriggerSourceCard({ stats }: ChartCardProps) {
  const triggerData = stats?.trigger_type_distribution ?? [];
  const totalTrigger = triggerData.reduce((sum, t) => sum + t.count, 0);
  const cronCount = triggerData.find((t) => t.trigger_type === 'cron')?.count ?? 0;
  const autoRate = totalTrigger > 0 ? (cronCount / totalTrigger) * 100 : 0;

  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><BarChartOutlined /><span>触发来源分析</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '16px 20px' } }}
    >
      {totalTrigger > 0 ? (
        <div>
          <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 6 }}>
            <span style={{ fontSize: 13, color: 'var(--color-text-secondary)' }}>自动化率（定时占比）</span>
            <span style={{ fontSize: 15, fontWeight: 700, color: '#8b5cf6' }}><AnimatedNumber value={autoRate} duration={1.2} decimals={1} suffix="%" /></span>
          </div>
          <div style={{ height: 6, borderRadius: 3, background: 'var(--color-fill-quaternary)', overflow: 'hidden', marginBottom: 16 }}>
            <div style={{ height: '100%', width: `${autoRate}%`, borderRadius: 3, background: 'linear-gradient(90deg, #3b82f6, #8b5cf6)', transition: 'width 0.8s ease' }} />
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 12 }}>
            {triggerData.map((t) => {
              const label = TRIGGER_LABELS[t.trigger_type] || t.trigger_type;
              const color = TRIGGER_COLORS[t.trigger_type] || '#6b7280';
              return (
                <div key={t.trigger_type} style={{ padding: '10px 14px', borderRadius: 10, background: `${color}10` }}>
                  <div style={{ fontSize: 11, color: 'var(--color-text-secondary)', marginBottom: 2 }}>{label}</div>
                  <div style={{ fontSize: 18, fontWeight: 700, color }}><AnimatedNumber value={t.count} duration={0.8} chineseFormat /></div>
                </div>
              );
            })}
          </div>
        </div>
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无数据" />
      )}
    </Card>
  );
}

export function TrendChartCard({ stats }: ChartCardProps) {
  const trendData = stats?.daily_executions ?? [];

  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><BarChartOutlined /><span>执行趋势</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '16px 20px' } }}
    >
      <TrendChart data={trendData} height={180} />
    </Card>
  );
}

export function ContributionHeatmapCard({ stats }: ChartCardProps) {
  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><BarChartOutlined /><span>活动热力图</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '16px 20px' } }}
    >
      <ContributionHeatmap data={stats?.daily_executions ?? []} />
    </Card>
  );
}

export function TokenTrendChartCard({ stats }: ChartCardProps) {
  const tokenTrendData = stats?.daily_token_stats ?? [];
  const maxToken = Math.max(...tokenTrendData.map(d => d.input_tokens + d.output_tokens), 1);

  const svg = tokenTrendData.length > 0 ? (
    <svg width="100%" height={180} viewBox="0 0 600 180" style={{ overflow: 'visible' }}>
      {(() => {
        const w = 600;
        const h = 180;
        const padL = 45;
        const padR = 12;
        const padB = 28;
        const padT = 12;
        const chartW = w - padL - padR;
        const chartH = h - padT - padB;

        const yTicks = [0, maxToken * 0.5, maxToken];

        const points = tokenTrendData.map((d, i) => {
          const x = padL + (i / Math.max(tokenTrendData.length - 1, 1)) * chartW;
          const inputY = padT + chartH - (d.input_tokens / maxToken) * chartH;
          const outputY = padT + chartH - (d.output_tokens / maxToken) * chartH;
          return { x, inputY, outputY, date: d.date };
        });

        const inputPath = points.map((p, i) => `${i === 0 ? 'M' : 'L'} ${p.x} ${p.inputY}`).join(' ');
        const outputPath = points.map((p, i) => `${i === 0 ? 'M' : 'L'} ${p.x} ${p.outputY}`).join(' ');

        return (
          <>
            {yTicks.map((t, i) => {
              const y = padT + chartH - (t / maxToken) * chartH;
              return (
                <g key={i}>
                  <line x1={padL} y1={y} x2={w - padR} y2={y} stroke="var(--color-border-secondary)" strokeWidth={1} />
                  <text x={padL - 6} y={y + 4} textAnchor="end" fontSize={10} fill="var(--color-text-tertiary)">
                    {t >= 10000 ? `${(t/10000).toFixed(0)}w` : t}
                  </text>
                </g>
              );
            })}
            <path d={inputPath} fill="none" stroke="#3b82f6" strokeWidth={2} strokeLinejoin="round" />
            <path d={outputPath} fill="none" stroke="#22c55e" strokeWidth={2} strokeLinejoin="round" />
            {points.map((p, i) => (
              <g key={i}>
                <circle cx={p.x} cy={p.inputY} r={3} fill="#3b82f6" />
                <circle cx={p.x} cy={p.outputY} r={3} fill="#22c55e" />
                <text
                  x={p.x}
                  y={h - 6}
                  textAnchor="middle"
                  fontSize={9}
                  fill="var(--color-text-tertiary)"
                  transform={tokenTrendData.length > 14 ? `rotate(-35, ${p.x}, ${h - 6})` : undefined}
                >
                  {p.date.slice(5)}
                </text>
              </g>
            ))}
          </>
        );
      })()}
    </svg>
  ) : null;

  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><BarChartOutlined /><span>Token 趋势</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '16px 20px' } }}
    >
      {tokenTrendData.length > 0 ? (
        <div style={{ width: '100%' }}>
          <div style={{ display: 'flex', gap: 16, marginBottom: 8, justifyContent: 'flex-end' }}>
            <span style={{ fontSize: 11, color: '#3b82f6', display: 'flex', alignItems: 'center', gap: 4 }}>
              <span style={{ width: 8, height: 8, borderRadius: 2, background: '#3b82f6' }} />
              输入
            </span>
            <span style={{ fontSize: 11, color: '#22c55e', display: 'flex', alignItems: 'center', gap: 4 }}>
              <span style={{ width: 8, height: 8, borderRadius: 2, background: '#22c55e' }} />
              输出
            </span>
          </div>
          {svg}
        </div>
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无 Token 趋势数据" />
      )}
    </Card>
  );
}
