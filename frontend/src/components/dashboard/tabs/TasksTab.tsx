// 「任务」Tab — 聚焦「我有哪些事」:任务状态分布、标签分布、时间驱动 todo、模板、评分分布。
import type { DashboardStats } from '@/types';
import { TaskStatsCard } from '@/components/dashboard/StatsGridCards';
import { TagChartCard } from '@/components/dashboard/DistributionCards';
import { CronTodosCard } from '@/components/dashboard/cards/CronTodosCard';
import { TemplateCountCard } from '@/components/dashboard/cards/TemplateCountCard';
import { RatingDistCard } from '@/components/dashboard/cards/RatingDistCard';
import { TabMasonry, type PanelItem } from './TabMasonry';

interface TasksTabProps {
  stats: DashboardStats | null;
  loading: boolean;
  totalTodos: number;
}

export function TasksTab({ stats, loading, totalTodos }: TasksTabProps) {
  const panels: PanelItem[] = [
    { key: 'task-stats', render: () => <TaskStatsCard stats={stats} loading={loading} totalTodos={totalTodos} /> },
    { key: 'tag-chart', render: () => <TagChartCard stats={stats} /> },
    { key: 'cron-todos', render: () => <CronTodosCard /> },
    { key: 'template-count', render: () => <TemplateCountCard /> },
    { key: 'rating-dist', render: () => <RatingDistCard /> },
  ];
  return <TabMasonry panels={panels} />;
}
