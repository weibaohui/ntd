// 「总览」Tab — landing 视图,第一眼应看到的关键信号。
//
// 设计目标:一屏看完系统健康度,不放细节卡片(细节下沉到其他 Tab)。
// 包含:核心 KPI、运行中任务、执行趋势、贡献热力图、活跃/连续打卡、最近执行记录表,
// 末尾放分享卡(安装引导)——看完核心数据后的自然推广位。
import type { DashboardStats, RunningTask, Todo } from '@/types';
import { KeyMetricsCard, OverviewCard } from '@/components/dashboard/StatsGridCards';
import { ActiveTasksCard, ShareCardPanel } from '@/components/dashboard/SpecialCards';
import { TrendChartCard, ContributionHeatmapCard } from '@/components/dashboard/ChartCards';
import { RecentExecutionsTable } from '@/components/dashboard/RecentExecutionsTable';
import { TabMasonry, type PanelItem } from './TabMasonry';

interface OverviewTabProps {
  stats: DashboardStats | null;
  loading: boolean;
  successRate: number;
  runningTasks: RunningTask[];
  todos: Todo[];
}

export function OverviewTab({ stats, loading, successRate, runningTasks, todos }: OverviewTabProps) {
  // 沿用原 Dashboard 的 panels 结构(key + render),仅保留总览域 5 卡。
  // 卡片组件本身 0 改动,只是换了父容器,降低迁移风险。
  const panels: PanelItem[] = [
    { key: 'key-metrics', render: () => <KeyMetricsCard stats={stats} loading={loading} successRate={successRate} /> },
    { key: 'active-tasks', render: () => <ActiveTasksCard runningTasks={runningTasks} /> },
    { key: 'trend-chart', render: () => <TrendChartCard stats={stats} /> },
    { key: 'overview-card', render: () => <OverviewCard stats={stats} successRate={successRate} /> },
  ];

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <TabMasonry panels={panels} />
      {/* 活动热力图单独全宽:一年 53 周横向跨度大,塞进瀑布流单列时格子被压到 ~7px 几乎不可见;
          全宽渲染后格子随宽度放大到 ~18px,可读性显著提升。故从 panels 移出独占一行。 */}
      <ContributionHeatmapCard stats={stats} />
      {/* 最近执行记录是表格而非卡片,与瀑布流视觉节奏不同,单独成块置于下方更清晰。 */}
      <RecentExecutionsTable executions={stats?.recent_executions ?? []} todos={todos} />
      {/* 分享卡置于总览底部:安装引导属于推广位,原先在资源 tab(配置/健康度)语义不符。 */}
      <ShareCardPanel />
    </div>
  );
}
