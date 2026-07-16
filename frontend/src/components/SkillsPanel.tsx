import { useState } from 'react';
import { Segmented } from 'antd';
import {
  AppstoreOutlined, BarChartOutlined,
  ThunderboltOutlined,
  SyncOutlined, ShopOutlined,
} from '@ant-design/icons';
import type { ReactNode } from 'react';
import { PageCard } from '@/components/common/PageCard';
import { SkillsOverview } from './skills/SkillsOverview';
import { SkillsComparison } from './skills/SkillsComparison';
import { SkillVersionUpdate } from './skills/SkillVersionUpdate';
import { SkillMarketplace } from './skills/SkillMarketplace';

// 移除「同步管理」「调用追踪」两个使用频率低的子视图，保留核心 4 个：
// 总览 / 技能市场 / 版本更新 / 对比分析。
type SubView = 'overview' | 'version-update' | 'compare' | 'marketplace';

export function SkillsPanel() {
  const [activeView, setActiveView] = useState<SubView>('overview');

  const views: { label: ReactNode; value: SubView }[] = [
    { label: <span><AppstoreOutlined /> 总览</span>, value: 'overview' },
    { label: <span><ShopOutlined /> 技能市场</span>, value: 'marketplace' },
    { label: <span><SyncOutlined /> 版本更新</span>, value: 'version-update' },
    { label: <span><BarChartOutlined /> 对比分析</span>, value: 'compare' },
  ];

  return (
    <PageCard
      icon={<ThunderboltOutlined />}
      title="Skills"
      extra={
        <Segmented
          size="small"
          value={activeView}
          onChange={value => setActiveView(value as SubView)}
          options={views}
        />
      }
    >
      {activeView === 'overview' && <SkillsOverview />}
      {activeView === 'marketplace' && <SkillMarketplace />}
      {activeView === 'version-update' && <SkillVersionUpdate />}
      {activeView === 'compare' && <SkillsComparison />}
    </PageCard>
  );
}
