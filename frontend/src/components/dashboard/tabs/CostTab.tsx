// 「成本与模型」Tab — 花了多少钱、Token 去哪了。
//
// 汇总所有 token/费用/模型维度卡片,外加 ccusage 通道的会话统计。
// UsageStatsCard 自取 /api/usage-stats,只需 since/until,与其他卡片数据通道解耦。
import type { DashboardStats } from '@/types';
import { InferenceStatsCard } from '@/components/dashboard/StatsGridCards';
import { ModelTaskChartCard, ModelTokenChartCard, ModelCacheCard } from '@/components/dashboard/DistributionCards';
import { TokenChartCard, TokenTrendChartCard } from '@/components/dashboard/ChartCards';
import { LeaderboardCard } from '@/components/dashboard/SpecialCards';
import { UsageStatsCard } from '@/components/dashboard/UsageStatsCard';
import { SessionsStatsCard } from '@/components/dashboard/cards/SessionsStatsCard';
import { TabMasonry, type PanelItem } from './TabMasonry';

interface CostTabProps {
  stats: DashboardStats | null;
  loading: boolean;
  /** ccusage 通道的时间窗(ISO),由顶层 TimeRangeSelector 派生,全局共享。 */
  usageSince?: string;
  usageUntil?: string;
}

export function CostTab({ stats, loading, usageSince, usageUntil }: CostTabProps) {
  const panels: PanelItem[] = [
    { key: 'inference-stats', render: () => <InferenceStatsCard stats={stats} loading={loading} /> },
    { key: 'sessions-stats', render: () => <SessionsStatsCard /> },
    { key: 'token-chart', render: () => <TokenChartCard stats={stats} /> },
    { key: 'token-trend-chart', render: () => <TokenTrendChartCard stats={stats} /> },
    { key: 'model-task-chart', render: () => <ModelTaskChartCard stats={stats} /> },
    { key: 'model-token-chart', render: () => <ModelTokenChartCard stats={stats} /> },
    { key: 'model-cache', render: () => <ModelCacheCard stats={stats} /> },
    { key: 'leaderboard', render: () => <LeaderboardCard leaderboard={stats?.leaderboard ?? []} /> },
  ];

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <TabMasonry panels={panels} />
      {/* UsageStatsCard 数据较多(含 daily/weekly/monthly + 模型 breakdown),
          固定置底,与原 Dashboard 布局一致,避免被瀑布流拆散。 */}
      <UsageStatsCard since={usageSince} until={usageUntil} />
    </div>
  );
}
