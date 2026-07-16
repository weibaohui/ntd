import { useState } from 'react';
import { Segmented } from 'antd';
import {
  AppstoreOutlined,
  ThunderboltOutlined,
  SyncOutlined, ShopOutlined,
} from '@ant-design/icons';
import type { ReactNode } from 'react';
import { PageCard } from '@/components/common/PageCard';
import { SkillsOverview } from './skills/SkillsOverview';
import { SkillVersionUpdate } from './skills/SkillVersionUpdate';
import { SkillMarketplace } from './skills/SkillMarketplace';

// 移除使用频率低的子视图（「同步管理」「调用追踪」「对比分析」），
// 只保留最常用的 3 个：总览 / 技能市场 / 版本更新。
type SubView = 'overview' | 'version-update' | 'marketplace';

export function SkillsPanel() {
  const [activeView, setActiveView] = useState<SubView>('overview');

  const views: { label: ReactNode; value: SubView }[] = [
    { label: <span><AppstoreOutlined /> 总览</span>, value: 'overview' },
    { label: <span><ShopOutlined /> 技能市场</span>, value: 'marketplace' },
    { label: <span><SyncOutlined /> 版本更新</span>, value: 'version-update' },
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
    </PageCard>
  );
}
