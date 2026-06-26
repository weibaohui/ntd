import { useMemo } from 'react';
import type { ReactNode } from 'react';
import { Button, Tooltip } from 'antd';
import type { ButtonProps } from 'antd';
import {
  InboxOutlined,
  ApartmentOutlined,
  DashboardOutlined,
  ReadOutlined,
  SettingOutlined,
  DoubleRightOutlined,
  DoubleLeftOutlined,
} from '@ant-design/icons';

export type LeftRailKey = 'inbox' | 'loops' | 'dashboard' | 'memorial' | 'settings';

interface LeftRailItem {
  key: LeftRailKey;
  label: string;
  icon: ReactNode;
  ariaLabel: string;
  danger?: boolean;
}

export type LeftRailVariant = 'rail' | 'drawer';

interface LeftRailProps {
  activeKey: LeftRailKey;
  onSelect: (key: LeftRailKey) => void;
  variant?: LeftRailVariant;
  collapsed?: boolean;
  onToggleCollapsed?: () => void;
}

/**
 * 左侧主导航栏。
 * 目标：为“中间列表 + 右侧工作区”补上一层全局导航，让用户能用更低成本在核心区域间切换。
 */
export function LeftRail({ activeKey, onSelect, variant = 'rail', collapsed = true, onToggleCollapsed }: LeftRailProps) {
  const items = useMemo<LeftRailItem[]>(() => ([
    {
      key: 'inbox',
      label: '收件箱',
      icon: <InboxOutlined />,
      ariaLabel: '收件箱',
    },
    {
      key: 'loops',
      label: '环路',
      icon: <ApartmentOutlined />,
      ariaLabel: '环路',
    },
    {
      key: 'dashboard',
      label: '仪表盘',
      icon: <DashboardOutlined />,
      ariaLabel: '仪表盘',
    },
    {
      key: 'memorial',
      label: '看板',
      icon: <ReadOutlined />,
      ariaLabel: '看板',
    },
    {
      key: 'settings',
      label: '设置',
      icon: <SettingOutlined />,
      ariaLabel: '设置',
    },
  ]), []);

  const isDrawer = variant === 'drawer';
  const shouldShowLabels = isDrawer || !collapsed;

  /**
   * 渲染单个导航按钮。
   * rail：只展示图标（靠 Tooltip 告知含义）；drawer：展示图标 + 文本，适配移动端。
   */
  const renderNavButton = (item: LeftRailItem) => {
    const isActive = item.key === activeKey;
    const commonProps: ButtonProps = {
      type: 'text',
      icon: item.icon,
      onClick: () => onSelect(item.key),
      className: isDrawer ? 'ntd-left-rail-drawer-btn' : 'ntd-left-rail-btn',
      'aria-label': item.ariaLabel,
      'data-testid': `left-rail-${item.key}`,
      danger: item.danger,
    };

    if (isDrawer) {
      return (
        <Button
          key={item.key}
          {...commonProps}
          className={`${commonProps.className} ${isActive ? 'active' : ''}`}
        >
          <span className="ntd-left-rail-drawer-label" data-testid={`left-rail-label-${item.key}`}>{item.label}</span>
        </Button>
      );
    }

    if (!shouldShowLabels) {
      return (
        <Tooltip key={item.key} title={item.label} placement="right">
          <Button
            {...commonProps}
            className={`${commonProps.className} ${isActive ? 'active' : ''}`}
          />
        </Tooltip>
      );
    }

    return (
      <Button
        key={item.key}
        {...commonProps}
        className={`ntd-left-rail-expanded-btn ${isActive ? 'active' : ''}`}
      >
        <span className="ntd-left-rail-expanded-label" data-testid={`left-rail-label-${item.key}`}>{item.label}</span>
      </Button>
    );
  };

  return (
    <div
      className={isDrawer ? 'ntd-left-rail-drawer' : `ntd-left-rail ${shouldShowLabels ? 'expanded' : 'collapsed'}`}
      data-testid="left-rail"
    >
      <div className={isDrawer ? 'ntd-left-rail-drawer-top' : 'ntd-left-rail-top'}>
        {items.map(renderNavButton)}
      </div>

      {!isDrawer && (
        <div className="ntd-left-rail-bottom">
          <Tooltip title={shouldShowLabels ? '收起导航' : '展开导航'} placement="right">
            <Button
              type="text"
              className="ntd-left-rail-toggle"
              icon={shouldShowLabels ? <DoubleLeftOutlined /> : <DoubleRightOutlined />}
              onClick={onToggleCollapsed}
              aria-label={shouldShowLabels ? '收起导航' : '展开导航'}
              data-testid="left-rail-toggle"
            />
          </Tooltip>
        </div>
      )}
    </div>
  );
}
