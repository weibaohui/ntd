import { Card, Badge, Tag, Segmented, DatePicker } from 'antd';
import { ThunderboltOutlined, TrophyOutlined } from '@ant-design/icons';
import type { Dayjs } from 'dayjs';
import { Leaderboard } from './EnhancedCards';
import { ShareCard } from '../ShareCard';
import { RANDOM_QUOTE, TIME_RANGE_OPTIONS } from './constants';
import { getExecutorOption } from '../../types';
import { formatRelativeTime } from '../../utils/datetime';
import type { RunningTask } from '../../types';

interface ActiveTasksCardProps {
  runningTasks: RunningTask[];
}

export function ActiveTasksCard({ runningTasks }: ActiveTasksCardProps) {
  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><ThunderboltOutlined /><span>活跃任务</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: 0 } }}
    >
      <div style={{ minHeight: 148, padding: '12px 16px' }}>
        {runningTasks.length > 0 ? (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 10, maxHeight: 124, overflow: 'auto' }}>
            {runningTasks.map((task) => {
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
              {RANDOM_QUOTE}
            </div>
          </div>
        )}
      </div>
    </Card>
  );
}

export function LeaderboardCard({ leaderboard }: { leaderboard: any[] }) {
  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><TrophyOutlined /><span>模型排行榜</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '16px 20px' } }}
    >
      <Leaderboard data={leaderboard} />
    </Card>
  );
}

export function ShareCardPanel() {
  return <ShareCard />;
}

interface TimeRangeSelectorProps {
  timeRange: number | 'custom';
  customRange: [Dayjs, Dayjs] | null;
  onTimeRangeChange: (value: number | 'custom') => void;
  onCustomRangeChange: (dates: [Dayjs, Dayjs] | null) => void;
}

export function TimeRangeSelector({ timeRange, customRange, onTimeRangeChange, onCustomRangeChange }: TimeRangeSelectorProps) {
  return (
    <div style={{ marginBottom: 16, display: 'flex', gap: 12, alignItems: 'center', flexWrap: 'wrap' }}>
      <Segmented
        value={timeRange}
        onChange={(value) => onTimeRangeChange(value as number | 'custom')}
        options={TIME_RANGE_OPTIONS}
      />
      {timeRange === 'custom' && (
        <DatePicker.RangePicker
          value={customRange}
          onChange={(dates) => onCustomRangeChange(dates as [Dayjs, Dayjs] | null)}
          showTime={{ format: 'HH:mm' }}
          format="YYYY-MM-DD HH:mm"
          style={{ minWidth: 280 }}
        />
      )}
    </div>
  );
}
