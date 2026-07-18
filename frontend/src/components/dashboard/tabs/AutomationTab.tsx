// 「自动化」Tab — 哪些自动流程在驱动任务执行:Loop 环路、飞书消息与监听。
//
// Loop 聚合用全局 hours 过滤;飞书消息吞吐(MessageStatsCard)与监听健康(FeishuMonitorCard)互补。
import type { FeishuMessageStats } from '@/types';
import { MessageStatsCard } from '@/components/dashboard/StatsGridCards';
import { LoopStatsCard } from '@/components/dashboard/cards/LoopStatsCard';
import { FeishuMonitorCard } from '@/components/dashboard/cards/FeishuMonitorCard';
import { TabMasonry, type PanelItem } from './TabMasonry';

interface AutomationTabProps {
  msgStats: FeishuMessageStats | null;
  msgStatsError: boolean;
  processingRate: number;
  /** 全局时间范围(小时),供 Loop 聚合按窗口过滤;custom 时 undefined=全时段。 */
  hours?: number;
}

export function AutomationTab({ msgStats, msgStatsError, processingRate, hours }: AutomationTabProps) {
  const panels: PanelItem[] = [
    { key: 'loop-stats', render: () => <LoopStatsCard hours={hours} /> },
    {
      key: 'message-stats',
      render: () => (
        <MessageStatsCard msgStats={msgStats} msgStatsError={msgStatsError} processingRate={processingRate} />
      ),
    },
    { key: 'feishu-monitor', render: () => <FeishuMonitorCard /> },
  ];
  return <TabMasonry panels={panels} />;
}
