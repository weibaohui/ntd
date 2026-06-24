import { useState, useEffect, useMemo, useCallback } from 'react';
import { useApp } from '@/hooks/useApp';
import { useIsMobile } from '@/hooks/useIsMobile';
import { Button, Dropdown, Empty, Tooltip, Input, Segmented, Skeleton, Checkbox, Modal, App as AntApp, Form, Select } from 'antd';
import type { MenuProps } from 'antd';
import { ThunderboltOutlined, ClockCircleOutlined, InboxOutlined, DashboardOutlined, ReadOutlined, SettingOutlined, SunOutlined, MoonOutlined, ApartmentOutlined, FolderOpenOutlined, MoreOutlined, SearchOutlined, DownOutlined, SwapOutlined, StopOutlined } from '@ant-design/icons';
import { useTheme } from '@/hooks/useTheme';
import { StatusPicker } from './StatusPicker';
import * as db from '@/utils/database';
import type { ProjectDirectory, Todo } from '@/types';
import { ExecutorBadge } from './ExecutorBadge';
import { LoopListPanel } from './LoopStudioListPanel';
import type { LoopListItem } from '@/types/loop';
import * as dbLoops from '@/utils/database/loops';
import { EXECUTORS_FOR_PICKER } from '@/types/execution';
import { ExecutorPicker } from './todo-drawer/ExecutorPicker';
import { ActionToolbar, type BatchActionItem } from './common/ActionToolbar';
import { formatRelativeTime } from '@/utils/datetime';

