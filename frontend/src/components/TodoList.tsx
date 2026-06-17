import { useState, useEffect, useMemo, useCallback } from 'react';
import { useApp } from '@/hooks/useApp';
import { useIsMobile } from '@/hooks/useIsMobile';
import { Button, Dropdown, Empty, Tooltip, Input } from 'antd';
import type { MenuProps } from 'antd';
import { PlusOutlined, ThunderboltOutlined, ClockCircleOutlined, InboxOutlined, DashboardOutlined, ReadOutlined, SettingOutlined, SunOutlined, MoonOutlined, ApartmentOutlined, UnorderedListOutlined, FolderOpenOutlined, RightOutlined, MoreOutlined, SearchOutlined } from '@ant-design/icons';
import { useTheme } from '@/hooks/useTheme';
import { StatusPicker } from './StatusPicker';
import * as db from '@/utils/database';
import type { ProjectDirectory, Todo } from '@/types';
import { ExecutorBadge } from './ExecutorBadge';
import { formatRelativeTime } from '@/utils/datetime';

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

const DISPLAY_MODE_KEY = 'app_display_mode';

// 从 localStorage 读取上次的显示模式，读取失败时默认平铺
function getInitialDisplayMode(): ListDisplayMode {
  try {
    const saved = localStorage.getItem(DISPLAY_MODE_KEY);
    if (saved === 'flat' || saved === 'grouped') return saved;
  } catch {}
  return 'flat';
}

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
      ariaLabel: '仪表盘',
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
  // 搜索关键字状态，用于按标题或提示词过滤 todo 列表
  const [searchKeyword, setSearchKeyword] = useState('');
  // 显示模式：从 localStorage 读取上次用户选择，刷新后保留
  const [displayMode, setDisplayMode] = useState<ListDisplayMode>(getInitialDisplayMode);
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

  const filteredTodos = useMemo(() => {
    // 先按标签过滤
    // 按选中标签过滤：直接读 Todo.tag_ids 即可，
    // 不需要 `as any` — Todo 类型已在 frontend/src/types/todo.ts 中声明该字段。
    // 显式用 `!== null` 判定而不是真值判断：selectedTagId 类型是 number | null，
    // 0 是合法 id，truthy 判定会把合法的 0 当作「未选中」而错误地跳过过滤。
    let result = selectedTagId !== null
      ? todos.filter(t => t.tag_ids?.includes(selectedTagId))
      : todos;
    
    // 再按关键字搜索（匹配标题或提示词）
    if (searchKeyword.trim()) {
      const keyword = searchKeyword.toLowerCase().trim();
      result = result.filter(todo => {
        const title = (todo.title || '').toLowerCase();
        const prompt = (todo.prompt || '').toLowerCase();
        return title.includes(keyword) || prompt.includes(keyword);
      });
    }
    
    return result;
  }, [todos, selectedTagId, searchKeyword]);

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
      // 切换后立即持久化到 localStorage，确保刷新后不丢失
      const next = displayMode === 'flat' ? 'grouped' : 'flat';
      setDisplayMode(next);
      try { localStorage.setItem(DISPLAY_MODE_KEY, next); } catch {}
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
            {displayMode === 'flat' ? '分组' : '平铺'}
          </span>
        ),
      },
      {
        key: 'theme',
        icon: themeMode === 'light' ? <MoonOutlined /> : <SunOutlined />,
        label: themeMode === 'light' ? '暗色' : '亮色',
      },
    ];

    if (onShowSettings) {
      items.push(
        { type: 'divider' },
        {
          key: 'settings',
          icon: <SettingOutlined />,
          label: '设置',
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
    // tag_ids 在 Todo 类型中是必填 number[]，但历史接口偶发返回缺失字段，
    // 所以用可选链 + 空数组兜底，避免运行时崩溃。
    const todoTags = todo.tag_ids?.map(id => tagMap.get(id)).filter((t): t is typeof tags[0] => !!t) ?? [];
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
                {todo.todo_type === 1 && (
                  <span
                    className="todo-tag-badge"
                    style={{
                      backgroundColor: '#722ed118',
                      color: '#722ed1',
                      border: '1px solid #722ed130',
                    }}
                    title="评审任务：自动评审时复制此 todo"
                  >
                    [评审任务]
                  </span>
                )}
                {todo.todo_type === 2 && (
                  <span
                    className="todo-tag-badge"
                    style={{
                      backgroundColor: '#13c2c218',
                      color: '#13c2c2',
                      border: '1px solid #13c2c230',
                    }}
                    title={`评审实例 (原 todo #${todo.parent_todo_id ?? '?'})`}
                  >
                    [评审]
                  </span>
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
              <Tooltip title={displayMode === 'flat' ? '分组' : '平铺'}>
                <Button
                  type="text"
                  size="small"
                  icon={displayMode === 'flat' ? <FolderOpenOutlined /> : <UnorderedListOutlined />}
                  onClick={() => {
                  const next = displayMode === 'flat' ? 'grouped' : 'flat';
                  setDisplayMode(next);
                  try { localStorage.setItem(DISPLAY_MODE_KEY, next); } catch {}
                }}
                  className="header-nav-btn"
                  aria-label={displayMode === 'flat' ? '分组' : '平铺'}
                  aria-pressed={displayMode === 'grouped'}
                />
              </Tooltip>
              <Tooltip title={themeMode === 'light' ? '暗色' : '亮色'}>
                <Button
                  type="text"
                  size="small"
                  icon={themeMode === 'light' ? <MoonOutlined /> : <SunOutlined />}
                  onClick={toggleTheme}
                  className="header-nav-btn"
                  aria-label={themeMode === 'light' ? '暗色' : '亮色'}
                />
              </Tooltip>
              <Tooltip title="设置">
                <Button
                  type="text"
                  size="small"
                  icon={<SettingOutlined />}
                  onClick={() => onShowSettings?.()}
                  className="header-nav-btn"
                  aria-label="设置"
                />
              </Tooltip>
            </div>
          )}
          </div>
        </div>
      </div>

      {/* Search box - 在 todo 列表上方，按标题或提示词关键字搜索 */}
      {/* 横线颜色用 --color-border-light 而不是硬编码：useTheme 通过切换 documentElement 的 data-theme 来驱动 CSS 变量；浅色=#f1f5f9、暗色=#262637，与仓库其他 7 处分隔线（TodoDrawer/HistoryList/SessionDetailDrawer 等）保持一致，避免暗色下出现一条突兀的浅线 (issue #602) */}
      <div style={{ padding: '8px 16px', borderBottom: '1px solid var(--color-border-light)' }}>
        <Input
          placeholder="搜索标题或提示词..."
          prefix={<SearchOutlined style={{ color: '#bfbfbf' }} />}
          value={searchKeyword}
          onChange={(e) => setSearchKeyword(e.target.value)}
          allowClear
          size="small"
        />
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
