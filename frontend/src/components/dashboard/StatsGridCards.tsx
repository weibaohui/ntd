import { Card, Empty } from 'antd';
import {
  ThunderboltOutlined, CheckCircleOutlined, DollarOutlined,
  ClockCircleOutlined, TagOutlined, BarChartOutlined,
  FireOutlined, TrophyOutlined, MessageOutlined,
  FileTextOutlined, PlayCircleOutlined, CloseCircleOutlined,
} from '@ant-design/icons';
import { MetricCard, HighlightStat } from './EnhancedCards';
import { MiniStat } from './MiniStat';
import { AnimatedNumber } from '../AnimatedNumber';
import type { DashboardStats, FeishuMessageStats } from '../../types';

interface KeyMetricsCardProps {
  stats: DashboardStats | null;
  loading: boolean;
  successRate: number;
}

export function KeyMetricsCard({ stats, loading, successRate }: KeyMetricsCardProps) {
  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><BarChartOutlined /><span>关键指标</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '16px 20px' } }}
    >
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(140px, 1fr))', gap: 12 }}>
        <MetricCard
          title="今日执行"
          value={stats?.today_executions ?? 0}
          change={stats?.executions_change}
          changeLabel="vs昨日"
          prefix={<ThunderboltOutlined />}
          color="#8b5cf6"
          loading={loading && !stats}
          chineseFormat
        />
        <MetricCard
          title="总执行"
          value={stats?.total_executions ?? 0}
          change={stats?.executions_change}
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
    </Card>
  );
}

interface HighlightStatsCardProps {
  stats: DashboardStats | null;
}

export function HighlightStatsCard({ stats }: HighlightStatsCardProps) {
  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><TrophyOutlined /><span>亮点数据</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '16px 20px' } }}
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
  );
}

interface TaskStatsCardProps {
  stats: DashboardStats | null;
  loading: boolean;
  totalTodos: number;
}

export function TaskStatsCard({ stats, loading, totalTodos }: TaskStatsCardProps) {
  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><FileTextOutlined /><span>任务概览</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '16px 20px' } }}
    >
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 16 }}>
        <MiniStat title="总任务" value={totalTodos} prefix={<FileTextOutlined />} color="#0891b2" loading={loading && !stats} chineseFormat />
        <MiniStat title="运行中" value={stats?.running_todos ?? 0} prefix={<PlayCircleOutlined />} color="#3b82f6" loading={loading && !stats} />
        <MiniStat title="已完成" value={stats?.completed_todos ?? 0} prefix={<CheckCircleOutlined />} color="#22c55e" loading={loading && !stats} />
        <MiniStat title="失败" value={stats?.failed_todos ?? 0} prefix={<CloseCircleOutlined />} color="#ef4444" loading={loading && !stats} />
      </div>
    </Card>
  );
}

interface ExecStatsCardProps {
  stats: DashboardStats | null;
  loading: boolean;
  tagsLength: number;
}

export function ExecStatsCard({ stats, loading, tagsLength }: ExecStatsCardProps) {
  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><ThunderboltOutlined /><span>执行概览</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '16px 20px' } }}
    >
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 16 }}>
        <MiniStat title="标签" value={stats?.total_tags ?? tagsLength} prefix={<TagOutlined />} color="#8b5cf6" loading={loading && !stats} />
        <MiniStat title="定时" value={stats?.scheduled_todos ?? 0} prefix={<ClockCircleOutlined />} color="#f59e0b" loading={loading && !stats} />
        <MiniStat title="总执行" value={stats?.total_executions ?? 0} prefix={<ThunderboltOutlined />} color="#0d9488" loading={loading && !stats} chineseFormat />
        <MiniStat title="总花费" value={stats ? Math.round(stats.total_cost_usd) : 0} suffix="$" prefix={<DollarOutlined />} color="#dc2626" loading={loading && !stats} />
      </div>
    </Card>
  );
}

interface InferenceStatsCardProps {
  stats: DashboardStats | null;
  loading: boolean;
}

export function InferenceStatsCard({ stats, loading }: InferenceStatsCardProps) {
  const totalInput = stats?.total_input_tokens ?? 0;
  const totalOutput = stats?.total_output_tokens ?? 0;
  const totalCost = stats?.total_cost_usd ?? 0;
  const outputRate = totalInput > 0 ? (totalOutput / totalInput) * 100 : 0;

  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><ThunderboltOutlined /><span>推理统计</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '16px 20px' } }}
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
}

interface OverviewCardProps {
  stats: DashboardStats | null;
  successRate: number;
}

export function OverviewCard({ stats, successRate }: OverviewCardProps) {
  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><ThunderboltOutlined /><span>执行概览</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '16px 20px' } }}
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
  );
}

interface MessageStatsCardProps {
  msgStats: FeishuMessageStats | null;
  msgStatsError: boolean;
  processingRate: number;
}

export function MessageStatsCard({ msgStats, msgStatsError, processingRate }: MessageStatsCardProps) {
  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><MessageOutlined /><span>消息记录分析</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '16px 20px' } }}
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
  );
}

interface BackupStatsCardProps {
  stats: DashboardStats | null;
  loading: boolean;
}

export function BackupStatsCard({ stats, loading }: BackupStatsCardProps) {
  const backupStats = stats?.backup_stats;

  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><FileTextOutlined /><span>备份统计</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '16px 20px' } }}
    >
      {backupStats ? (
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 16 }}>
          <MiniStat title="数据库备份" value={backupStats.database.file_count} suffix="个" prefix={<FileTextOutlined />} color="#3b82f6" loading={loading} />
          <MiniStat title="Todo 备份" value={backupStats.todo.file_count} suffix="个" prefix={<FileTextOutlined />} color="#22c55e" loading={loading} />
          <MiniStat title="Skills 备份" value={backupStats.skills.file_count} suffix="个" prefix={<FileTextOutlined />} color="#f59e0b" loading={loading} />
          <div style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '12px 14px', borderRadius: 10, background: 'var(--color-fill-quaternary)' }}>
            <div style={{ width: 40, height: 40, borderRadius: 10, backgroundColor: '#8b5f616', color: '#8b5cf6', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 18 }}>
              <FileTextOutlined />
            </div>
            <div style={{ minWidth: 0 }}>
              <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 2 }}>总大小</div>
              <div style={{ fontSize: 22, fontWeight: 700, color: 'var(--color-text)', lineHeight: 1.2 }}>
                {backupStats.total_size_formatted || '0 B'}
              </div>
            </div>
          </div>
        </div>
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无备份数据" />
      )}
    </Card>
  );
}
