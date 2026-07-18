// 「资源与运维」Tab — 配置了什么、系统健康吗。
//
// 汇总配置类资源盘点(专家/Bot/工作空间/执行器/备份)+ 运维健康度
// (版本/云同步/内置资源同步)。每张卡自取数据,失败时降级为空状态。
// 分享卡已移至总览 Tab;Skills 调用统计卡暂移除——该功能后端尚未落地
// (skill_invocations 无写入),展示空数据无意义,待功能就绪再加回。
import type { DashboardStats } from '@/types';
import { BackupStatsCard } from '@/components/dashboard/StatsGridCards';
import { VersionCard } from '@/components/dashboard/cards/VersionCard';
import { CloudSyncCard } from '@/components/dashboard/cards/CloudSyncCard';
import { BundledSyncCard } from '@/components/dashboard/cards/BundledSyncCard';
import { ExpertsCard } from '@/components/dashboard/cards/ExpertsCard';
import { AgentBotsCard } from '@/components/dashboard/cards/AgentBotsCard';
import { WorkspaceCard } from '@/components/dashboard/cards/WorkspaceCard';
import { ExecutorConfigCard } from '@/components/dashboard/cards/ExecutorConfigCard';
import { TabMasonry, type PanelItem } from './TabMasonry';

interface ResourcesTabProps {
  stats: DashboardStats | null;
  loading: boolean;
}

export function ResourcesTab({ stats, loading }: ResourcesTabProps) {
  const panels: PanelItem[] = [
    { key: 'experts', render: () => <ExpertsCard /> },
    { key: 'agent-bots', render: () => <AgentBotsCard /> },
    { key: 'workspace', render: () => <WorkspaceCard /> },
    { key: 'executor-config', render: () => <ExecutorConfigCard /> },
    { key: 'version', render: () => <VersionCard /> },
    { key: 'cloud-sync', render: () => <CloudSyncCard /> },
    { key: 'bundled-sync', render: () => <BundledSyncCard /> },
    { key: 'backup-stats', render: () => <BackupStatsCard stats={stats} loading={loading} /> },
  ];
  return <TabMasonry panels={panels} />;
}
