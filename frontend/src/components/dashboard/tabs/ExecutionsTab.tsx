// 「执行」Tab — 任务跑的结果如何:成功率、执行器分布、触发源、亮点统计。
//
// 这里汇集所有「执行结果维度」的卡片:状态/执行器/时长/触发来源,
// 让用户诊断「任务跑得好不好、哪个执行器慢、谁触发的」。
import type { DashboardStats } from '@/types';
import { HighlightStatsCard, ExecStatsCard } from '@/components/dashboard/StatsGridCards';
import { ExecutorChartCard, ExecutorDurationCard } from '@/components/dashboard/DistributionCards';
import { StatusChartCard, TriggerSourceCard } from '@/components/dashboard/ChartCards';
import { TabMasonry, type PanelItem } from './TabMasonry';

interface ExecutionsTabProps {
  stats: DashboardStats | null;
  loading: boolean;
  totalTodos: number;
  tagsLength: number;
}

export function ExecutionsTab({ stats, loading, totalTodos, tagsLength }: ExecutionsTabProps) {
  const panels: PanelItem[] = [
    { key: 'exec-stats', render: () => <ExecStatsCard stats={stats} loading={loading} tagsLength={tagsLength} /> },
    { key: 'status-chart', render: () => <StatusChartCard stats={stats} totalTodos={totalTodos} /> },
    { key: 'executor-chart', render: () => <ExecutorChartCard stats={stats} /> },
    { key: 'executor-duration', render: () => <ExecutorDurationCard stats={stats} /> },
    { key: 'trigger-source', render: () => <TriggerSourceCard stats={stats} /> },
    { key: 'highlight-stats', render: () => <HighlightStatsCard stats={stats} /> },
  ];
  return <TabMasonry panels={panels} />;
}
