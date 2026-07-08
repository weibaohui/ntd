import { useMemo } from 'react';
import type { ReactNode } from 'react';
import { Button, Tooltip } from 'antd';
import type { ButtonProps } from 'antd';
import {
  UnorderedListOutlined,
  AppstoreOutlined,
  RetweetOutlined,
  DashboardOutlined,
  ReadOutlined,
  SettingOutlined,
  ThunderboltOutlined,
  FolderOutlined,
  CodeOutlined,
  DoubleRightOutlined,
  DoubleLeftOutlined,
  SunOutlined,
  MoonOutlined,
} from '@ant-design/icons';
import { TfiBlackboard } from 'react-icons/tfi';
import { WorkspaceSwitcher } from './WorkspaceSwitcher';

export type LeftRailKey =
  | 'items'
  | 'todoCenter'
  | 'loops'
  | 'dashboard'
  | 'memorial'
  | 'blackboard'
  | 'settings'
  | 'settings_projectDirectories'
  | 'settings_sessions'
  | 'settings_skills'
  | 'settings_executors';

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
  /** 当前选中的工作空间 ID（project_directories.id，唯一键）。null 表示未选。 */
  workspace?: number | null;
  onWorkspaceChange?: (workspaceId: number | null) => void;
  themeMode: 'light' | 'dark';
  toggleTheme: () => void;
}

/**
 * 左侧主导航栏。
 * 目标：为“中间列表 + 右侧工作区”补上一层全局导航，让用户能用更低成本在核心区域间切换。
 */
export function LeftRail({
  activeKey,
  onSelect,
  variant = 'rail',
  collapsed = true,
  onToggleCollapsed,
  workspace,
  onWorkspaceChange,
  themeMode,
  toggleTheme,
}: LeftRailProps) {
  const sections = useMemo(() => ([
    {
      title: '工作区',
      items: [
        { key: 'items', label: '事项', icon: <UnorderedListOutlined />, ariaLabel: '事项' },
        { key: 'todoCenter', label: '事项中心', icon: <AppstoreOutlined />, ariaLabel: '事项中心' },
        { key: 'loops', label: '环路', icon: <RetweetOutlined />, ariaLabel: '环路' },
        { key: 'blackboard', label: '黑板', icon: <TfiBlackboard />, ariaLabel: '黑板' },
        { key: 'dashboard', label: '仪表盘', icon: <DashboardOutlined />, ariaLabel: '仪表盘' },
        { key: 'memorial', label: '看板', icon: <ReadOutlined />, ariaLabel: '看板' },
      ] satisfies LeftRailItem[],
    },
    {
      title: '配置',
      items: [
        { key: 'settings_executors', label: '执行器', icon: <CodeOutlined />, ariaLabel: '执行器' },
        { key: 'settings_skills', label: 'Skills', icon: <ThunderboltOutlined />, ariaLabel: 'Skills' },
        { key: 'settings_projectDirectories', label: '工作空间', icon: <FolderOutlined />, ariaLabel: '工作空间' },
        { key: 'settings', label: '设置', icon: <SettingOutlined />, ariaLabel: '设置' },
      ] satisfies LeftRailItem[],
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

  const renderWorkspaceArea = () => {
    if (isDrawer || shouldShowLabels) {
      return (
        <div className={isDrawer ? 'ntd-left-rail-drawer-workspace' : 'ntd-left-rail-workspace'}>
          <WorkspaceSwitcher
            value={workspace ?? null}
            onChange={(next) => onWorkspaceChange?.(next)}
            onManage={() => onSelect('settings_projectDirectories')}
            showAddOption={true}
            mode="full"
          />
        </div>
      );
    }

    return (
      <div className="ntd-left-rail-workspace-collapsed">
        <WorkspaceSwitcher
          value={workspace ?? null}
          onChange={(next) => onWorkspaceChange?.(next)}
          onManage={() => onSelect('settings_projectDirectories')}
          showAddOption={true}
          mode="compact"
        />
      </div>
    );
  };

  return (
    <div
      className={isDrawer ? 'ntd-left-rail-drawer' : `ntd-left-rail ${shouldShowLabels ? 'expanded' : 'collapsed'}`}
      data-testid="left-rail"
    >
      {renderWorkspaceArea()}

      <div className={isDrawer ? 'ntd-left-rail-drawer-top' : 'ntd-left-rail-top'}>
        {sections.map(section => (
          <div key={section.title} className={isDrawer ? 'ntd-left-rail-drawer-section' : 'ntd-left-rail-section'}>
            {shouldShowLabels && (
              <div className={isDrawer ? 'ntd-left-rail-drawer-section-title' : 'ntd-left-rail-section-title'}>
                {section.title}
              </div>
            )}
            <div className={isDrawer ? 'ntd-left-rail-drawer-section-body' : 'ntd-left-rail-section-body'}>
              {section.items.map(renderNavButton)}
            </div>
          </div>
        ))}
      </div>

      {isDrawer && (
        <div className="ntd-left-rail-drawer-bottom">
          {/* 移动端抽屉底部：亮/暗色主题切换按钮 */}
          <Button
            type="text"
            block
            icon={themeMode === 'light' ? <MoonOutlined /> : <SunOutlined />}
            onClick={toggleTheme}
            className="ntd-left-rail-drawer-btn"
            data-testid="left-rail-theme-toggle"
          >
            <span className="ntd-left-rail-drawer-label">
              {themeMode === 'light' ? '暗色模式' : '亮色模式'}
            </span>
          </Button>
        </div>
      )}

      {!isDrawer && (
        <div className="ntd-left-rail-bottom">
          {/* 亮/暗色主题切换按钮 — 当前为亮色显示太阳，暗色显示月亮，点击切换 */}
          <Tooltip title={themeMode === 'light' ? '切换暗色' : '切换亮色'} placement="right">
            <Button
              type="text"
              className="ntd-left-rail-theme-toggle"
              icon={themeMode === 'light' ? <SunOutlined /> : <MoonOutlined />}
              onClick={toggleTheme}
              aria-label={themeMode === 'light' ? '切换暗色' : '切换亮色'}
              data-testid="left-rail-theme-toggle"
            />
          </Tooltip>
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
