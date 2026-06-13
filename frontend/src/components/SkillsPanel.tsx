import { useState } from 'react';
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
    { key: 'overview', label: '总览', icon: <AppstoreOutlined /> },
    { key: 'compare', label: '对比分析', icon: <BarChartOutlined /> },
    { key: 'sync', label: '同步管理', icon: <SwapOutlined /> },
    { key: 'tracking', label: '调用追踪', icon: <ThunderboltOutlined /> },
  ];

  return (
    <div>
      {/* Tab bar */}
      <div
        role="tablist"
        aria-label="Skills 管理"
        style={{
          display: 'flex',
          gap: 4,
          marginBottom: 20,
          borderBottom: '1px solid var(--color-border, #e2e8f0)',
          paddingBottom: 0,
        }}
      >
        {views.map(v => {
          const isActive = activeView === v.key;
          return (
            <button
              key={v.key}
              role="tab"
              aria-selected={isActive}
              onClick={() => setActiveView(v.key)}
              style={{
                display: 'inline-flex',
                alignItems: 'center',
                gap: 6,
                padding: '10px 16px',
                border: 'none',
                borderBottom: `2px solid ${isActive ? 'var(--color-primary, #0891b2)' : 'transparent'}`,
                background: 'transparent',
                color: isActive
                  ? 'var(--color-primary, #0891b2)'
                  : 'var(--color-text-secondary, #a6adc8)',
                cursor: 'pointer',
                fontSize: 14,
                fontWeight: isActive ? 500 : 400,
                transition: 'all 0.2s',
                marginBottom: -1,
              }}
              onMouseEnter={e => {
                if (!isActive) e.currentTarget.style.color = 'var(--color-text, #cdd6f4)';
              }}
              onMouseLeave={e => {
                if (!isActive) e.currentTarget.style.color = 'var(--color-text-secondary, #a6adc8)';
              }}
            >
              {v.icon}
              {v.label}
            </button>
          );
        })}
      </div>

      {activeView === 'overview' && <SkillsOverview />}
      {activeView === 'compare' && <SkillsComparison />}
      {activeView === 'sync' && <SkillSync />}
      {activeView === 'tracking' && <SkillTracking />}
    </div>
  );
}
