import { useState, useEffect, useMemo, useCallback } from 'react';
import { useApp } from '../hooks/useApp';
import { useIsMobile } from '../hooks/useIsMobile';
import { Button, Dropdown, Empty, Tooltip } from 'antd';
import type { MenuProps } from 'antd';
import { PlusOutlined, ThunderboltOutlined, ClockCircleOutlined, InboxOutlined, DashboardOutlined, ReadOutlined, SettingOutlined, SunOutlined, MoonOutlined, ApartmentOutlined, UnorderedListOutlined, FolderOpenOutlined, RightOutlined, MoreOutlined } from '@ant-design/icons';
import { useTheme } from '../hooks/useTheme';
import { StatusPicker } from './StatusPicker';
import * as db from '../utils/database';
import type { ProjectDirectory, Todo } from '../types';
import { ExecutorBadge } from './ExecutorBadge';
import { formatRelativeTime } from '../utils/datetime';

interface TodoListProps {
  onOpenCreateModal: () => void;
  onOpenSmartCreate: () => void;
  onSelectTodo?: (todoId: string | number) => void;
  onShowDashboard?: () => void;
  onShowMemorial?: () => void;
  onShowRelationMap?: () => void;
  onShowSettings?: () => void;
}

function SkeletonRow() {
  return <div className="skeleton-row" />;
}

function SkeletonList() {
  return (
    <div style={{ padding: '12px 16px' }}>
      {Array.from({ length: 6 }).map((_, i) => (
        <SkeletonRow key={i} />
      ))}
    </div>
  );
}

// 列表显示模式：flat 是当前的平铺模式；grouped 按项目目录把 todo 折叠成"文件夹"分组
type ListDisplayMode = 'flat' | 'grouped';

/**
 * 构建桌面端头部高频导航按钮。
 */
function buildDesktopNavActions(
  onShowDashboard: TodoListProps['onShowDashboard'],
  onShowMemorial: TodoListProps['onShowMemorial'],
  onShowRelationMap: TodoListProps['onShowRelationMap'],
) {
  return [
    {
      key: 'dashboard',
      title: '仪表盘',
      icon: <DashboardOutlined />,
      onClick: onShowDashboard ? () => onShowDashboard() : undefined,
      ariaLabel: '查看仪表盘',
    },
    {
      key: 'memorial',
      title: '看板',
      icon: <ReadOutlined />,
      onClick: onShowMemorial ? () => onShowMemorial() : undefined,
      ariaLabel: '看板',
    },
    {
      key: 'relation-map',
      title: '关联图',
      icon: <ApartmentOutlined />,
      onClick: onShowRelationMap ? () => onShowRelationMap() : undefined,
      ariaLabel: '关联图',
    },
  ].filter(action => typeof action.onClick === 'function');
}

