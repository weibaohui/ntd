import { useMemo } from 'react';
import type { ReactNode } from 'react';
import { Button, Tooltip } from 'antd';
import type { ButtonProps } from 'antd';
import {
  UnorderedListOutlined,
  RetweetOutlined,
  RocketOutlined,
  DashboardOutlined,
  CompassOutlined,
  SettingOutlined,
  ThunderboltOutlined,
  FolderOutlined,
  CodeOutlined,
  DoubleRightOutlined,
  DoubleLeftOutlined,
  SunOutlined,
  MoonOutlined,
  MessageOutlined,
  RobotOutlined,
  TeamOutlined,
  BuildOutlined,
  QuestionCircleOutlined,
} from '@ant-design/icons';
import { TfiBlackboard } from 'react-icons/tfi';
import { WorkspaceSwitcher } from './WorkspaceSwitcher';

/**
 * LeftRail 导航项 key 联合类型。
 *
 * 导航 key 契约（028 后）：
 *   - 顶级视图 key（'todos' / 'loops' / 'tasks' / 'processes' / 'messages' /
 *     'dashboard' / 'blackboard' / 'onboarding'）必须与
 *     useViewState.ts 的 View 联合类型一一对应，确保 activeView → navKey 不会失配。
 *   - settings_* 是「设置」面板下的子标签页 key，对应 SettingsPage 内部分支；
 *     顶级 'settings' 由 handleRailSelect 走 showSettings(null) 进入默认标签。
 *   - viewToNavKey 在 useViewState 内提供 View → LeftRailKey 的映射，
 *     修改本类型时需同步调整该映射，否则 LeftRail 高亮会错位。
 */
export type LeftRailKey =
  | 'todos'           // 028：原 'items' 已迁移为 'todos'，与 View 严格对齐
  | 'loops'
  | 'tasks'
  | 'processes'
  | 'messages'
  | 'dashboard'
  | 'blackboard'
  | 'settings'
  | 'settings_workspaces'
  | 'settings_sessions'
  | 'settings_skills'
  | 'settings_executors'
  | 'settings_experts'
  | 'settings_bots'
  | 'onboarding';

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
  /** 当前选中的工作空间 ID（workspaces.id，唯一键）。null 表示未选。 */
  workspace?: number | null;
  onWorkspaceChange?: (workspaceId: number | null) => void;
  themeMode: 'light' | 'dark';
  toggleTheme: () => void;
  /** 打开帮助抽屉的回调（rail 底部帮助按钮）。 */
  onOpenHelp?: () => void;
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
  onOpenHelp,
}: LeftRailProps) {
  const sections = useMemo(() => ([
    // 「概览」前置：新用户首次进入应先看「导航」理解概念，
    // Dashboard 数据为全库聚合不随 workspace 切换，与导航同属概览更合理。
    {
      title: '概览',
      items: [
        { key: 'onboarding', label: '导航', icon: <CompassOutlined />, ariaLabel: '导航' },
        { key: 'dashboard', label: '仪表盘', icon: <DashboardOutlined />, ariaLabel: '仪表盘' },
      ] satisfies LeftRailItem[],
    },
    // 「工作」：核心工作概念，按使用频率排序（任务最常用 → 工艺最少用）。
    // 任务/事项是日常载体，环路/工艺是配置态，频次递降。
    {
      title: '工作',
      items: [
        { key: 'tasks', label: '任务', icon: <RocketOutlined />, ariaLabel: '任务' },
        { key: 'todos', label: '事项', icon: <UnorderedListOutlined />, ariaLabel: '事项' },
        { key: 'loops', label: '环路', icon: <RetweetOutlined />, ariaLabel: '环路' },
        { key: 'processes', label: '工艺', icon: <BuildOutlined />, ariaLabel: '工艺' },
      ] satisfies LeftRailItem[],
    },
    // 「观察」：辅助观察工具，低频访问，独立分组避免与核心工作概念混层。
    {
      title: '观察',
      items: [
        { key: 'messages', label: '消息', icon: <MessageOutlined />, ariaLabel: '消息' },
        { key: 'blackboard', label: '黑板', icon: <TfiBlackboard />, ariaLabel: '黑板' },
      ] satisfies LeftRailItem[],
    },
    // 「配置」：所有配置项合并一处，避免技能/专家与执行器/智能助手分两处导致用户找不到。
    // 顺序：能力包（技能/专家）→ 运行时（执行器）→ 入口（智能助手/工作空间）→ 其余（更多设置）。
    {
      title: '配置',
      items: [
        { key: 'settings_skills', label: '技能', icon: <ThunderboltOutlined />, ariaLabel: '技能' },
        { key: 'settings_experts', label: '专家', icon: <TeamOutlined />, ariaLabel: '专家' },
        { key: 'settings_executors', label: '执行器', icon: <CodeOutlined />, ariaLabel: '执行器' },
        { key: 'settings_bots', label: '智能助手', icon: <RobotOutlined />, ariaLabel: '智能助手' },
        { key: 'settings_workspaces', label: '工作空间', icon: <FolderOutlined />, ariaLabel: '工作空间' },
        { key: 'settings', label: '更多设置', icon: <SettingOutlined />, ariaLabel: '更多设置' },
      ] satisfies LeftRailItem[],
    },
  ]), []);

  // 配置项菜单已合并到上方「配置」区常驻入口，底部弹出菜单已废弃（消除分裂感）。
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
            onManage={() => onSelect('settings_workspaces')}
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
          onManage={() => onSelect('settings_workspaces')}
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
          {/* 移动端抽屉底部：亮/暗色主题切换按钮（配置菜单已合并到上方「配置」区常驻入口） */}
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
          {/* 帮助按钮：与主题切换并列，移动端 drawer 形态下也可见 */}
          <Button
            type="text"
            block
            icon={<QuestionCircleOutlined />}
            onClick={onOpenHelp}
            className="ntd-left-rail-drawer-btn"
            data-testid="left-rail-help"
          >
            <span className="ntd-left-rail-drawer-label">帮助</span>
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
          {/* 帮助按钮：打开帮助抽屉，与主题切换并列属「全局辅助操作」 */}
          <Tooltip title="帮助" placement="right">
            <Button
              type="text"
              className="ntd-left-rail-help"
              icon={<QuestionCircleOutlined />}
              onClick={onOpenHelp}
              aria-label="帮助"
              data-testid="left-rail-help"
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
