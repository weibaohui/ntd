import { useMemo, useState } from 'react';
import type { ReactNode } from 'react';
import { Button, Tooltip, Popover } from 'antd';
import type { ButtonProps } from 'antd';
import {
  UnorderedListOutlined,
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
  MessageOutlined,
  RobotOutlined,
  TeamOutlined,
  AppstoreOutlined,
} from '@ant-design/icons';
import { TfiBlackboard } from 'react-icons/tfi';
import { WorkspaceSwitcher } from './WorkspaceSwitcher';

export type LeftRailKey =
  | 'items'
  | 'loops'
  | 'messages'
  | 'dashboard'
  | 'memorial'
  | 'blackboard'
  | 'settings'
  | 'settings_projectDirectories'
  | 'settings_sessions'
  | 'settings_skills'
  | 'settings_executors'
  | 'settings_experts'
  | 'settings_bots';

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
        { key: 'loops', label: '环路', icon: <RetweetOutlined />, ariaLabel: '环路' },
        { key: 'messages', label: '消息', icon: <MessageOutlined />, ariaLabel: '消息' },
        { key: 'blackboard', label: '黑板', icon: <TfiBlackboard />, ariaLabel: '黑板' },
        { key: 'dashboard', label: '仪表盘', icon: <DashboardOutlined />, ariaLabel: '仪表盘' },
        { key: 'memorial', label: '看板', icon: <ReadOutlined />, ariaLabel: '看板' },
      ] satisfies LeftRailItem[],
    },
    // 「配置」区放在主「工作区」下方：
    // 技能/专家原本藏在底部弹出菜单里，层级过深，提升为常驻入口更易触达。
    // key 复用 settings_skills / settings_experts，路由与 active 高亮无需额外改动。
    {
      title: '配置',
      items: [
        { key: 'settings_skills', label: '技能', icon: <ThunderboltOutlined />, ariaLabel: '技能' },
        { key: 'settings_experts', label: '专家', icon: <TeamOutlined />, ariaLabel: '专家' },
      ] satisfies LeftRailItem[],
    },
  ]), []);

  // 配置项菜单 — 从侧边栏主体收拢到底部弹出菜单，减少主导航的视觉噪音。
  // 技能/专家已提升为工作空间下方的常驻「配置」区，这里只保留次要配置入口；
  // 「设置」改名「更多设置」，强调它是技能/专家之外的其余配置入口。
  const configItems = useMemo(() => ([
    { key: 'settings_bots', label: '智能助手', icon: <RobotOutlined />, ariaLabel: '智能助手' },
    { key: 'settings_executors', label: '执行器', icon: <CodeOutlined />, ariaLabel: '执行器' },
    { key: 'settings_projectDirectories', label: '工作空间', icon: <FolderOutlined />, ariaLabel: '工作空间' },
    { key: 'settings', label: '更多设置', icon: <SettingOutlined />, ariaLabel: '更多设置' },
  ] satisfies LeftRailItem[]), []);

  const [openConfigPopover, setOpenConfigPopover] = useState(false);

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

  /**
   * 渲染配置弹出菜单的内容面板。
   * 将原来侧边栏里的配置 section 收拢为一个可点击展开的菜单，保持功能完整的同时减少主导航视觉噪音。
   */
  const renderConfigMenu = () => (
    <div className="ntd-config-menu">
      <div className="ntd-config-menu-title">配置</div>
      <div className="ntd-config-menu-body">
        {configItems.map((item) => {
          const isActive = item.key === activeKey;
          return (
            <button
              key={item.key}
              className={`ntd-config-menu-item ${isActive ? 'active' : ''}`}
              onClick={() => {
                onSelect(item.key);
                setOpenConfigPopover(false);
              }}
              aria-label={item.ariaLabel}
              data-testid={`config-menu-${item.key}`}
            >
              <span className="ntd-config-menu-item-icon">{item.icon}</span>
              <span className="ntd-config-menu-item-label">{item.label}</span>
            </button>
          );
        })}
      </div>
    </div>
  );

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
          {/* 移动端抽屉底部：配置菜单 + 亮/暗色主题切换按钮 */}
          <Popover
            content={renderConfigMenu()}
            open={openConfigPopover}
            onOpenChange={setOpenConfigPopover}
            placement="topLeft"
            trigger="click"
            overlayClassName="ntd-config-menu-popover"
          >
            <Button
              type="text"
              block
              icon={<AppstoreOutlined />}
              className="ntd-left-rail-drawer-btn"
              data-testid="left-rail-config-toggle"
            >
              <span className="ntd-left-rail-drawer-label">配置</span>
            </Button>
          </Popover>
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
          {/* 配置按钮 — 点击弹出收拢的配置菜单，替代原来侧边栏里的配置 section */}
          {/* 桌面端「点了弹不出来」的根因与修复见 App.css 的 .ntd-config-menu-popover 注释
              （rc-trigger 在 transition 中误测坐标，用 transition:none 修复）。
              这里两点配合：placement 用 topLeft（左对齐按钮、最自然，与 Drawer 内同款一致）；
              getPopupContainer 指向 body，避免挂进 AntD <App> 的 height:0 容器。 */}
          <Popover
            content={renderConfigMenu()}
            open={openConfigPopover}
            onOpenChange={setOpenConfigPopover}
            placement="topLeft"
            trigger="click"
            getPopupContainer={() => document.body}
            overlayClassName="ntd-config-menu-popover"
          >
            <Button
              type="text"
              className="ntd-left-rail-config-toggle"
              icon={<AppstoreOutlined />}
              aria-label="配置"
              data-testid="left-rail-config-toggle"
            />
          </Popover>
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