export function TodoList({ onOpenCreateModal, onOpenSmartCreate, onSelectTodo, onShowDashboard, onShowMemorial, onShowRelationMap, onShowSettings }: TodoListProps) {
  const { state, dispatch } = useApp();
  const { themeMode, toggleTheme } = useTheme();
  const { todos, selectedTodoId, selectedTagId, tags } = state;
  const isMobile = useIsMobile();
  const [isLoading, setIsLoading] = useState(true);
  // 显示模式：默认平铺；用户切换时只影响本地视图，刷新后会回到默认
  const [displayMode, setDisplayMode] = useState<ListDisplayMode>('flat');
  // 项目目录：分组视图需要按项目维度折叠，必须先有这份映射；
  // 即使切回平铺视图也保留数据，避免切换时闪烁
  const [projectDirectories, setProjectDirectories] = useState<ProjectDirectory[]>([]);
  // 记录每个分组的折叠状态，key 为 workspace 路径
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set());

  useEffect(() => {
    setIsLoading(false);
  }, []);

  // 进入页面时拉取项目目录；后续 Todo 抽屉新增/删除目录时也会主动重拉，确保分组始终准确。
  // 失败时静默处理：分组视图退化为只显示路径即可，不阻塞主流程。
  const reloadProjectDirectories = useCallback(() => {
    db.getProjectDirectories() // 从后端拉取全量目录列表
      .then(setProjectDirectories) // 更新本地状态，触发分组重新计算
      .catch(() => {
        // 静默失败：分组视图退化为只显示路径即可，不阻塞主流程
      });
  }, []);

  useEffect(() => {
    reloadProjectDirectories(); // 首次加载目录
    // 监听 TodoDrawer 快速新增项目目录的事件，及时刷新分组数据
    const handleDirAdded = () => reloadProjectDirectories();
    window.addEventListener('projectDirectoryAdded', handleDirAdded); // 跨组件通知
    return () => window.removeEventListener('projectDirectoryAdded', handleDirAdded); // 清理：卸载时移除监听
  }, [reloadProjectDirectories]);

  const filteredTodos = useMemo(() =>
    selectedTagId
      ? todos.filter(t => (t as any).tag_ids?.includes(selectedTagId))
      : todos,
    [todos, selectedTagId]
  );

  // 按 workspace 路径分组；workspace 为空或 null 的归入"未分组"虚拟分组，
  // 保证游离 todo 不会消失，只是放到列表底部
  const groupedByProject = useMemo(() => {
    // path -> { name, items }：用 path 作为 key，避免同路径多份目录实体造成的重复分组
    // 用 Map 存储目录查找表，把内层 O(n·m) 查找降为 O(1)
    const dirByPath = new Map(projectDirectories.map(d => [d.path, d]));
    const groups = new Map<string, { name: string; items: Todo[] }>();
    for (const todo of filteredTodos) {
      const ws = (todo.workspace || '').trim(); // 取 workspace 路径，空字符串视为未分组
      if (!ws) {
        const bucket = groups.get('__ungrouped__') ?? { name: '未分组', items: [] }; // 虚拟分组 key，不会与真实路径冲突
        bucket.items.push(todo);
        groups.set('__ungrouped__', bucket);
        continue;
      }
      const dir = dirByPath.get(ws); // O(1) 查找目录实体，取项目名称
      const bucket = groups.get(ws) ?? { name: dir?.name || ws, items: [] }; // 有名称用名称，无则回退显示路径
      bucket.items.push(todo);
      groups.set(ws, bucket);
    }
    // 稳定顺序：项目目录按 name 排序，未分组始终在最末
    const entries = Array.from(groups.entries());
    entries.sort((a, b) => {
      if (a[0] === '__ungrouped__') return 1;
      if (b[0] === '__ungrouped__') return -1;
      return a[1].name.localeCompare(b[1].name);
    });
    return entries;
  }, [filteredTodos, projectDirectories]);

  const handleStatusChange = useCallback(async (todoId: number, title: string, prompt: string, newStatus: string) => {
    try {
      const updated = await db.updateTodo(todoId, title, prompt, newStatus);
      dispatch({ type: 'UPDATE_TODO', payload: updated });
    } catch {
      // ignore: interceptor already shows error
    }
  }, [dispatch]);

  const desktopNavActions = useMemo(
    () => buildDesktopNavActions(onShowDashboard, onShowMemorial, onShowRelationMap),
    [onShowDashboard, onShowMemorial, onShowRelationMap],
  );

  const toggleGroupCollapse = useCallback((key: string) => {
    setCollapsedGroups(prev => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  }, []);

  /**
   * 处理桌面端头部“更多”菜单点击。
   */
  const handleHeaderMenuClick = useCallback<NonNullable<MenuProps['onClick']>>(({ key }) => {
    if (key === 'display-mode') {
      setDisplayMode(prev => (prev === 'flat' ? 'grouped' : 'flat'));
      return;
    }
    if (key === 'theme') {
      toggleTheme();
      return;
    }
    if (key === 'settings') {
      onShowSettings?.();
    }
  }, [onShowSettings, toggleTheme]);

  const headerMenuItems = useMemo<MenuProps['items']>(() => {
    const items: NonNullable<MenuProps['items']> = [
      {
        key: 'display-mode',
        icon: displayMode === 'flat' ? <FolderOpenOutlined /> : <UnorderedListOutlined />,
        label: (
          <span aria-pressed={displayMode === 'grouped'}>
            {displayMode === 'flat' ? '切换为按项目分组' : '切换为平铺列表'}
          </span>
        ),
      },
      {
        key: 'theme',
        icon: themeMode === 'light' ? <MoonOutlined /> : <SunOutlined />,
        label: themeMode === 'light' ? '切换暗色主题' : '切换亮色主题',
      },
    ];

    if (onShowSettings) {
      items.push(
        { type: 'divider' },
        {
          key: 'settings',
          icon: <SettingOutlined />,
          label: '配置管理',
        },
      );
    }

    return items;
  }, [displayMode, themeMode, onShowSettings]);

  const tagMap = useMemo(() => {
    const map = new Map<number, typeof tags[0]>();
    for (const tag of tags) map.set(tag.id, tag);
    return map;
  }, [tags]);

  // 抽离 Todo 行渲染，平铺与分组两个模式共用，避免重复代码
  const renderTodoItem = (todo: Todo) => {
    const todoTags = ((todo as any).tag_ids as number[] | undefined)?.map(id => tagMap.get(id)).filter((t): t is typeof tags[0] => !!t) ?? [];
    const primaryTag = todoTags[0];
    const isCompleted = todo.status === 'completed';
    const relativeTime = formatRelativeTime(todo.updated_at);

    return (
      <div
        key={todo.id}
        onClick={() => {
          dispatch({ type: 'SELECT_TODO', payload: todo.id });
          onSelectTodo?.(todo.id);
        }}
        className={`todo-item ${selectedTodoId === todo.id ? 'selected' : ''}`}
        style={{
          cursor: 'pointer',
          borderLeftColor: primaryTag?.color || '#cbd5e1',
          borderLeftWidth: 4,
          borderLeftStyle: 'solid',
        }}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            dispatch({ type: 'SELECT_TODO', payload: todo.id });
            onSelectTodo?.(todo.id);
          }
        }}
      >
        <div className="todo-item-content">
          <div className="todo-item-main">
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
              <div
                className="todo-item-title"
                style={{ opacity: isCompleted ? 0.6 : 1 }}
              >
                <span style={{ color: '#999', marginRight: 4, fontSize: 13 }}>#{todo.id}</span>{todo.title}
              </div>
              <ExecutorBadge executor={todo.executor || 'claudecode'} />
            </div>
            {todo.prompt && (
              <div className="todo-item-desc">
                {todo.prompt.length > 60 ? todo.prompt.substring(0, 60) + '...' : todo.prompt}
              </div>
            )}
            <div className="todo-item-tags" style={{ justifyContent: 'space-between' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 4, flexWrap: 'wrap' }}>
                {todoTags.map(t => (
                  <span
                    key={t.id}
                    className="todo-tag-badge"
                    style={{
                      backgroundColor: t.color + '18',
                      color: t.color,
                      border: `1px solid ${t.color}30`,
                    }}
                  >
                    {t.name}
                  </span>
                ))}
                {todo.scheduler_config && (
                  <ClockCircleOutlined
                    style={{
                      fontSize: 12,
                      color: todo.scheduler_enabled ? 'var(--color-warning)' : 'var(--color-text-tertiary)',
                      marginLeft: todoTags.length > 0 ? 4 : 0,
                    }}
                  />
                )}
              </div>
              <span
                style={{
                  fontSize: 11,
                  color: 'var(--color-text-quaternary)',
                  flexShrink: 0,
                  marginLeft: 8,
                }}
                title={relativeTime}
              >
                {relativeTime}
              </span>
            </div>
          </div>
          <div
            className="todo-item-status"
            aria-label="更改任务状态"
          >
            <StatusPicker
              value={todo.status}
              onChange={(newStatus) => handleStatusChange(todo.id, todo.title, todo.prompt || '', newStatus)}
            />
          </div>
        </div>
      </div>
    );
  };

  if (isLoading) {
    return (
      <div className="todo-list-container">
        <SkeletonList />
      </div>
    );
  }

  return (
    <div className="todo-list-container">
      {/* Header */}
      <div className="todo-list-header">
        {/* NTD Logo */}
        <div className="ntd-logo" aria-label="NTD Logo">NTD</div>
        <div className="header-actions">
          <div className="header-toolbar">
            {desktopNavActions.length > 0 && (
              <div className="header-nav-cluster" aria-label="主导航">
                {desktopNavActions.map(action => (
                  <Tooltip key={action.key} title={action.title}>
                    <Button
                      type="text"
                      size="small"
                      icon={action.icon}
                      onClick={action.onClick}
                      className="header-nav-btn"
                      aria-label={action.ariaLabel}
                    />
                  </Tooltip>
                ))}
              </div>
            )}

            {!isMobile && (
              <Dropdown
                menu={{ items: headerMenuItems, onClick: handleHeaderMenuClick }}
                trigger={['click']}
                placement="bottomRight"
              >
                <Button
                  type="text"
                  size="small"
                  icon={<MoreOutlined />}
                  className="header-overflow-btn"
                  aria-label="更多操作"
                />
              </Dropdown>
            )}

          {!isMobile && (
            <div className="header-quick-actions">
              <Tooltip title="智能新建">
                <Button
                  type="text"
                  size="small"
                  icon={<ThunderboltOutlined />}
                  className="header-primary-action header-primary-action-smart"
                  onClick={onOpenSmartCreate}
                  aria-label="智能新建"
                />
              </Tooltip>
              <Tooltip title="新建任务">
                <Button
                  type="text"
                  size="small"
                  icon={<PlusOutlined />}
                  className="header-primary-action header-primary-action-create"
                  onClick={onOpenCreateModal}
                  aria-label="新建任务"
                />
              </Tooltip>
            </div>
          )}

          {isMobile && (
            <div className="header-nav-cluster" aria-label="移动端操作">
              <Tooltip title={themeMode === 'light' ? '切换暗色主题' : '切换亮色主题'}>
                <Button
                  type="text"
                  size="small"
                  icon={themeMode === 'light' ? <MoonOutlined /> : <SunOutlined />}
                  onClick={toggleTheme}
                  className="header-nav-btn"
                  aria-label="切换主题"
                />
              </Tooltip>
              <Button
                type="text"
                size="small"
                icon={<SettingOutlined />}
                onClick={() => onShowSettings?.()}
                className="header-nav-btn"
                aria-label="配置管理"
              />
            </div>
          )}
          </div>
        </div>
      </div>

      {/* Tag filter chips */}
      {tags.length > 0 && (
        <div className="tag-filter-bar">
          <button
            className={`tag-chip ${selectedTagId === null ? 'active' : ''}`}
            onClick={() => dispatch({ type: 'SELECT_TAG', payload: null })}
          >
            全部
          </button>
          {tags.map(tag => (
            <button
              key={tag.id}
              className={`tag-chip ${selectedTagId === tag.id ? 'active' : ''}`}
              style={{ '--tag-color': tag.color } as React.CSSProperties}
              onClick={() => dispatch({ type: 'SELECT_TAG', payload: tag.id })}
            >
              <span className="tag-dot" style={{ backgroundColor: tag.color }} />
              {tag.name}
            </button>
          ))}
        </div>
      )}

      {/* Todo list */}
      <div className="todo-list-content">
        {filteredTodos.length === 0 ? (
          <div className="empty-state">
            <div className="empty-state-icon">
              <InboxOutlined />
            </div>
            <Empty
              description={
                <div style={{ color: 'var(--color-text-tertiary)', fontSize: 14 }}>
                  {selectedTagId ? '该标签下暂无任务' : '暂无任务'}
                  <br />
                  <span style={{ fontSize: 13, marginTop: 4, display: 'inline-block' }}>
                    点击右上角新建按钮创建第一个任务
                  </span>
                </div>
              }
              image={Empty.PRESENTED_IMAGE_SIMPLE}
            />
          </div>
        ) : displayMode === 'grouped' ? (
          // 分组视图：每个项目一个"文件夹"块，块内平铺 todo；
          // 文件夹头部用项目名 + 数量徽标，方便一眼看清每个项目下堆积了多少 todo
          <div className="todo-grouped-list">
            {groupedByProject.map(([key, group]) => {
              const isCollapsed = collapsedGroups.has(key);
              return (
                <div key={key} className="todo-group">
                  <div
                    className="todo-group-header"
                    onClick={() => toggleGroupCollapse(key)}
                    style={{ cursor: 'pointer' }}
                    role="button"
                    tabIndex={0}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        toggleGroupCollapse(key);
                      }
                    }}
                    aria-expanded={!isCollapsed}
                  >
                    <RightOutlined
                      style={{
                        color: 'var(--color-primary)',
                        marginRight: 6,
                        fontSize: 10,
                        transform: isCollapsed ? 'rotate(0deg)' : 'rotate(90deg)',
                        transition: 'transform 0.2s',
                      }}
                    />
                    <span className="todo-group-title">{group.name}</span>
                    <span className="todo-group-count">{group.items.length}</span>
                  </div>
                  {!isCollapsed && (
                    <div className="todo-group-items">
                      {group.items.map(renderTodoItem)}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        ) : (
          filteredTodos.map(renderTodoItem)
        )}
      </div>
    </div>
  );
}
