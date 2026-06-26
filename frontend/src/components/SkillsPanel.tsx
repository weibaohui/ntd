import { useState } from 'react';
import { Segmented } from 'antd';
import {
  AppstoreOutlined, BarChartOutlined,
  SwapOutlined, ThunderboltOutlined,
} from '@ant-design/icons';
import type { ReactNode } from 'react';
import { PageCard } from '@/components/common/PageCard';
import { SkillsOverview } from './skills/SkillsOverview';
import { SkillsComparison } from './skills/SkillsComparison';
import { SkillSync } from './skills/SkillSync';
import { SkillTracking } from './skills/SkillTracking';

type SubView = 'overview' | 'compare' | 'sync' | 'tracking';

export function SkillsPanel() {
  const [activeView, setActiveView] = useState<SubView>('overview');

  const views: { label: ReactNode; value: SubView }[] = [
    { label: <span><AppstoreOutlined /> 总览</span>, value: 'overview' },
    { label: <span><BarChartOutlined /> 对比分析</span>, value: 'compare' },
    { label: <span><SwapOutlined /> 同步管理</span>, value: 'sync' },
    { label: <span><ThunderboltOutlined /> 调用追踪</span>, value: 'tracking' },
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
      {activeView === 'compare' && <SkillsComparison />}
      {activeView === 'sync' && <SkillSync />}
      {activeView === 'tracking' && <SkillTracking />}
    </PageCard>
  );
}
