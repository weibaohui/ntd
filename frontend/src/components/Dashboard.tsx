import { useEffect, useState } from 'react';
import { Card, Table, Badge, Tag, Empty, Masonry, App, Button, Segmented, DatePicker } from 'antd';
import {
  LeftOutlined,
  FileTextOutlined,
  PlayCircleOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
  TagOutlined,
  ClockCircleOutlined,
  ThunderboltOutlined,
  DollarOutlined,
  BarChartOutlined,
  MessageOutlined,
  TrophyOutlined,
  FireOutlined,
  UserOutlined,
} from '@ant-design/icons';
import dayjs from 'dayjs';
import { useApp } from '../hooks/useApp';
import { PieChart, PieChartLegend } from './PieChart';
import { TrendChart, ContributionHeatmap } from './dashboard/DashboardCharts';
import { MetricCard, Leaderboard, HighlightStat, TeamMemberCard } from './dashboard/EnhancedCards';
import { AnimatedNumber } from './AnimatedNumber';
import * as db from '../utils/database';
import { getExecutorOption } from '../types';
import type { DashboardStats, FeishuMessageStats } from '../types';
import { formatRelativeTime } from '../utils/datetime';
import { ShareCard } from './ShareCard';

const TIME_RANGE_OPTIONS: { label: string; value: number | 'custom' }[] = [
  { label: '5小时', value: 5 },
  { label: '7天', value: 168 },
  { label: '14天', value: 336 },
  { label: '30天', value: 720 },
  { label: '自定义', value: 'custom' },
];

const STATUS_COLORS: Record<string, string> = {
  pending: '#94a3b8',
  running: '#3b82f6',
  completed: '#22c55e',
  failed: '#ef4444',
};

const EMPTY_STATE_QUOTES = [
  '心若如镜，来者皆照，去者不留。',
  '万念俱息处，真心自现前。',
  '不取一尘，万象皆净。',
  '心若无形，何处不自在。',
  '念起如风，觉知如山。',
  '心若无声，万法皆听。',
  '不求不拒，方得本真。',
  '心若无界，一念通天。',
  '万象皆幻，唯觉不动。',
  '心若无痕，事事皆轻。',
  '不住于相，方见诸相空。',
  '心若无执，处处皆圆满。',
  '念起如潮，觉照如岸。',
  '心若无阴，光自无尽。',
  '不随念走，念自归寂。',
  '心若无缚，万法皆通。',
  '不逐一念，一念自灭。',
  '心若无碍，步步皆通途。',
  '不守一境，一境皆自在。',
  '心若无偏，万物皆平等。',
  '不求圆满，圆满自来。',
  '心若无重，万事皆轻盈。',
  '不逐前尘，前尘如烟散。',
  '心若无形，形形皆自在。',
  '不住当下，当下自明。',
  '心若无我，万法皆一。',
  '不守成见，见见皆新。',
  '心若无畏，天地皆宽。',
  '不执善恶，善恶皆空花。',
  '心若无欲，万物皆清凉。',
  '不随喜怒，喜怒皆幻影。',
  '心若无乱，万象皆秩序。',
  '不求远方，远方在心间。',
  '心若无界，步步皆无边。',
  '不逐光影，光影自分明。',
  '心若无名，万法皆可名。',
  '不守一念，一念皆虚空。',
  '心若无住，处处皆安然。',
  '不求悟道，道已随行。',
  '心若无尘，风来不动。',
  '不逐旧梦，旧梦自消散。',
  '心若无声，万籁皆寂然。',
  '不求真相，真相自显露。',
  '心若无求，所求皆得。',
  '不执成败，成败皆如露。',
  '心若无苦，苦亦成空。',
  '不逐未来，未来自来。',
  '心若无边，念念皆无尽。',
  '不守过往，过往皆如烟。',
];

const STATUS_LABELS: Record<string, string> = {
  pending: '待处理',
  running: '运行中',
  completed: '已完成',
  failed: '失败',
};

interface MiniStatProps {
  title: string;
  value: number;
  suffix?: string;
  prefix?: React.ReactNode;
  color: string;
  loading?: boolean;
  decimals?: number;
  chineseFormat?: boolean;
}

function MiniStat({ title, value, suffix, prefix, color, loading, decimals = 0, chineseFormat = false }: MiniStatProps) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '12px 14px', borderRadius: 10, background: 'var(--color-fill-quaternary)', transition: 'background 0.2s' }}>
      <div
        style={{
          width: 40,
          height: 40,
          borderRadius: 10,
          backgroundColor: `${color}18`,
          color,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          fontSize: 18,
          flexShrink: 0,
        }}
      >
        {prefix}
      </div>
      <div style={{ minWidth: 0 }}>
        <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 2 }}>{title}</div>
        <div style={{ fontSize: 22, fontWeight: 700, color: 'var(--color-text)', lineHeight: 1.2 }}>
          <AnimatedNumber value={loading ? 0 : value} duration={0.8} decimals={decimals} chineseFormat={chineseFormat} />
          {suffix && <span style={{ fontSize: 13, fontWeight: 500, marginLeft: 2 }}>{suffix}</span>}
        </div>
      </div>
    </div>
  );
}

