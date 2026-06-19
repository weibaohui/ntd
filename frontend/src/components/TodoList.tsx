import { useState, useEffect, useMemo, useCallback } from 'react';
import { useApp } from '@/hooks/useApp';
import { useIsMobile } from '@/hooks/useIsMobile';
import { Button, Dropdown, Empty, Tooltip, Input, Segmented, Skeleton } from 'antd';
import type { MenuProps } from 'antd';
import { PlusOutlined, ThunderboltOutlined, ClockCircleOutlined, InboxOutlined, DashboardOutlined, ReadOutlined, SettingOutlined, SunOutlined, MoonOutlined, ApartmentOutlined, FolderOpenOutlined, MoreOutlined, SearchOutlined, DownOutlined } from '@ant-design/icons';
import { useTheme } from '@/hooks/useTheme';
import { StatusPicker } from './StatusPicker';
import * as db from '@/utils/database';
import type { ProjectDirectory, Todo } from '@/types';
import { ExecutorBadge } from './ExecutorBadge';
import { LoopListPanel } from './LoopStudioListPanel';
import type { LoopListItem } from '@/types/loop';
import * as dbLoops from '@/utils/database/loops';
import { formatRelativeTime } from '@/utils/datetime';

interface TodoListProps {
  onOpenCreateModal: () => void;
  onOpenSmartCreate: () => void;
  onSelectTodo?: (todoId: string | number) => void;
  onShowDashboard?: () => void;
  onShowMemorial?: () => void;
  onShowRelationMap?: () => void;
  onShowSteps?: () => void;
  onShowSettings?: () => void;
  onSelectLoop?: (loopId: number) => void;
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

export function TodoList({ onOpenCreateModal, onOpenSmartCreate, onSelectTodo, onShowDashboard, onShowMemorial, onShowRelationMap, onShowSettings, onSelectLoop }: TodoListProps) {
  const { state, dispatch } = useApp();
  const { themeMode, toggleTheme } = useTheme();
  const { todos, selectedTodoId, selectedTagId, selectedWorkspace, tags } = state;
  const isMobile = useIsMobile();
  const [isLoading, setIsLoading] = useState(true);
  // 搜索关键字状态，用于按标题或提示词过滤 todo 列表
  const [searchKeyword, setSearchKeyword] = useState('');
  // 列表模式：'item' = 事项, 'step' = 环节, 'loop' = 环路
  const [listMode, setListMode] = useState<'item' | 'step' | 'loop'>(() => {
    const saved = localStorage.getItem('ntd_list_mode');
    if (saved === 'item' || saved === 'step' || saved === 'loop') return saved;
    return 'item';
  });
  // 环路列表数据（只在 listMode === 'loop' 时使用）
  const [loopList, setLoopList] = useState<LoopListItem[]>([]);
  const [loopLoading, setLoopLoading] = useState(false);
  // 当前选中的 loop id（来自左侧环路列表），用于高亮
  const [selectedLoopId, setSelectedLoopId] = useState<number | null>(null);
  // 项目目录：工作空间选择器需要目录列表
  const [projectDirectories, setProjectDirectories] = useState<ProjectDirectory[]>([]);

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

  // 当列表切换到「环路」时，自动加载 loop 列表；
  // 切换到「事项」或「环节」时不做额外操作。
  useEffect(() => {
    if (listMode !== 'loop') return;
    setLoopLoading(true);
    dbLoops.listLoops()
      .then(setLoopList)
      .catch(() => setLoopList([]))
      .finally(() => setLoopLoading(false));
  }, [listMode]);

  // 持久化列表模式到 localStorage
  useEffect(() => {
    localStorage.setItem('ntd_list_mode', listMode);
  }, [listMode]);

  const filteredTodos = useMemo(() => {
    // 环路模式下不需要过滤 todo（左侧渲染环路列表）
    if (listMode === 'loop') return [];

    // 先按标签过滤
    // 按选中标签过滤：直接读 Todo.tag_ids 即可，
    // 不需要 `as any` — Todo 类型已在 frontend/src/types/todo.ts 中声明该字段。
    // 显式用 `!== null` 判定而不是真值判断：selectedTagId 类型是 number | null，
    // 0 是合法 id，truthy 判定会把合法的 0 当作「未选中」而错误地跳过过滤。
    let result = selectedTagId !== null
      ? todos.filter(t => t.tag_ids?.includes(selectedTagId))
      : todos;
    
    // 按 workspace 过滤：selectedWorkspace 为 null 时显示全部，
    // 否则只显示匹配 workspace 路径的 todo
    if (selectedWorkspace !== null) {
      result = result.filter(todo => todo.workspace === selectedWorkspace);
    }
    
    // 再按关键字搜索（匹配标题或提示词）
    if (searchKeyword.trim()) {
      const keyword = searchKeyword.toLowerCase().trim();
      result = result.filter(todo => {
        const title = (todo.title || '').toLowerCase();
        const prompt = (todo.prompt || '').toLowerCase();
        return title.includes(keyword) || prompt.includes(keyword);
      });
    }

    // 按类型过滤 (v3 kind 列)：'item' = 仅事项，'step' = 仅环节
    // 两个模式都显式过滤，避免事项列表中出现环节。
    if (listMode === 'item') {
      result = result.filter(todo => (todo.kind ?? 'item') === 'item');
    } else if (listMode === 'step') {
      result = result.filter(todo => (todo.kind ?? 'item') === 'step');
    }

    return result;
  }, [todos, selectedTagId, selectedWorkspace, searchKeyword, listMode]);

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

  /**
   * 处理桌面端头部"更多"菜单点击。
   */
  const handleHeaderMenuClick = useCallback<NonNullable<MenuProps['onClick']>>(({ key }) => {
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
  }, [themeMode, onShowSettings]);

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

      {/* Workspace selector - 在搜索框上方，用于切换不同工作空间 */}
      <div style={{ padding: '8px 16px', borderBottom: '1px solid var(--color-border-light)' }}>
        <Dropdown
          menu={{
            items: [
              {
                key: '__all__',
                label: '全部工作空间',
                icon: <ApartmentOutlined />,
              },
              ...projectDirectories.map(dir => ({
                key: dir.path,
                label: dir.name || dir.path,
                icon: <FolderOpenOutlined />,
              })),
              { type: 'divider' as const },
              {
                key: '__manage__',
                label: '管理工作空间',
                icon: <SettingOutlined />,
              },
            ],
            onClick: ({ key }) => {
              if (key === '__manage__') {
                onShowSettings?.();
              } else {
                dispatch({ type: 'SELECT_WORKSPACE', payload: key === '__all__' ? null : key });
              }
            },
          }}
          trigger={['click']}
        >
          <Button
            type="text"
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 8,
              width: '100%',
              justifyContent: 'space-between',
              padding: '8px 12px',
              borderRadius: 'var(--radius-md)',
              background: 'var(--color-bg-elevated)',
              border: '1px solid var(--color-border)',
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <ApartmentOutlined style={{ color: 'var(--color-primary)' }} />
              <span style={{ fontWeight: 500 }}>
                {selectedWorkspace
                  ? projectDirectories.find(d => d.path === selectedWorkspace)?.name || selectedWorkspace
                  : '全部工作空间'
                }
              </span>
            </div>
            <DownOutlined style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }} />
          </Button>
        </Dropdown>
      </div>

      {/* 搜索框：环路模式下隐藏，loop 列表有自己的过滤 */}
      {listMode !== 'loop' && (
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
      )}

      {/* 列表选择：事项 / 专家 / 环路 */}
      <div style={{ padding: '8px 16px', borderBottom: '1px solid var(--color-border-light)' }}>
        <Segmented
          block
          size="small"
          value={listMode}
          onChange={(v) => setListMode(v as 'item' | 'step' | 'loop')}
          options={[
            { label: '事项', value: 'item' },
            { label: '环节', value: 'step' },
            { label: '环路', value: 'loop' },
          ]}
        />
      </div>

      {/* 标签过滤：环路模式下不显示，loop 不按 tag 过滤 */}
      {listMode !== 'loop' && tags.length > 0 && (
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

      {/* 环路列表：在 listMode === 'loop' 时用 LoopListPanel 替代 todo 列表 */}
      {listMode === 'loop' ? (
        <div style={{ flex: 1, minHeight: 0, overflow: 'auto' }}>
          {loopLoading ? (
            <Skeleton active style={{ padding: 16 }} />
          ) : (
            <LoopListPanel
              loops={loopList}
              selectedId={selectedLoopId}
              onSelect={(id) => {
                setSelectedLoopId(id);
                onSelectLoop?.(id);
              }}
            />
          )}
        </div>
      ) : (
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
          ) : (
            filteredTodos.map(renderTodoItem)
          )}
        </div>
      )}
    </div>
  );
}
