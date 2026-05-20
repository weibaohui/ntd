import { Empty } from 'antd';
import { ArrowUpOutlined, ArrowDownOutlined, CrownOutlined } from '@ant-design/icons';
import { AnimatedNumber } from '../AnimatedNumber';

interface MetricCardProps {
  title: string;
  value: number;
  suffix?: string;
  prefix?: React.ReactNode;
  change?: number;
  changeLabel?: string;
  color: string;
  loading?: boolean;
  decimals?: number;
  chineseFormat?: boolean;
}

export function MetricCard({
  title,
  value,
  suffix,
  prefix,
  change,
  changeLabel,
  color,
  loading = false,
  decimals = 0,
  chineseFormat = false,
}: MetricCardProps) {
  const isPositive = change !== undefined && change > 0;
  const isNegative = change !== undefined && change < 0;
  const changeColor = isPositive ? '#22c55e' : isNegative ? '#ef4444' : 'var(--color-text-tertiary)';
  const ChangeIcon = isPositive ? ArrowUpOutlined : isNegative ? ArrowDownOutlined : null;

  return (
    <div
      style={{
        padding: '16px 18px',
        borderRadius: 12,
        background: 'var(--color-fill-quaternary)',
        display: 'flex',
        flexDirection: 'column',
        gap: 8,
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <span style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>{title}</span>
        {prefix && (
          <div
            style={{
              width: 28,
              height: 28,
              borderRadius: 8,
              backgroundColor: `${color}18`,
              color,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              fontSize: 14,
            }}
          >
            {prefix}
          </div>
        )}
      </div>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: 6 }}>
        <span style={{ fontSize: 28, fontWeight: 700, color: 'var(--color-text)', lineHeight: 1.2 }}>
          <AnimatedNumber value={loading ? 0 : value} duration={0.8} decimals={decimals} chineseFormat={chineseFormat} />
        </span>
        {suffix && (
          <span style={{ fontSize: 14, fontWeight: 500, color: 'var(--color-text-secondary)' }}>{suffix}</span>
        )}
      </div>
      {change !== undefined && (
        <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
          {ChangeIcon && <ChangeIcon style={{ fontSize: 10, color: changeColor }} />}
          <span style={{ fontSize: 11, color: changeColor, fontWeight: 600 }}>
            {Math.abs(change).toFixed(1)}%
          </span>
          {changeLabel && (
            <span style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>{changeLabel}</span>
          )}
        </div>
      )}
    </div>
  );
}

interface LeaderboardItem {
  rank: number;
  name: string;
  avatar?: string;
  tokens: number;
  sessions: number;
  change?: number;
}

interface LeaderboardProps {
  data: LeaderboardItem[];
  maxTokens?: number;
  loading?: boolean;
}

const RANK_COLORS = ['#f59e0b', '#94a3b8', '#cd7f32'];

export function Leaderboard({ data, maxTokens, loading: _loading = false }: LeaderboardProps) {
  const max = maxTokens ?? Math.max(1, ...data.map(d => d.tokens ?? 0));

  if (data.length === 0) {
    return <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无排行数据" />;
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      {data.map((item) => {
        const rankColor = item.rank <= 3 ? RANK_COLORS[item.rank - 1] : 'var(--color-text-tertiary)';
        const isTop3 = item.rank <= 3;
        return (
          <div
            key={item.rank}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 12,
              padding: '10px 14px',
              borderRadius: 10,
              background: isTop3 ? `${rankColor}08` : 'var(--color-fill-quaternary)',
              border: isTop3 ? `1px solid ${rankColor}30` : '1px solid transparent',
            }}
          >
            <div
              style={{
                width: 28,
                height: 28,
                borderRadius: 8,
                background: isTop3 ? `${rankColor}20` : 'var(--color-fill-elevated)',
                color: rankColor,
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                fontSize: 12,
                fontWeight: 700,
                flexShrink: 0,
              }}
            >
              {isTop3 ? <CrownOutlined /> : `#${item.rank}`}
            </div>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 13, fontWeight: 600, color: 'var(--color-text)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {item.name}
              </div>
              <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>
                {item.sessions} sessions
              </div>
            </div>
            <div style={{ textAlign: 'right' }}>
              <div style={{ fontSize: 14, fontWeight: 700, color: 'var(--color-text)' }}>
                {item.tokens >= 10000 ? `${(item.tokens / 10000).toFixed(1)}w` : (item.tokens ?? 0)}
              </div>
              {item.change != null && (
                <div style={{ fontSize: 10, color: (item.change ?? 0) > 0 ? '#22c55e' : '#ef4444' }}>
                  {(item.change ?? 0) > 0 ? '+' : ''}{(item.change ?? 0).toFixed(1)}%
                </div>
              )}
            </div>
            <div
              style={{
                width: 40,
                height: 4,
                borderRadius: 2,
                background: 'var(--color-fill-quaternary)',
                overflow: 'hidden',
                flexShrink: 0,
              }}
            >
              <div
                style={{
                  height: '100%',
                  width: `${(item.tokens / max) * 100}%`,
                  minWidth: item.tokens > 0 ? 4 : 0,
                  borderRadius: 2,
                  background: rankColor,
                  transition: 'width 0.6s ease',
                }}
              />
            </div>
          </div>
        );
      })}
    </div>
  );
}

