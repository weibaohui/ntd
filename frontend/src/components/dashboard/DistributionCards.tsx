import { Card, Empty } from 'antd';
import { BarChartOutlined, ThunderboltOutlined, TagOutlined, ClockCircleOutlined, CheckCircleOutlined } from '@ant-design/icons';
import { CompactRow } from './CompactRow';
import { MetricCard } from './EnhancedCards';
import { MODEL_COLORS } from './constants';
import { getExecutorOption } from '../../types';
import type { DashboardStats } from '../../types';

interface BaseCardProps {
  stats: DashboardStats | null;
}

export function ExecutorChartCard({ stats }: BaseCardProps) {
  const executorData = stats?.executor_distribution ?? [];
  const executorMax = Math.max(...executorData.map((e) => e.count), 1);

  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><BarChartOutlined /><span>执行器分布</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '8px 16px' } }}
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
  );
}

export function ExecutorDurationCard({ stats }: BaseCardProps) {
  const durationData = stats?.executor_duration_stats ?? [];
  const durationMax = Math.max(...durationData.map((d) => d.avg_duration_ms), 1);

  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><ThunderboltOutlined /><span>执行器平均耗时</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '8px 16px' } }}
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
  );
}

export function TagChartCard({ stats }: BaseCardProps) {
  const tagData = stats?.tag_distribution ?? [];
  const tagMax = Math.max(...tagData.map((t) => t.count), 1);

  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><TagOutlined /><span>标签分布</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '8px 16px' } }}
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
  );
}

export function ModelTaskChartCard({ stats }: BaseCardProps) {
  const modelData = stats?.model_distribution ?? [];
  const modelCountMax = Math.max(...modelData.map((m) => m.count), 1);

  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><BarChartOutlined /><span>模型任务分布</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '8px 16px' } }}
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
  );
}

export function ModelTokenChartCard({ stats }: BaseCardProps) {
  const modelData = stats?.model_distribution ?? [];
  const modelTokenMax = Math.max(...modelData.map((m) => m.total_input_tokens + m.total_output_tokens), 1);

  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><ThunderboltOutlined /><span>模型推理统计</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '8px 16px' } }}
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
  );
}

export function ModelCacheCard({ stats }: BaseCardProps) {
  const cacheData = stats?.model_cache_stats ?? [];
  const cacheRateMax = Math.max(...cacheData.map((c) => c.cache_hit_rate), 1);

  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><BarChartOutlined /><span>缓存效率</span></div>}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '8px 16px' } }}
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
  );
}

interface SkillsStatsCardProps {
  stats: DashboardStats | null;
  loading: boolean;
}

export function SkillsStatsCard({ stats, loading }: SkillsStatsCardProps) {
  const skillsStats = stats?.skills_stats;
  const skillsSuccessRate = skillsStats && skillsStats.total_invocations > 0
    ? (skillsStats.success_invocations / skillsStats.total_invocations * 100)
    : 0;

  return (
    <Card
      title={<div style={{ display: 'flex', alignItems: 'center', gap: 8 }}><ThunderboltOutlined /><span>Skills 调用统计</span></div>}
      extra={<ClockCircleOutlined style={{ color: 'var(--color-text-tertiary)' }} />}
      className="dashboard-card" style={{ borderRadius: 12 }}
      styles={{ body: { padding: '16px 20px' } }}
    >
      {skillsStats ? (
        <div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 12, marginBottom: 16 }}>
            <MetricCard
              title="总调用"
              value={skillsStats.total_invocations}
              prefix={<ThunderboltOutlined />}
              color="#6366f1"
              loading={loading}
              chineseFormat
            />
            <MetricCard
              title="今日调用"
              value={skillsStats.invocations_today}
              prefix={<ThunderboltOutlined />}
              color="#22c55e"
              loading={loading}
            />
            <MetricCard
              title="成功率"
              value={skillsSuccessRate}
              suffix="%"
              prefix={<CheckCircleOutlined />}
              color="#3b82f6"
              loading={loading}
              decimals={1}
            />
            <MetricCard
              title="平均耗时"
              value={skillsStats.avg_duration_ms}
              suffix="ms"
              prefix={<BarChartOutlined />}
              color="#f59e0b"
              loading={loading}
            />
          </div>
          {skillsStats.top_skills && skillsStats.top_skills.length > 0 && (
            <div>
              <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 8 }}>Top Skills</div>
              {skillsStats.top_skills.slice(0, 5).map((skill) => (
                <CompactRow
                  key={skill.skill_name}
                  name={skill.skill_name}
                  value={skill.count}
                  sub={`成功率 ${skill.success_rate.toFixed(1)}%`}
                  color="#6366f1"
                  barPct={(skill.count / (skillsStats.top_skills[0]?.count || 1)) * 100}
                />
              ))}
            </div>
          )}
        </div>
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无 Skills 调用数据" />
      )}
    </Card>
  );
}