function CompactRow({ name, value, sub, color, barPct }: {
  name: string; value: React.ReactNode; sub: React.ReactNode; color: string; barPct: number;
}) {
  return (
    <div style={{ padding: '10px 0', borderBottom: '1px solid var(--color-border-secondary)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginBottom: 6 }}>
        <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--color-text)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', marginRight: 12 }} title={name}>{name}</span>
        {value}
      </div>
      <div style={{ height: 4, borderRadius: 2, background: 'var(--color-fill-quaternary)', marginBottom: 6 }}>
        <div style={{ height: '100%', width: `${Math.max(barPct, 0)}%`, minWidth: barPct > 0 ? 4 : 0, borderRadius: 2, background: color, transition: 'width 0.6s ease' }} />
      </div>
      <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>{sub}</div>
    </div>
  );
}


interface DashboardProps {
  onBack?: () => void;
}

export function Dashboard({ onBack }: DashboardProps) {
  const { state } = useApp();
  const { message } = App.useApp();
  const { todos, tags, runningTasks } = state;

  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [msgStats, setMsgStats] = useState<FeishuMessageStats | null>(null);
  const [msgStatsError, setMsgStatsError] = useState(false);
  const [timeRange, setTimeRange] = useState<number | 'custom'>(720); // default 30 days
  const [customRange, setCustomRange] = useState<[dayjs.Dayjs, dayjs.Dayjs] | null>(null);

  const loadStats = async (hours?: number) => {
    try {
      setLoading(true);
      const data = await db.getDashboardStats(hours);
      setStats(data);
    } catch {
      message.error('加载统计数据失败');
    } finally {
      setLoading(false);
    }
  };

  const handleTimeRangeChange = (value: number | 'custom') => {
    setTimeRange(value);
    if (value === 'custom') {
      // Don't load yet, wait for date selection
    } else {
      setCustomRange(null);
      loadStats(value);
      loadMsgStats(value);
    }
  };

  const handleCustomRangeChange = (dates: [dayjs.Dayjs, dayjs.Dayjs] | null) => {
    setCustomRange(dates);
    if (dates) {
      const hours = Math.round(dates[1].diff(dates[0], 'hour', true));
      loadStats(hours);
      loadMsgStats(hours);
    }
  };

  useEffect(() => {
    loadStats(720);
    loadMsgStats(720);
  }, []);

  const loadMsgStats = async (hours?: number) => {
    try {
      setMsgStatsError(false);
      const data = await db.getFeishuMessageStats(hours);
      setMsgStats(data);
    } catch {
      setMsgStatsError(true);
    }
  };

  const totalTodos = stats?.total_todos ?? todos.length;

  const statusSegments = [
    { value: stats?.pending_todos ?? 0, color: STATUS_COLORS.pending, label: STATUS_LABELS.pending },
    { value: stats?.running_todos ?? 0, color: STATUS_COLORS.running, label: STATUS_LABELS.running },
    { value: stats?.completed_todos ?? 0, color: STATUS_COLORS.completed, label: STATUS_LABELS.completed },
    { value: stats?.failed_todos ?? 0, color: STATUS_COLORS.failed, label: STATUS_LABELS.failed },
  ].filter((s) => s.value > 0);

  const executorData = stats?.executor_distribution ?? [];
  const executorMax = Math.max(...executorData.map((e) => e.count), 1);

  const tagData = stats?.tag_distribution ?? [];
  const tagMax = Math.max(...tagData.map((t) => t.count), 1);

  const tokenSegments = stats
    ? [
        { value: stats.total_input_tokens, color: '#3b82f6', label: '输入 Tokens' },
        { value: stats.total_output_tokens, color: '#22c55e', label: '输出 Tokens' },
        { value: stats.total_cache_read_tokens, color: '#f59e0b', label: '缓存读' },
        { value: stats.total_cache_creation_tokens, color: '#a78bfa', label: '缓存写' },
      ].filter((s) => s.value > 0)
    : [];

  const trendData = stats?.daily_executions ?? [];
  const runningList = Object.values(runningTasks);

  const successRate = stats && stats.total_executions > 0
    ? (stats.success_executions / stats.total_executions) * 100
    : 0;

  const recentColumns = [
    {
      title: '任务',
      dataIndex: 'todo_id',
      key: 'todo_id',
      render: (_: unknown, record: DashboardStats['recent_executions'][number]) => {
        const todo = todos.find((t) => t.id === record.todo_id);
        return <span style={{ fontWeight: 600 }}>{todo?.title ?? `任务 #${record.todo_id}`}</span>;
      },
    },
    {
      title: '执行器',
      dataIndex: 'executor',
      key: 'executor',
      width: 100,
      render: (v: string | null) => {
        if (!v) return <span>-</span>;
        const opt = getExecutorOption(v);
        return <Tag color={opt.color} style={{ fontWeight: 600 }}>{opt.label}</Tag>;
      },
    },
    {
      title: '触发',
      dataIndex: 'trigger_type',
      key: 'trigger_type',
      width: 70,
      render: (v: string) => (
        <Tag color={v === 'cron' ? '#8b5cf6' : '#6b7280'} style={{ fontSize: 10 }}>
          {v === 'cron' ? 'Cron' : '手动'}
        </Tag>
      ),
    },
    {
      title: '状态',
      dataIndex: 'status',
      key: 'status',
      width: 90,
      render: (v: string) => (
        <Badge
          status={v === 'success' ? 'success' : v === 'failed' ? 'error' : 'processing'}
          text={v === 'success' ? '成功' : v === 'failed' ? '失败' : '运行中'}
        />
      ),
    },
    {
      title: '时间',
      dataIndex: 'started_at',
      key: 'started_at',
      width: 140,
      render: (v: string) => <span style={{ fontSize: 12, color: 'var(--color-text-tertiary)' }}>{formatRelativeTime(v)}</span>,
    },
  ];

  const panels: { key: string; render: () => React.ReactNode }[] = [];

  const ACTIVE_TASKS_MIN_HEIGHT = 148;

  panels.push({
    key: 'active-tasks',
    render: () => (
      <Card
        title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><ThunderboltOutlined /><span>活跃任务</span></div>}
        className="dashboard-card" style={{ borderRadius: 12 }}
        bodyStyle={{ padding: 0 }}
      >
        <div style={{ minHeight: ACTIVE_TASKS_MIN_HEIGHT, padding: '12px 16px' }}>
          {runningList.length > 0 ? (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 10, maxHeight: ACTIVE_TASKS_MIN_HEIGHT - 24, overflow: 'auto' }}>
              {runningList.map((task) => {
                const opt = getExecutorOption(task.executor);
                return (
                  <div
                    key={task.taskId}
                    style={{
                      padding: '10px 14px',
                      borderRadius: 10,
                      background: 'var(--color-bg-elevated)',
                      border: '1px solid var(--color-border-secondary)',
                      display: 'flex',
                      alignItems: 'center',
                      gap: 10,
                      flexShrink: 0,
                    }}
                  >
                    <Badge status="processing" />
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{ fontWeight: 600, fontSize: 13, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                        {task.todoTitle}
                      </div>
                      <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>
                        {opt.label} · {formatRelativeTime(task.startedAt)}
                      </div>
                    </div>
                    <Tag color={opt.color} style={{ fontSize: 11 }}>{opt.label}</Tag>
                  </div>
                );
              })}
            </div>
          ) : (
            <div style={{ textAlign: 'center', color: 'var(--color-text-secondary)', padding: '20px 0' }}>
              <div style={{ fontSize: 32, fontWeight: 700, marginBottom: 8, color: 'var(--color-text)' }}>
                nothing todo
              </div>
              <div style={{ fontSize: 16, fontWeight: 600, marginBottom: 12, color: 'var(--color-text)' }}>
                but everything is todo
              </div>
              <div style={{ fontSize: 13, color: 'var(--color-text-tertiary)' }}>
                {EMPTY_STATE_QUOTES[Math.floor(Math.random() * EMPTY_STATE_QUOTES.length)]}
              </div>
            </div>
          )}
        </div>
      </Card>
    ),
  });

  // TODO: 新增面板暂时注释掉，排查崩溃问题
  panels.push({
    key: 'key-metrics',
    render: () => (
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(140px, 1fr))', gap: 12 }}>
        <MetricCard
          title="今日执行"
          value={stats?.today_executions ?? 0}
          change={stats?.executions_change ?? 0}
          changeLabel="vs昨日"
          prefix={<ThunderboltOutlined />}
          color="#8b5cf6"
          loading={loading && !stats}
          chineseFormat
        />
        <MetricCard
          title="总执行"
          value={stats?.total_executions ?? 0}
          change={stats?.executions_change ?? 0}
          changeLabel="本周"
          prefix={<ThunderboltOutlined />}
          color="#3b82f6"
          loading={loading && !stats}
          chineseFormat
        />
        <MetricCard
          title="成功率"
          value={successRate}
          suffix="%"
          change={stats?.success_rate_change}
          prefix={<CheckCircleOutlined />}
          color="#22c55e"
          loading={loading && !stats}
          decimals={1}
        />
        <MetricCard
          title="总花费"
          value={stats ? Math.round(stats.total_cost_usd) : 0}
          suffix="$"
          change={stats?.cost_change}
          prefix={<DollarOutlined />}
          color="#f59e0b"
          loading={loading && !stats}
        />
        <MetricCard
          title="活跃天数"
          value={stats?.active_days ?? 0}
          prefix={<ClockCircleOutlined />}
          color="#ef4444"
          loading={loading && !stats}
        />
        <MetricCard
          title="连续天数"
          value={stats?.streak_days ?? 0}
          prefix={<TagOutlined />}
          color="#f97316"
          loading={loading && !stats}
        />
        <MetricCard
          title="平均耗时"
          value={stats?.avg_duration_ms ? stats.avg_duration_ms / 1000 : 0}
          suffix="s"
          prefix={<BarChartOutlined />}
          color="#0891b2"
          loading={loading && !stats}
          decimals={1}
        />
      </div>
    ),
  });

  // Highlight Stats - 重点数据展示
  panels.push({
    key: 'highlight-stats',
    render: () => (
      <Card
        title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><TrophyOutlined /><span>亮点数据</span></div>}
        className="dashboard-card" style={{ borderRadius: 12 }}
        bodyStyle={{ padding: '16px 20px' }}
      >
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(140px, 1fr))', gap: 12 }}>
          <HighlightStat
            label="单日峰值"
            value={stats?.peak_daily_executions ?? 0}
            subLabel="历史最高"
            color="#f59e0b"
            icon={<FireOutlined />}
          />
          <HighlightStat
            label="最高产模型"
            value={stats?.top_model ?? '-'}
            subLabel={stats?.top_model_tokens ? `${(stats.top_model_tokens / 10000).toFixed(1)}万 tokens` : ''}
            color="#8b5cf6"
            icon={<ThunderboltOutlined />}
          />
          <HighlightStat
            label="活跃天数"
            value={stats?.active_days ?? 0}
            subLabel="累计活跃"
            color="#22c55e"
            icon={<TrophyOutlined />}
          />
        </div>
      </Card>
    ),
  });

  // Leaderboard - 排行榜
  panels.push({
    key: 'leaderboard',
    render: () => {
      const leaderboardData = stats?.leaderboard ?? [];
      return (
        <Card
          title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><TrophyOutlined /><span>模型排行榜</span></div>}
          className="dashboard-card" style={{ borderRadius: 12 }}
          bodyStyle={{ padding: '16px 20px' }}
        >
          <Leaderboard data={leaderboardData} />
        </Card>
      );
    },
  });

  // Team Members - 团队成员
  panels.push({
    key: 'team-members',
    render: () => {
      const teamData = stats?.leaderboard?.slice(0, 3).map((item, i) => ({
        name: item.name,
        role: 'Member',
        nickname: `Rank #${i + 1}`,
        stats: { tokens: item.tokens, sessions: item.sessions, satisfaction: 95 },
        color: ['#8b5cf6', '#3b82f6', '#22c55e'][i] ?? '#8b5cf6',
      })) ?? [];
      if (teamData.length === 0) {
        return (
          <Card
            title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><UserOutlined /><span>团队成员</span></div>}
            className="dashboard-card" style={{ borderRadius: 12 }}
            bodyStyle={{ padding: '16px 20px' }}
          >
            <div style={{ textAlign: 'center', color: 'var(--color-text-tertiary)', padding: '20px 0' }}>暂无数据</div>
          </Card>
        );
      }
      return (
        <Card
          title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><UserOutlined /><span>团队成员</span></div>}
          className="dashboard-card" style={{ borderRadius: 12 }}
          bodyStyle={{ padding: '16px 20px' }}
        >
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))', gap: 12 }}>
            {teamData.map((member) => (
              <TeamMemberCard key={member.name} {...member} />
            ))}
          </div>
        </Card>
      );
    },
  });

  panels.push({
    key: 'task-stats',
    render: () => (
      <Card
        title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><FileTextOutlined /><span>任务概览</span></div>}
        className="dashboard-card" style={{ borderRadius: 12 }}
        bodyStyle={{ padding: '16px 20px' }}
      >
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 16 }}>
          <MiniStat title="总任务" value={totalTodos} prefix={<FileTextOutlined />} color="#0891b2" loading={loading && !stats} chineseFormat />
          <MiniStat title="运行中" value={stats?.running_todos ?? 0} prefix={<PlayCircleOutlined />} color="#3b82f6" loading={loading && !stats} />
          <MiniStat title="已完成" value={stats?.completed_todos ?? 0} prefix={<CheckCircleOutlined />} color="#22c55e" loading={loading && !stats} />
          <MiniStat title="失败" value={stats?.failed_todos ?? 0} prefix={<CloseCircleOutlined />} color="#ef4444" loading={loading && !stats} />
        </div>
      </Card>
    ),
  });

  panels.push({
    key: 'exec-stats',
    render: () => (
      <Card
        title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><ThunderboltOutlined /><span>执行概览</span></div>}
        className="dashboard-card" style={{ borderRadius: 12 }}
        bodyStyle={{ padding: '16px 20px' }}
      >
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 16 }}>
          <MiniStat title="标签" value={stats?.total_tags ?? tags.length} prefix={<TagOutlined />} color="#8b5cf6" loading={loading && !stats} />
          <MiniStat title="定时" value={stats?.scheduled_todos ?? 0} prefix={<ClockCircleOutlined />} color="#f59e0b" loading={loading && !stats} />
          <MiniStat title="总执行" value={stats?.total_executions ?? 0} prefix={<ThunderboltOutlined />} color="#0d9488" loading={loading && !stats} chineseFormat />
          <MiniStat title="总花费" value={stats ? Math.round(stats.total_cost_usd) : 0} suffix="$" prefix={<DollarOutlined />} color="#dc2626" loading={loading && !stats} />
        </div>
      </Card>
    ),
  });

  panels.push({
    key: 'status-chart',
    render: () => (
      <Card
        title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><BarChartOutlined /><span>任务状态分布</span></div>}
        className="dashboard-card" style={{ borderRadius: 12 }}
        bodyStyle={{ padding: '16px 20px' }}
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
    ),
  });

  const triggerData = stats?.trigger_type_distribution ?? [];
  const totalTrigger = triggerData.reduce((sum, t) => sum + t.count, 0);
  const cronCount = triggerData.find((t) => t.trigger_type === 'cron')?.count ?? 0;
  const autoRate = totalTrigger > 0 ? (cronCount / totalTrigger) * 100 : 0;

  const TRIGGER_LABELS: Record<string, string> = {
    manual: '手动',
    cron: '定时',
    slash_command: '命令',
    default_response: '默认回复',
  };

  const TRIGGER_COLORS: Record<string, string> = {
    manual: '#3b82f6',
    cron: '#8b5cf6',
    slash_command: '#f59e0b',
    default_response: '#22c55e',
  };

  panels.push({
    key: 'trigger-source',
    render: () => (
      <Card
        title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><BarChartOutlined /><span>触发来源分析</span></div>}
        className="dashboard-card" style={{ borderRadius: 12 }}
        bodyStyle={{ padding: '16px 20px' }}
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
    ),
  });

  panels.push({
    key: 'executor-chart',
    render: () => (
      <Card
        title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><BarChartOutlined /><span>执行器分布</span></div>}
        className="dashboard-card" style={{ borderRadius: 12 }}
        bodyStyle={{ padding: '8px 16px' }}
      >
        {executorData.length > 0 ? (
          executorData.map((e) => {
            const opt = getExecutorOption(e.executor);
            const execRate = e.execution_count > 0 ? ((e.success_count / e.execution_count) * 100).toFixed(0) : '0';
            return (
              <CompactRow
                key={e.executor}
                name={opt.label}
                value={<span style={{ fontSize: 18, fontWeight: 700, color: opt.color }}>{e.count}</span>}
                color={opt.color}
                barPct={(e.count / executorMax) * 100}
                sub={
                  <span>
                    执行 <strong style={{ color: 'var(--color-text)' }}>{e.execution_count}</strong> 次
                    <span style={{ margin: '0 6px' }}>·</span>
                    成功率 <strong style={{ color: '#22c55e' }}>{execRate}%</strong>
                    {e.total_cost_usd > 0 && (
                      <>
                        <span style={{ margin: '0 6px' }}>·</span>
                        <span style={{ color: '#f59e0b', fontWeight: 600 }}>${Math.round(e.total_cost_usd)}</span>
                      </>
                    )}
                  </span>
                }
              />
            );
          })
        ) : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无数据" />
        )}
      </Card>
    ),
  });

  const durationData = stats?.executor_duration_stats ?? [];
  const durationMax = Math.max(...durationData.map((d) => d.avg_duration_ms), 1);

  panels.push({
    key: 'executor-duration',
    render: () => (
      <Card
        title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><ThunderboltOutlined /><span>执行器平均耗时</span></div>}
        className="dashboard-card" style={{ borderRadius: 12 }}
        bodyStyle={{ padding: '8px 16px' }}
      >
        {durationData.length > 0 ? (
          durationData.map((d) => {
            const opt = getExecutorOption(d.executor);
            const seconds = d.avg_duration_ms > 1000
              ? (d.avg_duration_ms / 1000).toFixed(1) + 's'
              : d.avg_duration_ms.toFixed(0) + 'ms';
            return (
              <CompactRow
                key={d.executor}
                name={opt.label}
                value={<span style={{ fontSize: 18, fontWeight: 700, color: opt.color }}>{seconds}</span>}
                color={opt.color}
                barPct={(d.avg_duration_ms / durationMax) * 100}
                sub={
                  <span>
                    执行 <strong style={{ color: 'var(--color-text)' }}>{d.execution_count}</strong> 次
                  </span>
                }
              />
            );
          })
        ) : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无数据" />
        )}
      </Card>
    ),
  });

  panels.push({
    key: 'tag-chart',
    render: () => (
      <Card
        title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><TagOutlined /><span>标签分布</span></div>}
        className="dashboard-card" style={{ borderRadius: 12 }}
        bodyStyle={{ padding: '8px 16px' }}
      >
        {tagData.length > 0 ? (
          tagData.map((t) => {
            const execRate = t.execution_count > 0 ? ((t.success_count / t.execution_count) * 100).toFixed(0) : '0';
            return (
              <CompactRow
                key={t.tag_id}
                name={t.tag_name}
                value={<span style={{ fontSize: 18, fontWeight: 700, color: t.tag_color }}>{t.count}</span>}
                color={t.tag_color}
                barPct={(t.count / tagMax) * 100}
                sub={
                  <span>
                    执行 <strong style={{ color: 'var(--color-text)' }}>{t.execution_count}</strong> 次
                    <span style={{ margin: '0 6px' }}>·</span>
                    成功率 <strong style={{ color: '#22c55e' }}>{execRate}%</strong>
                    {t.total_cost_usd > 0 && (
                      <>
                        <span style={{ margin: '0 6px' }}>·</span>
                        <span style={{ color: '#f59e0b', fontWeight: 600 }}>${Math.round(t.total_cost_usd)}</span>
                      </>
                    )}
                  </span>
                }
              />
            );
          })
        ) : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无标签数据" />
        )}
      </Card>
    ),
  });

  panels.push({
    key: 'token-chart',
    render: () => (
      <Card
        title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><ThunderboltOutlined /><span>Token 消耗</span></div>}
        className="dashboard-card" style={{ borderRadius: 12 }}
        bodyStyle={{ padding: '16px 20px' }}
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
    ),
  });

  panels.push({
    key: 'trend-chart',
    render: () => (
      <Card
        title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><BarChartOutlined /><span>执行趋势</span></div>}
        className="dashboard-card" style={{ borderRadius: 12 }}
        bodyStyle={{ padding: '16px 20px' }}
      >
        <TrendChart data={trendData} height={180} />
      </Card>
    ),
  });

  panels.push({
    key: 'contribution-heatmap',
    render: () => (
      <Card
        title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><BarChartOutlined /><span>活动热力图</span></div>}
        className="dashboard-card" style={{ borderRadius: 12 }}
        bodyStyle={{ padding: '16px 20px' }}
      >
        <ContributionHeatmap data={stats?.daily_executions ?? []} />
      </Card>
    ),
  });

  const modelData = stats?.model_distribution ?? [];
  const modelCountMax = Math.max(...modelData.map((m) => m.count), 1);
  const modelTokenMax = Math.max(...modelData.map((m) => m.total_input_tokens + m.total_output_tokens), 1);

  const MODEL_COLORS = ['#8b5cf6', '#3b82f6', '#22c55e', '#f59e0b', '#ef4444', '#0891b2', '#ec4899', '#6366f1'];

  panels.push({
    key: 'model-task-chart',
    render: () => (
      <Card
        title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><BarChartOutlined /><span>模型任务分布</span></div>}
        className="dashboard-card" style={{ borderRadius: 12 }}
        bodyStyle={{ padding: '8px 16px' }}
      >
        {modelData.length > 0 ? (
          modelData.map((m, i) => {
            const rate = m.execution_count > 0 ? ((m.success_count / m.execution_count) * 100).toFixed(0) : '0';
            return (
              <CompactRow
                key={m.model}
                name={m.model}
                value={<span style={{ fontSize: 18, fontWeight: 700, color: MODEL_COLORS[i % MODEL_COLORS.length] }}>{m.count}</span>}
                color={MODEL_COLORS[i % MODEL_COLORS.length]}
                barPct={(m.count / modelCountMax) * 100}
                sub={
                  <span>
                    执行 <strong style={{ color: 'var(--color-text)' }}>{m.execution_count}</strong> 次
                    <span style={{ margin: '0 6px' }}>·</span>
                    成功率 <strong style={{ color: '#22c55e' }}>{rate}%</strong>
                  </span>
                }
              />
            );
          })
        ) : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无模型数据" />
        )}
      </Card>
    ),
  });

  panels.push({
    key: 'model-token-chart',
    render: () => (
      <Card
        title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><ThunderboltOutlined /><span>模型推理统计</span></div>}
        className="dashboard-card" style={{ borderRadius: 12 }}
        bodyStyle={{ padding: '8px 16px' }}
      >
        {modelData.length > 0 ? (
          modelData.map((m, i) => {
            const outputRate = m.total_input_tokens > 0 ? (m.total_output_tokens / m.total_input_tokens) * 100 : 0;
            const costDisplay = m.total_cost_usd < 10000 ? `$${m.total_cost_usd.toFixed(2)}` : `$${(m.total_cost_usd / 10000).toFixed(2)}万`;
            return (
              <CompactRow
                key={m.model}
                name={m.model}
                value={<span style={{ fontSize: 16, fontWeight: 700, color: MODEL_COLORS[i % MODEL_COLORS.length] }}>{(m.total_input_tokens / 10000).toFixed(1)}万</span>}
                color={MODEL_COLORS[i % MODEL_COLORS.length]}
                barPct={(m.total_input_tokens / modelTokenMax) * 100}
                sub={
                  <span>
                    推理输入 <strong style={{ color: '#3b82f6' }}>{(m.total_input_tokens / 10000).toFixed(1)}万</strong>
                    <span style={{ margin: '0 4px' }}>·</span>
                    成本 <strong style={{ color: '#f59e0b' }}>{costDisplay}</strong>
                    <span style={{ margin: '0 4px' }}>·</span>
                    输出率 <strong style={{ color: '#22c55e' }}>{outputRate.toFixed(1)}%</strong>
                  </span>
                }
              />
            );
          })
        ) : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无模型数据" />
        )}
      </Card>
    ),
  });

  const cacheData = stats?.model_cache_stats ?? [];
  const cacheRateMax = Math.max(...cacheData.map((c) => c.cache_hit_rate), 1);

  panels.push({
    key: 'model-cache',
    render: () => (
      <Card
        title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><BarChartOutlined /><span>缓存效率</span></div>}
        className="dashboard-card" style={{ borderRadius: 12 }}
        bodyStyle={{ padding: '8px 16px' }}
      >
        {cacheData.length > 0 ? (
          cacheData.map((c) => {
            const color = c.cache_hit_rate > 50 ? '#22c55e' : c.cache_hit_rate > 20 ? '#f59e0b' : '#ef4444';
            return (
              <CompactRow
                key={c.model}
                name={c.model}
                value={<span style={{ fontSize: 16, fontWeight: 700, color }}>{c.cache_hit_rate.toFixed(1)}%</span>}
                color={color}
                barPct={(c.cache_hit_rate / cacheRateMax) * 100}
                sub={
                  <span>
                    缓存读 <strong style={{ color: '#22c55e' }}>{(c.total_cache_read_tokens / 10000).toFixed(1)}万</strong>
                    <span style={{ margin: '0 4px' }}>·</span>
                    输入 <strong style={{ color: '#3b82f6' }}>{(c.total_input_tokens / 10000).toFixed(1)}万</strong>
                  </span>
                }
              />
            );
          })
        ) : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无缓存数据" />
        )}
      </Card>
    ),
  });

  panels.push({
    key: 'token-trend-chart',
    render: () => {
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
          bodyStyle={{ padding: '16px 20px' }}
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
    },
  });

  panels.push({
    key: 'share-card',
    render: () => <ShareCard />,
  });

  panels.push({
    key: 'inference-stats',
    render: () => {
      const totalInput = stats?.total_input_tokens ?? 0;
      const totalOutput = stats?.total_output_tokens ?? 0;
      const totalCost = stats?.total_cost_usd ?? 0;
      const outputRate = totalInput > 0 ? (totalOutput / totalInput) * 100 : 0;

      return (
        <Card
          title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><ThunderboltOutlined /><span>推理统计</span></div>}
          className="dashboard-card" style={{ borderRadius: 12 }}
          bodyStyle={{ padding: '16px 20px' }}
        >
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 12 }}>
            <div style={{ padding: '12px 14px', borderRadius: 10, background: '#3b82f610', textAlign: 'center' }}>
              <div style={{ fontSize: 11, color: 'var(--color-text-secondary)', marginBottom: 4 }}>推理输入</div>
              <div style={{ fontSize: 20, fontWeight: 700, color: '#3b82f6' }}>
                <AnimatedNumber value={loading ? 0 : totalInput / 10000} duration={1.2} decimals={2} suffix="万" />
              </div>
            </div>
            <div style={{ padding: '12px 14px', borderRadius: 10, background: '#22c55e10', textAlign: 'center' }}>
              <div style={{ fontSize: 11, color: 'var(--color-text-secondary)', marginBottom: 4 }}>推理输出</div>
              <div style={{ fontSize: 20, fontWeight: 700, color: '#22c55e' }}>
                <AnimatedNumber value={loading ? 0 : totalOutput / 10000} duration={1.2} decimals={2} suffix="万" />
              </div>
            </div>
            <div style={{ padding: '12px 14px', borderRadius: 10, background: '#f59e0b10', textAlign: 'center' }}>
              <div style={{ fontSize: 11, color: 'var(--color-text-secondary)', marginBottom: 4 }}>成本</div>
              <div style={{ fontSize: 20, fontWeight: 700, color: '#f59e0b' }}>
                <AnimatedNumber value={loading ? 0 : totalCost} duration={1.2} prefix="$" decimals={2} />
              </div>
            </div>
            <div style={{ padding: '12px 14px', borderRadius: 10, background: '#8b5cf610', textAlign: 'center' }}>
              <div style={{ fontSize: 11, color: 'var(--color-text-secondary)', marginBottom: 4 }}>输出率</div>
              <div style={{ fontSize: 20, fontWeight: 700, color: '#8b5cf6' }}>
                <AnimatedNumber value={loading ? 0 : outputRate} duration={1.2} decimals={1} suffix="%" />
              </div>
            </div>
          </div>
        </Card>
      );
    },
  });

  const processingRate = msgStats && msgStats.total_messages > 0
    ? (msgStats.processed / msgStats.total_messages) * 100
    : 0;

  panels.push({
    key: 'message-stats',
    render: () => (
      <Card
        title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><MessageOutlined /><span>消息记录分析</span></div>}
        className="dashboard-card" style={{ borderRadius: 12 }}
        bodyStyle={{ padding: '16px 20px' }}
      >
        {msgStats ? (
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 16 }}>
            <MiniStat title="消息总量" value={msgStats.total_messages} prefix={<MessageOutlined />} color="#3b82f6" loading={false} chineseFormat />
            <MiniStat title="已处理" value={msgStats.processed} prefix={<CheckCircleOutlined />} color="#22c55e" loading={false} chineseFormat />
            <MiniStat title="处理率" value={processingRate} prefix={<BarChartOutlined />} color="#8b5cf6" decimals={1} suffix="%" loading={false} />
            <MiniStat title="触发任务" value={msgStats.triggered_todos} prefix={<ThunderboltOutlined />} color="#f59e0b" loading={false} chineseFormat />
          </div>
        ) : msgStatsError ? (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="消息数据加载失败" />
        ) : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无消息数据" />
        )}
      </Card>
    ),
  });

  panels.push({
    key: 'overview-card',
    render: () => (
      <Card
        title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><ThunderboltOutlined /><span>执行概览</span></div>}
        className="dashboard-card" style={{ borderRadius: 12 }}
        bodyStyle={{ padding: '16px 20px' }}
      >
        <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
          <div>
            <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 6 }}>
              <span style={{ fontSize: 13, color: 'var(--color-text-secondary)' }}>成功率</span>
              <span style={{ fontSize: 15, fontWeight: 700, color: '#22c55e' }}><AnimatedNumber value={successRate} duration={1.2} decimals={1} suffix="%" /></span>
            </div>
            <div style={{ height: 6, borderRadius: 3, background: 'var(--color-fill-quaternary)', overflow: 'hidden' }}>
              <div style={{ height: '100%', width: `${successRate}%`, borderRadius: 3, background: 'linear-gradient(90deg, #22c55e, #4ade80)', transition: 'width 0.8s ease' }} />
            </div>
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
            <div style={{ padding: '10px 14px', borderRadius: 10, background: '#22c55e10' }}>
              <div style={{ fontSize: 11, color: 'var(--color-text-secondary)', marginBottom: 2 }}>成功执行</div>
              <div style={{ fontSize: 18, fontWeight: 700, color: '#22c55e' }}><AnimatedNumber value={stats?.success_executions ?? 0} duration={0.8} chineseFormat /></div>
            </div>
            <div style={{ padding: '10px 14px', borderRadius: 10, background: '#ef444410' }}>
              <div style={{ fontSize: 11, color: 'var(--color-text-secondary)', marginBottom: 2 }}>失败执行</div>
              <div style={{ fontSize: 18, fontWeight: 700, color: '#ef4444' }}><AnimatedNumber value={stats?.failed_executions ?? 0} duration={0.8} chineseFormat /></div>
            </div>
            <div style={{ padding: '10px 14px', borderRadius: 10, background: 'var(--color-fill-quaternary)' }}>
              <div style={{ fontSize: 11, color: 'var(--color-text-secondary)', marginBottom: 2 }}>平均耗时</div>
              <div style={{ fontSize: 18, fontWeight: 700, color: 'var(--color-text)' }}>
                {stats && stats.avg_duration_ms > 0 ? <AnimatedNumber value={stats.avg_duration_ms / 1000} duration={1.2} decimals={1} suffix="s" /> : '-'}
              </div>
            </div>
            <div style={{ padding: '10px 14px', borderRadius: 10, background: '#f59e0b10' }}>
              <div style={{ fontSize: 11, color: 'var(--color-text-secondary)', marginBottom: 2 }}>总花费</div>
              <div style={{ fontSize: 18, fontWeight: 700, color: '#f59e0b' }}><AnimatedNumber value={stats ? Math.round(stats.total_cost_usd) : 0} duration={1.2} prefix="$" /></div>
            </div>
          </div>
        </div>
      </Card>
    ),
  });

  return (
    <div style={{ height: '100%', overflow: 'auto', padding: '16px 20px', background: 'var(--color-bg-layout)' }}>
      <style>{`
        .dashboard-card { transition: border-color 0.2s, box-shadow 0.2s; }
        .dashboard-card:hover { border-color: var(--color-border); box-shadow: 0 2px 12px rgba(0,0,0,0.08); }
      `}</style>
      {onBack && (
        <Button
          type="text"
          size="small"
          icon={<LeftOutlined />}
          onClick={onBack}
          style={{ marginBottom: 12, marginLeft: -4 }}
          aria-label="返回"
        />
      )}
      {/* Time Range Selector */}
      <div style={{ marginBottom: 16, display: 'flex', gap: 12, alignItems: 'center', flexWrap: 'wrap' }}>
        <Segmented
          value={timeRange}
          onChange={(value) => handleTimeRangeChange(value as number | 'custom')}
          options={TIME_RANGE_OPTIONS}
        />
        {timeRange === 'custom' && (
          <DatePicker.RangePicker
            value={customRange}
            onChange={(dates) => handleCustomRangeChange(dates as [dayjs.Dayjs, dayjs.Dayjs] | null)}
            showTime={{ format: 'HH:mm' }}
            format="YYYY-MM-DD HH:mm"
            style={{ minWidth: 280 }}
          />
        )}
      </div>
      <Masonry
        columns={{ xs: 1, sm: 1, md: 2, lg: 2, xl: 3 }}
        gutter={[16, 16]}
        items={panels.map(p => ({ key: p.key, data: p }))}
        itemRender={(item) => item.data.render()}
        fresh
      />
      <Card
        title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><ThunderboltOutlined /><span>最近执行记录</span></div>}
        style={{ borderRadius: 12, marginTop: 16 }}
        bodyStyle={{ padding: '16px 20px' }}
      >
        {stats && stats.recent_executions.length > 0 ? (
          <Table columns={recentColumns} dataSource={stats.recent_executions} rowKey="id" pagination={false} size="small" scroll={{ x: 'max-content' }} />
        ) : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无执行记录" />
        )}
      </Card>
    </div>
  );
}