interface TodoListProps {
  onOpenCreateModal: () => void;
  onOpenSmartCreate: () => void;
  onSelectTodo?: (todoId: string | number) => void;
  onShowDashboard?: () => void;
  onShowMemorial?: () => void;
  onShowRelationMap?: () => void;
  onShowSettings?: () => void;
  onSelectLoop?: (loopId: number) => void;
  onCreateLoop?: () => void;
  loopUpdateCount?: number;
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

export function TodoList(props: TodoListProps) {
  const { onOpenCreateModal, onOpenSmartCreate, onSelectTodo, onShowDashboard, onShowMemorial, onShowRelationMap, onShowSettings, onSelectLoop, onCreateLoop, loopUpdateCount } = props;
  const { state, dispatch } = useApp();
  const { themeMode, toggleTheme } = useTheme();
  const { todos, selectedTodoId, selectedTagId, selectedWorkspace, tags } = state;
  const { message } = AntApp.useApp();
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
  // 环节列表数据（只在 listMode === 'step' 时使用）
  const [stepList, setStepList] = useState<Todo[]>([]);
  const [stepLoading, setStepLoading] = useState(false);
  // 当前选中的 step id（高亮选中状态）
  const [selectedStepId, setSelectedStepId] = useState<number | null>(null);
  // 环路列表数据（只在 listMode === 'loop' 时使用）
  const [loopList, setLoopList] = useState<LoopListItem[]>([]);
  const [loopLoading, setLoopLoading] = useState(false);
  // 当前选中的 loop id（来自左侧环路列表），用于高亮
  const [selectedLoopId, setSelectedLoopId] = useState<number | null>(null);
  // 项目目录：工作空间选择器需要目录列表
  const [projectDirectories, setProjectDirectories] = useState<ProjectDirectory[]>([]);
  // —— 通用工具栏：跨模式的多选 id 列表 ——
  // 切换 listMode 时清空，避免不同模式 id 串台（todo/step/loop 都是 number id）
  const [selectedIds, setSelectedIds] = useState<number[]>([]);
  // 批量更换执行器 Modal（事项 / 环节共用）
  const [executorModalOpen, setExecutorModalOpen] = useState(false);
  const [pendingExecutorChangeIds, setPendingExecutorChangeIds] = useState<number[]>([]);
  // 强停确认 Modal（环路）
  const [forceStopModalOpen, setForceStopModalOpen] = useState(false);
  const [pendingForceStopIds, setPendingForceStopIds] = useState<number[]>([]);
  // 新建环节 Modal（环节模式「新建」入口）
  const [stepCreateOpen, setStepCreateOpen] = useState(false);
  const [stepCreateForm] = Form.useForm<{ title: string; prompt: string; executor?: string }>();
  const [stepCreating, setStepCreating] = useState(false);

  /**
   * 新建环节：直接创建 kind='step' 的 Todo。
   *
   * 环节与 Todo 已在后端合并，LoopStep 直接引用 Todo。
   */
  const handleCreateStep = useCallback(async (values: { title: string; prompt: string; executor?: string }) => {
    if (!values.title.trim()) { message.error('标题必填'); return; }
    setStepCreating(true);
    try {
      const created = await db.createTodo(values.title.trim(), values.prompt?.trim() ?? '', [], undefined, undefined, 'step' as any);
      message.success(`环节「${created.title}」已创建`);
      setStepCreateOpen(false);
      stepCreateForm.resetFields();
      // 刷新环节列表
      const fresh = await db.getAllTodos('step');
      setStepList(fresh);
    } catch {
      // axios 拦截器已弹错；不关 modal 让用户能继续修改
    } finally {
      setStepCreating(false);
    }
  }, [message, stepCreateForm]);

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

  // 当列表切换到「环节」时，自动加载 step 列表
  useEffect(() => {
    if (listMode !== 'step') return;
    setStepLoading(true);
    db.getAllTodos('step')
      .then(setStepList)
      .catch(() => setStepList([]))
      .finally(() => setStepLoading(false));
  }, [listMode]);

  // 当列表切换到「环路」时，自动加载 loop 列表；或环路变更时刷新
  useEffect(() => {
    if (listMode !== 'loop') return;
    setLoopLoading(true);
    dbLoops.listLoops(selectedWorkspace)
      .then(setLoopList)
      .catch(() => setLoopList([]))
      .finally(() => setLoopLoading(false));
  }, [listMode, loopUpdateCount, selectedWorkspace]);

  // 持久化列表模式到 localStorage
  useEffect(() => {
    localStorage.setItem('ntd_list_mode', listMode);
  }, [listMode]);

  // 切换 listMode 时清空选择：todo/step/loop 虽然 id 都是 number，
  // 但语义不同（同一数字可能指向不同实体），跨模式保留选择会让用户困惑。
  useEffect(() => {
    setSelectedIds([]);
  }, [listMode]);

  // 切换单条 id 的选中态（toggle 语义，工具栏的「全选」用 onSelectionChange 全量覆盖）
  const toggleSelect = useCallback((id: number) => {
    setSelectedIds(prev => prev.includes(id) ? prev.filter(x => x !== id) : [...prev, id]);
  }, []);

  const filteredTodos = useMemo(() => {
    // 步骤模式下不需要过滤 todo（左侧渲染步骤列表）
    if (listMode === 'step') return [];
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

    // 按类型过滤：仅显示事项
    if (listMode === 'item') {
      result = result.filter(todo => (todo.kind ?? 'item') === 'item');
    }

    return result;
  }, [todos, selectedTagId, selectedWorkspace, searchKeyword, listMode]);

  // 通用关键字过滤：环节 / 环路模式也按标题搜索（用户反馈：避免切换时跳界面）
  const filteredStepList = useMemo(() => {
    const keyword = searchKeyword.trim().toLowerCase();
    if (!keyword) return stepList;
    return stepList.filter(s => (s.title || '').toLowerCase().includes(keyword));
  }, [stepList, searchKeyword]);

  const filteredLoopList = useMemo(() => {
    const keyword = searchKeyword.trim().toLowerCase();
    if (!keyword) return loopList;
    return loopList.filter(l => (l.name || '').toLowerCase().includes(keyword));
  }, [loopList, searchKeyword]);

  // 当前 listMode 下"可见可选"的 id 列表，传给 ActionToolbar 用于「全选」/计数。
  // 三种模式都按当前 searchKeyword 过滤后的列表计算，避免「全选」选中隐藏项。
  const visibleIds = useMemo<number[]>(() => {
    if (listMode === 'item') return filteredTodos.map(t => t.id);
    if (listMode === 'step') return filteredStepList.map(s => s.id);
    return filteredLoopList.map(l => l.id);
  }, [listMode, filteredTodos, filteredStepList, filteredLoopList]);

  const handleStatusChange = useCallback(async (todoId: number, title: string, prompt: string, newStatus: string) => {
    try {
      const updated = await db.updateTodo(todoId, title, prompt, newStatus);
      dispatch({ type: 'UPDATE_TODO', payload: updated });
    } catch {
      // ignore: interceptor already shows error
    }
  }, [dispatch]);

  // —— 批量操作：事项模式 ——
  // 「更换执行器」打开 Modal，Modal 内确认后调 db.batchUpdateTodosExecutor
  const openItemChangeExecutor = useCallback((ids: number[]) => {
    setPendingExecutorChangeIds(ids);
    setExecutorModalOpen(true);
  }, []);

  // —— 批量操作：环节模式 ——
  const openStepChangeExecutor = useCallback((ids: number[]) => {
    setPendingExecutorChangeIds(ids);
    setExecutorModalOpen(true);
  }, []);

  // —— 批量操作：环路模式 ——
  const openLoopForceStop = useCallback((ids: number[]) => {
    setPendingForceStopIds(ids);
    setForceStopModalOpen(true);
  }, []);

  // 确认更换执行器（事项 / 环节共用，根据 listMode 路由到不同的 db 函数）
  const handleConfirmChangeExecutor = useCallback(async (executor: string) => {
    const ids = pendingExecutorChangeIds;
    if (ids.length === 0) return;
    setExecutorModalOpen(false);
    setPendingExecutorChangeIds([]);
    try {
      const result = listMode === 'item'
        ? await db.batchUpdateTodosExecutor(ids, executor)
        : await db.batchUpdateTodosExecutor(ids, executor);
      if (result.failed.length === 0) {
        message.success(`已为 ${result.updated.length} 项更换执行器为「${executor}」`);
      } else {
        message.warning(`成功 ${result.updated.length} 条，失败 ${result.failed.length} 条`);
      }
      // 触发列表刷新：loop 通过各自的 reload
      if (listMode === 'item') {
        // item 模式依赖全局 todos 状态，由 useApp 拉取；全量表查一次避免 N 次单条 GET
        const allItems = await db.getAllTodos('item');
        for (const todo of allItems) {
          dispatch({ type: 'UPDATE_TODO', payload: todo });
        }
      } else if (listMode === 'step') {
        // step 模式独立拉取：手动刷新一次
        const fresh = await db.getAllTodos('step');
        setStepList(fresh);
      }
    } catch {
      // axios 拦截器已弹错
    } finally {
      setSelectedIds([]);
    }
  }, [pendingExecutorChangeIds, listMode, message, dispatch]);

  // 确认强停（环路占位实现）
  const handleConfirmForceStop = useCallback(async () => {
    const ids = pendingForceStopIds;
    if (ids.length === 0) return;
    setForceStopModalOpen(false);
    setPendingForceStopIds([]);
    try {
      const result = await dbLoops.forceStopLoops(ids);
      if (result.stopped.length > 0) {
        message.success(`已强停 ${result.stopped.length} 个环路`);
      } else {
        // 占位实现会全部走失败分支，统一提示"开发中"
        message.warning(`环路强停功能开发中（已选 ${ids.length} 个）`);
      }
    } finally {
      setSelectedIds([]);
    }
  }, [pendingForceStopIds, message]);

  // 工具栏配置：按 listMode 切换 createLabel / batchActions。
  // 「新建」按钮统一显示为「新建」2 字（用户能从当前 listMode 知道新建的是什么）；
  // 批量菜单项按模式差异。
  const toolbarConfig = useMemo<{
    createLabel: string;
    batchActions: BatchActionItem<number>[];
  }>(() => {
    if (listMode === 'item') {
      return {
        createLabel: '新建',
        batchActions: [{
          key: 'change-executor',
          label: '更换执行器',
          icon: <SwapOutlined />,
          onClick: openItemChangeExecutor,
        }],
      };
    }
    if (listMode === 'step') {
      return {
        createLabel: '新建',
        batchActions: [{
          key: 'change-executor',
          label: '更换执行器',
          icon: <SwapOutlined />,
          onClick: openStepChangeExecutor,
        }],
      };
    }
    return {
      createLabel: '新建',
      batchActions: [{
        key: 'force-stop',
        label: '强停',
        icon: <StopOutlined />,
        danger: true,
        onClick: openLoopForceStop,
      }],
    };
  }, [listMode, openItemChangeExecutor, openStepChangeExecutor, openLoopForceStop]);

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
    const isChecked = selectedIds.includes(todo.id);

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
          // 工具栏的多选复选框用 position: absolute 浮在卡片左上；
          // 若 .todo-item 不设 position: relative，复选框会逃逸到上层容器，
          // 所有卡片的复选框都叠在同一个屏幕坐标，点击会命中最后渲染的那个。
          position: 'relative',
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
        {/* 多选复选框：position absolute 浮在卡片左上，避免打乱原本的 layout。
            stopPropagation 阻止冒泡到卡片的 onClick（不会触发详情选中）。 */}
        <Checkbox
          checked={isChecked}
          onChange={(e) => { e.stopPropagation(); toggleSelect(todo.id); }}
          onClick={(e) => e.stopPropagation()}
          data-testid={`todo-row-checkbox-${todo.id}`}
          style={{ position: 'absolute', top: 12, left: 12, zIndex: 1 }}
        />
        <div className="todo-item-content" style={{ paddingLeft: 28 }}>
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
                {/* todo_type === 1 已废弃：评审模板自 V15 起迁出至 review_templates 表。 */}
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
              {/* header 只保留「智能新建」（AI 一句话生成）。普通新建入口已迁到
                  ActionToolbar 的「新建事项 / 新建环节 / 新建环路」按钮，避免两处入口混淆。 */}
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

      {/* 搜索框：三种模式都展示（用户反馈：环节/环路原本没有，切换时会跳界面）。
          placeholder 按 listMode 切换，关键词同时匹配事项标题/提示词、环节标题、环路名称。 */}
      <div style={{ padding: '8px 16px', borderBottom: '1px solid var(--color-border-light)' }}>
        <Input
          placeholder={
            listMode === 'item' ? '搜索标题或提示词...'
            : listMode === 'step' ? '搜索环节标题...'
            : '搜索环路名称...'
          }
          prefix={<SearchOutlined style={{ color: '#bfbfbf' }} />}
          value={searchKeyword}
          onChange={(e) => setSearchKeyword(e.target.value)}
          allowClear
          size="small"
        />
      </div>

      {/* 列表选择：事项 / 环节 / 环路 */}
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

      {/* 通用操作工具栏：跨模式的「全选 / 批量 / 新建」入口。
          createLabel / batchActions 按 listMode 在 toolbarConfig 中切换。 */}
      <ActionToolbar
        selectableIds={visibleIds}
        selectedIds={selectedIds}
        onSelectionChange={setSelectedIds}
        createLabel={toolbarConfig.createLabel}
        onCreate={
          listMode === 'item' ? onOpenCreateModal
          : listMode === 'step' ? () => setStepCreateOpen(true)
          : onCreateLoop
        }
        batchActions={toolbarConfig.batchActions}
      />

      {/* 标签过滤：环路模式下不显示，loop 不按 tag 过滤 */}
      {listMode === 'item' && tags.length > 0 && (
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

      {/* 环节列表：在 listMode === 'step' 时显示步骤列表 */}
      {listMode === 'step' ? (
        <div style={{ flex: 1, minHeight: 0, overflow: 'auto', padding: 8 }}>
          {stepLoading ? (
            <Skeleton active style={{ padding: 16 }} />
          ) : filteredStepList.length === 0 ? (
            <Empty
              image={Empty.PRESENTED_IMAGE_SIMPLE}
              description={
                <span style={{ fontSize: 13 }}>
                  暂无环节<br />
                  <span style={{ fontSize: 12, color: 'var(--color-text-tertiary, #94a3b8)' }}>
                    在事项详情中点击"升级为环节"
                  </span>
                </span>
              }
              style={{ marginTop: 32 }}
            />
          ) : (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              {filteredStepList.map(step => (
                <div
                  key={step.id}
                  onClick={() => {
                    setSelectedStepId(step.id);
  
                  }}
                  role="button"
                  tabIndex={0}
                  onKeyDown={(e) => { if (e.key === 'Enter') { }}}
                  style={{
                    position: 'relative',
                    background: selectedStepId === step.id
                      ? 'var(--color-primary-bg, #f0f9ff)'
                      : 'var(--color-bg-elevated, #ffffff)',
                    border: `1px solid ${selectedStepId === step.id
                      ? 'var(--color-primary, #0891b2)'
                      : 'var(--color-border, #e2e8f0)'}`,
                    boxShadow: selectedStepId === step.id
                      ? 'inset 0 0 0 1px var(--color-primary, #0891b2)'
                      : '0 1px 2px color-mix(in srgb, var(--color-text, #0f172a) 6%, transparent)',
                    borderRadius: 10,
                    padding: '12px 12px 14px 16px',
                    cursor: 'pointer',
                    overflow: 'hidden',
                    transition: 'background 200ms, border-color 200ms, box-shadow 200ms, transform 200ms',
                  }}
                  onMouseEnter={(e) => {
                    if (selectedStepId !== step.id) {
                      e.currentTarget.style.borderColor = 'var(--color-text-tertiary, #94a3b8)';
                      e.currentTarget.style.boxShadow = '0 4px 10px color-mix(in srgb, var(--color-text, #0f172a) 10%, transparent)';
                      e.currentTarget.style.transform = 'translateY(-1px)';
                    }
                  }}
                  onMouseLeave={(e) => {
                    if (selectedStepId !== step.id) {
                      e.currentTarget.style.borderColor = 'var(--color-border, #e2e8f0)';
                      e.currentTarget.style.boxShadow = '0 1px 2px color-mix(in srgb, var(--color-text, #0f172a) 6%, transparent)';
                      e.currentTarget.style.transform = 'translateY(0)';
                    }
                  }}
                >
                  {/* 多选复选框：与 todo 卡片一致，绝对定位浮在卡片左上 */}
                  <Checkbox
                    checked={selectedIds.includes(step.id)}
                    onChange={(e) => { e.stopPropagation(); toggleSelect(step.id); }}
                    onClick={(e) => e.stopPropagation()}
                    data-testid={`step-row-checkbox-${step.id}`}
                    style={{ position: 'absolute', top: 14, left: 12, zIndex: 1 }}
                  />
                  {/* 左侧 3px 颜色条（从标签解析颜色） */}
                  <span style={{
                    position: 'absolute', left: 0, top: 0, bottom: 0, width: 3,
                    background: (() => {
                      const tag = tags.find(t => step.tag_ids?.includes(t.id));
                      return tag?.color || 'var(--color-primary, #0891b2)';
                    })(),
                    borderRadius: '10px 0 0 10px',
                  }} />

                  {/* 标题行: #id + 名称，多选模式左侧留出复选框空间 */}
                  <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4, paddingLeft: 28 }}>
                    <span style={{ color: 'var(--color-text-tertiary, #94a3b8)', fontSize: 11, fontFamily: 'monospace' }}>#{step.id}</span>
                    <span style={{
                      fontWeight: 600, fontSize: 14, flex: 1, minWidth: 0,
                      overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                      color: 'var(--color-text, #0f172a)',
                    }}>
                      {step.title}
                    </span>
                  </div>

                  {/* meta: 执行器 */}
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)' }}>
                    {step.executor && (
                      <ExecutorBadge executor={step.executor} style={{ fontSize: 9, padding: '1px 5px' }} />
                    )}
                  </div>

                  {/* 底部 3px 进度条（淡出指示条） */}
                  <div style={{
                    position: 'absolute', left: 0, right: 0, bottom: 0, height: 3,
                    background: (() => {
                      const tag = tags.find(t => step.tag_ids?.includes(t.id));
                      return tag?.color || 'var(--color-primary, #0891b2)';
                    })(),
                    opacity: 0.25,
                    borderRadius: '0 0 10px 10px',
                  }} />
                </div>
              ))}
            </div>
          )}
        </div>
      ) : listMode === 'loop' ? (
        <div style={{ flex: 1, minHeight: 0, overflow: 'auto' }}>
          {loopLoading ? (
            <Skeleton active style={{ padding: 16 }} />
          ) : (
            <LoopListPanel
              loops={filteredLoopList}
              selectedId={selectedLoopId}
              onSelect={(id) => {
                setSelectedLoopId(id);
                onSelectLoop?.(id);
              }}
              onCreate={onCreateLoop}
              selectedIds={selectedIds}
              onToggleSelect={toggleSelect}
              projectDirs={projectDirectories}
              tags={tags}
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

      {/* 批量更换执行器 Modal：事项 / 环节共用。
          关闭即作废，不会触发回调（避免半路取消导致 selectedIds 与 Modal 状态不一致）。 */}
      <Modal
        title={`更换执行器（${pendingExecutorChangeIds.length} 项）`}
        open={executorModalOpen}
        onCancel={() => { setExecutorModalOpen(false); setPendingExecutorChangeIds([]); }}
        footer={null}
        destroyOnClose
      >
        <ExecutorPicker
          executor=""
          executorOptions={EXECUTORS_FOR_PICKER}
          onChange={(v) => handleConfirmChangeExecutor(v)}
        />
      </Modal>

      {/* 强停环路确认 Modal：占位实现，最终会调真实接口 */}
      <Modal
        title="强停环路"
        open={forceStopModalOpen}
        onOk={handleConfirmForceStop}
        onCancel={() => { setForceStopModalOpen(false); setPendingForceStopIds([]); }}
        okText="强停"
        cancelText="取消"
        okButtonProps={{ danger: true }}
        destroyOnClose
      >
        <p>将停止 <strong>{pendingForceStopIds.length}</strong> 个环路关联的所有正在运行的执行。</p>
        <p style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>
          （强停功能开发中，详见 utils/database/loops.ts 的 forceStopLoops 注释。）
        </p>
      </Modal>

      {/* 新建环节 Modal：工具栏「新建环节」触发。
          字段：标题（必填）/ 提示词 / 执行器，复用 createTodo + promote 流程。 */}
      <Modal
        title="新建环节"
        open={stepCreateOpen}
        onCancel={() => { setStepCreateOpen(false); stepCreateForm.resetFields(); }}
        onOk={() => stepCreateForm.submit()}
        confirmLoading={stepCreating}
        okText="创建"
        cancelText="取消"
        destroyOnClose
      >
        <Form
          form={stepCreateForm}
          layout="vertical"
          onFinish={handleCreateStep}
          initialValues={{ executor: 'claudecode' }}
        >
          <Form.Item label="标题" name="title" rules={[{ required: true, message: '标题必填' }]}>
            <Input placeholder="例如：代码审查环节" maxLength={100} />
          </Form.Item>
          <Form.Item label="提示词 (Prompt)" name="prompt" tooltip="描述这个环节能做什么">
            <Input.TextArea rows={5} placeholder="例如：你是资深代码审查员..." maxLength={4000} />
          </Form.Item>
          <Form.Item label="执行器" name="executor">
            <Select
              options={EXECUTORS_FOR_PICKER.map(e => ({ label: e.label, value: e.value }))}
              placeholder="选择执行器"
            />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
}
