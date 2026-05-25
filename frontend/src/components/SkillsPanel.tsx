import { useState } from 'react';
import { Button } from 'antd';
import {
  AppstoreOutlined, BarChartOutlined,
  SwapOutlined, ThunderboltOutlined,
} from '@ant-design/icons';
import type { ReactNode } from 'react';
import { SkillsOverview } from './skills/SkillsOverview';
import { SkillsComparison } from './skills/SkillsComparison';
import { SkillSync } from './skills/SkillSync';
import { SkillTracking } from './skills/SkillTracking';

type SubView = 'overview' | 'compare' | 'sync' | 'tracking';

export function SkillsPanel() {
  const [activeView, setActiveView] = useState<SubView>('overview');

  const views: { key: SubView; label: string; icon: ReactNode }[] = [
    { key: 'overview', label: 'Skills 总览', icon: <AppstoreOutlined /> },
    { key: 'compare', label: '对比分析', icon: <BarChartOutlined /> },
    { key: 'sync', label: '同步管理', icon: <SwapOutlined /> },
    { key: 'tracking', label: '调用追踪', icon: <ThunderboltOutlined /> },
  ];

  return (
    <div>
      <div style={{
        display: 'flex',
        flexWrap: 'wrap',
        gap: 8,
        marginBottom: 20,
        borderBottom: '1px solid var(--color-border-light, #f0f0f0)',
        paddingBottom: 12,
      }}>
        {views.map(v => (
          <Button
            key={v.key}
            type={activeView === v.key ? 'primary' : 'default'}
            icon={v.icon}
            onClick={() => setActiveView(v.key)}
            style={{ borderRadius: 8, fontSize: 13, padding: '4px 10px' }}
          >
            {v.label}
          </Button>
        ))}
      </div>

      {activeView === 'overview' && <SkillsOverview />}
      {activeView === 'compare' && <SkillsComparison />}
      {activeView === 'sync' && <SkillSync />}
      {activeView === 'tracking' && <SkillTracking />}
    </div>
  );
}