interface EventMarker {
  date: string;
  label: string;
  color?: string;
}

interface EventTrendChartProps {
  data: { date: string; success: number; failed: number }[];
  events?: EventMarker[];
  height?: number;
}

export function EventTrendChart({ data, events = [], height = 200 }: EventTrendChartProps) {
  if (data.length === 0) {
    return (
      <div style={{ height, display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--color-text-tertiary)', fontSize: 13 }}>
        暂无数据
      </div>
    );
  }

  const w = 600;
  const h = height;
  const padL = 45;
  const padR = 12;
  const padB = 32;
  const padT = 20;
  const chartW = w - padL - padR;
  const chartH = h - padT - padB;

  const maxVal = Math.max(...data.map(d => Math.max(d.success, d.failed)), 1);

  const eventMap = new Map(events.map(e => [e.date, e]));

  const points = data.map((d, i) => {
    const x = padL + (i / Math.max(data.length - 1, 1)) * chartW;
    const succY = padT + chartH - (d.success / maxVal) * chartH;
    const failY = padT + chartH - (d.failed / maxVal) * chartH;
    return { x, succY, failY, date: d.date };
  });

  const yTicks = [0, maxVal * 0.5, maxVal];

  const successPath = points.map((p, i) => `${i === 0 ? 'M' : 'L'} ${p.x} ${p.succY}`).join(' ');
  const failPath = points.map((p, i) => `${i === 0 ? 'M' : 'L'} ${p.x} ${p.failY}`).join(' ');

  return (
    <div style={{ width: '100%' }}>
      <div style={{ display: 'flex', gap: 16, marginBottom: 8, justifyContent: 'flex-end', flexWrap: 'wrap' }}>
        <span style={{ fontSize: 11, color: '#22c55e', display: 'flex', alignItems: 'center', gap: 4 }}>
          <span style={{ width: 8, height: 8, borderRadius: 2, background: '#22c55e' }} />
          成功
        </span>
        <span style={{ fontSize: 11, color: '#ef4444', display: 'flex', alignItems: 'center', gap: 4 }}>
          <span style={{ width: 8, height: 8, borderRadius: 2, background: '#ef4444' }} />
          失败
        </span>
        {events.length > 0 && (
          <span style={{ fontSize: 11, color: '#f59e0b', display: 'flex', alignItems: 'center', gap: 4 }}>
            <span style={{ width: 8, height: 8, borderRadius: 2, background: '#f59e0b' }} />
            里程碑
          </span>
        )}
      </div>
      <svg width="100%" height={h} viewBox={`0 0 ${w} ${h}`} style={{ overflow: 'visible' }}>
        {yTicks.map((t, i) => {
          const y = padT + chartH - (t / maxVal) * chartH;
          return (
            <g key={i}>
              <line x1={padL} y1={y} x2={w - padR} y2={y} stroke="var(--color-border)" strokeWidth={1} />
              <text x={padL - 6} y={y + 4} textAnchor="end" fontSize={10} fill="var(--color-text-tertiary)">
                {Math.round(t)}
              </text>
            </g>
          );
        })}
        <path d={successPath} fill="none" stroke="#22c55e" strokeWidth={2} strokeLinejoin="round" />
        <path d={failPath} fill="none" stroke="#ef4444" strokeWidth={2} strokeLinejoin="round" />
        {points.map((p, i) => {
          const event = eventMap.get(p.date);
          return (
            <g key={i}>
              <circle cx={p.x} cy={p.succY} r={3} fill="#22c55e" />
              <circle cx={p.x} cy={p.failY} r={3} fill="#ef4444" />
              {event && (
                <>
                  <line x1={p.x} y1={padT} x2={p.x} y2={padT + chartH} stroke="#f59e0b" strokeWidth={1} strokeDasharray="4,2" />
                  <circle cx={p.x} cy={padT} r={4} fill="#f59e0b" />
                  <text
                    x={p.x}
                    y={padT - 6}
                    textAnchor="middle"
                    fontSize={9}
                    fill="#f59e0b"
                    fontWeight={600}
                  >
                    {event.label}
                  </text>
                </>
              )}
              <text
                x={p.x}
                y={h - 6}
                textAnchor="middle"
                fontSize={9}
                fill="var(--color-text-tertiary)"
                transform={data.length > 14 ? `rotate(-35, ${p.x}, ${h - 6})` : undefined}
              >
                {p.date.slice(5)}
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}

interface HighlightStatProps {
  label: string;
  value: number | string;
  subLabel?: string;
  subValue?: string;
  color: string;
  icon?: React.ReactNode;
}

export function HighlightStat({ label, value, subLabel, subValue, color, icon }: HighlightStatProps) {
  const displayValue = value ?? '-';
  return (
    <div
      style={{
        padding: '14px 16px',
        borderRadius: 10,
        background: `${color}10`,
        border: `1px solid ${color}25`,
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 8 }}>
        {icon && <span style={{ color, fontSize: 14 }}>{icon}</span>}
        <span style={{ fontSize: 11, color: 'var(--color-text-secondary)' }}>{label}</span>
      </div>
      <div style={{ fontSize: 22, fontWeight: 700, color, marginBottom: 4 }}>
        {typeof displayValue === 'number' ? (
          <AnimatedNumber value={displayValue} duration={1} />
        ) : displayValue}
      </div>
      {subLabel && (
        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>
          {subLabel}
          {subValue && <span style={{ color, fontWeight: 600 }}> {subValue}</span>}
        </div>
      )}
    </div>
  );
}

interface TeamMemberCardProps {
  name: string;
  role: string;
  nickname: string;
  avatar?: string;
  stats: {
    tokens: number;
    sessions: number;
    satisfaction?: number;
  };
  color: string;
}

export function TeamMemberCard({ name, role, nickname, stats, color }: TeamMemberCardProps) {
  return (
    <div
      style={{
        padding: '16px',
        borderRadius: 12,
        background: 'var(--color-fill-quaternary)',
        border: `1px solid ${color}20`,
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 12 }}>
        <div
          style={{
            width: 44,
            height: 44,
            borderRadius: 12,
            background: `${color}20`,
            color,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            fontSize: 20,
            fontWeight: 700,
            flexShrink: 0,
          }}
        >
          {name.charAt(0)}
        </div>
        <div style={{ minWidth: 0 }}>
          <div style={{ fontSize: 14, fontWeight: 700, color: 'var(--color-text)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {name}
          </div>
          <div style={{ fontSize: 11, color: 'var(--color-text-secondary)' }}>
            {role}
          </div>
          <div style={{ fontSize: 11, color, fontWeight: 600 }}>
            {nickname}
          </div>
        </div>
      </div>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 8 }}>
        <div style={{ textAlign: 'center', padding: '8px 4px', borderRadius: 8, background: 'var(--color-fill-elevated)' }}>
          <div style={{ fontSize: 14, fontWeight: 700, color: 'var(--color-text)' }}>
            {stats.tokens >= 10000 ? `${(stats.tokens / 10000).toFixed(1)}w` : stats.tokens}
          </div>
          <div style={{ fontSize: 9, color: 'var(--color-text-tertiary)' }}>tokens</div>
        </div>
        <div style={{ textAlign: 'center', padding: '8px 4px', borderRadius: 8, background: 'var(--color-fill-elevated)' }}>
          <div style={{ fontSize: 14, fontWeight: 700, color: 'var(--color-text)' }}>
            {stats.sessions}
          </div>
          <div style={{ fontSize: 9, color: 'var(--color-text-tertiary)' }}>sessions</div>
        </div>
        {stats.satisfaction !== undefined && (
          <div style={{ textAlign: 'center', padding: '8px 4px', borderRadius: 8, background: 'var(--color-fill-elevated)' }}>
            <div style={{ fontSize: 14, fontWeight: 700, color: '#22c55e' }}>
              {stats.satisfaction}%
            </div>
            <div style={{ fontSize: 9, color: 'var(--color-text-tertiary)' }}>satisfaction</div>
          </div>
        )}
      </div>
    </div>
  );
}
