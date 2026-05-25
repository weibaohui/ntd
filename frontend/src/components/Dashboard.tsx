import { useEffect, useState } from 'react';
import { Card, Table, Badge, Tag, Empty, Masonry, App, Button } from 'antd';
import {
  LeftOutlined,
  ThunderboltOutlined,
} from '@ant-design/icons';
import dayjs from 'dayjs';
import { useApp } from '../hooks/useApp';
import * as db from '../utils/database';
import { getExecutorOption } from '../types';
import type { DashboardStats, FeishuMessageStats } from '../types';
import { formatRelativeTime } from '../utils/datetime';
import {
  KeyMetricsCard, HighlightStatsCard, TaskStatsCard, ExecStatsCard,
  InferenceStatsCard, OverviewCard, MessageStatsCard, BackupStatsCard,
} from './dashboard/StatsGridCards';
import {
  ExecutorChartCard, ExecutorDurationCard, TagChartCard,
  ModelTaskChartCard, ModelTokenChartCard, ModelCacheCard, SkillsStatsCard,
} from './dashboard/DistributionCards';
import {
  StatusChartCard, TokenChartCard, TriggerSourceCard,
  TrendChartCard, ContributionHeatmapCard, TokenTrendChartCard,
} from './dashboard/ChartCards';
import { ActiveTasksCard, LeaderboardCard, ShareCardPanel, TimeRangeSelector } from './dashboard/SpecialCards';

export function Dashboard({ onBack }: { onBack?: () => void }) {
  const { state } = useApp();
  const { message } = App.useApp();
  const { todos, tags, runningTasks } = state;

  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [msgStats, setMsgStats] = useState<FeishuMessageStats | null>(null);
  const [msgStatsError, setMsgStatsError] = useState(false);
  const [timeRange, setTimeRange] = useState<number | 'custom'>(720);
  const [customRange, setCustomRange] = useState<[dayjs.Dayjs, dayjs.Dayjs] | null>(null);

  const totalTodos = stats?.total_todos ?? todos.length;
  const successRate = stats && stats.total_executions > 0
    ? (stats.success_executions / stats.total_executions) * 100
    : 0;

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

  const loadMsgStats = async (hours?: number) => {
    try {
      setMsgStatsError(false);
      const data = await db.getFeishuMessageStats(hours);
      setMsgStats(data);
    } catch {
      setMsgStatsError(true);
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

  const processingRate = msgStats && msgStats.total_messages > 0
    ? (msgStats.processed / msgStats.total_messages) * 100
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

  const panels: { key: string; render: () => React.ReactNode }[] = [
    { key: 'active-tasks', render: () => <ActiveTasksCard runningTasks={Object.values(runningTasks)} /> },
    { key: 'key-metrics', render: () => <KeyMetricsCard stats={stats} loading={loading} successRate={successRate} /> },
    { key: 'highlight-stats', render: () => <HighlightStatsCard stats={stats} /> },
    { key: 'leaderboard', render: () => <LeaderboardCard leaderboard={stats?.leaderboard ?? []} /> },
    { key: 'task-stats', render: () => <TaskStatsCard stats={stats} loading={loading} totalTodos={totalTodos} /> },
    { key: 'exec-stats', render: () => <ExecStatsCard stats={stats} loading={loading} tagsLength={tags.length} /> },
    { key: 'status-chart', render: () => <StatusChartCard stats={stats} totalTodos={totalTodos} /> },
    { key: 'trigger-source', render: () => <TriggerSourceCard stats={stats} /> },
    { key: 'executor-chart', render: () => <ExecutorChartCard stats={stats} /> },
    { key: 'executor-duration', render: () => <ExecutorDurationCard stats={stats} /> },
    { key: 'tag-chart', render: () => <TagChartCard stats={stats} /> },
    { key: 'token-chart', render: () => <TokenChartCard stats={stats} /> },
    { key: 'trend-chart', render: () => <TrendChartCard stats={stats} /> },
    { key: 'contribution-heatmap', render: () => <ContributionHeatmapCard stats={stats} /> },
    { key: 'model-task-chart', render: () => <ModelTaskChartCard stats={stats} /> },
    { key: 'model-token-chart', render: () => <ModelTokenChartCard stats={stats} /> },
    { key: 'model-cache', render: () => <ModelCacheCard stats={stats} /> },
    { key: 'token-trend-chart', render: () => <TokenTrendChartCard stats={stats} /> },
    { key: 'inference-stats', render: () => <InferenceStatsCard stats={stats} loading={loading} /> },
    { key: 'overview-card', render: () => <OverviewCard stats={stats} successRate={successRate} /> },
    { key: 'message-stats', render: () => <MessageStatsCard msgStats={msgStats} msgStatsError={msgStatsError} processingRate={processingRate} /> },
    { key: 'skills-stats', render: () => <SkillsStatsCard stats={stats} loading={loading} /> },
    { key: 'backup-stats', render: () => <BackupStatsCard stats={stats} loading={loading} /> },
    { key: 'share-card', render: () => <ShareCardPanel /> },
  ];

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
      <TimeRangeSelector
        timeRange={timeRange}
        customRange={customRange}
        onTimeRangeChange={handleTimeRangeChange}
        onCustomRangeChange={handleCustomRangeChange}
      />
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
        styles={{ body: { padding: '16px 20px' } }}
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
