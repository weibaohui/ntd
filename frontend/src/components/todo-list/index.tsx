// Todo 列表主组件。
// 子组件 SkeletonRow / SkeletonList / TodoItemRow 仅在本目录内部使用，
// 外部 caller 只 import 主组件 TodoList，不再 re-export。

import { useState, useEffect, useMemo, useCallback } from 'react';
import { useApp } from '@/hooks/useApp';
import { Empty, Input, Skeleton, Modal, App as AntApp } from 'antd';
import { InboxOutlined, SearchOutlined, SwapOutlined, StopOutlined, CopyOutlined, DragOutlined, PauseCircleOutlined, PlayCircleOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';
import type { ProjectDirectory } from '@/types';
import { LoopListPanel } from '@/components/LoopStudioListPanel';
import type { LoopListItem } from '@/types/loop';
import * as dbLoops from '@/utils/database/loops';
import { EXECUTORS_FOR_PICKER } from '@/types/execution';
import { ExecutorPicker } from '@/components/todo-drawer/ExecutorPicker';
import { ActionToolbar, type BatchActionItem } from '@/components/common/ActionToolbar';
import { WorkspaceSwitcher } from '@/components/shell/WorkspaceSwitcher';
import { SkeletonList } from './SkeletonRow';
import { TodoItemRow } from './TodoItemRow';

interface TodoListProps {
  onOpenCreateModal: () => void;
  onSelectTodo?: (todoId: string | number) => void;
  onSelectLoop?: (loopId: number) => void;
  onCreateLoop?: () => void;
  loopUpdateCount?: number;
  forcedListMode?: 'item' | 'loop';
  onListModeChange?: (mode: 'item' | 'loop') => void;
  hideCreateButton?: boolean;
  /** 外部传入的搜索词（来自 ItemsPage 顶层搜索框），优先级高于内部 searchKeyword。 */
  searchKeyword?: string;
}

export function TodoList(props: TodoListProps) {
  const { onOpenCreateModal, onSelectTodo, onSelectLoop, onCreateLoop, loopUpdateCount, forcedListMode, onListModeChange, hideCreateButton, searchKeyword: externalSearchKeyword } = props;
  const { state, dispatch } = useApp();
  const { todos, selectedTodoId, selectedTagId, selectedWorkspace, tags } = state;
  const { message } = AntApp.useApp();
  const [isLoading, setIsLoading] = useState(true);
  // 搜索关键字状态，用于按标题或提示词过滤 todo 列表。
  // 优先使用外部传入的搜索词（ItemsPage 顶层统一搜索框），否则用内部 state。
  const [internalSearch, setInternalSearch] = useState('');
  const searchKeyword = externalSearchKeyword ?? internalSearch;
  const setSearchKeyword = externalSearchKeyword === undefined ? setInternalSearch : () => {};
  // 列表模式：'item' = 事项, 'loop' = 环路
  const [listMode, setListMode] = useState<'item' | 'loop'>(() => {
    const saved = localStorage.getItem('ntd_list_mode');
    if (saved === 'item' || saved === 'loop') return saved;
    return 'item';
  });
  // 环路列表数据（只在 listMode === 'loop' 时使用）
  const [loopList, setLoopList] = useState<LoopListItem[]>([]);
  const [loopLoading, setLoopLoading] = useState(false);
  // 当前选中的 loop id（来自左侧环路列表），用于高亮
  const [selectedLoopId, setSelectedLoopId] = useState<number | null>(null);
  // 项目目录：工作空间选择器需要目录列表
  const [projectDirectories, setProjectDirectories] = useState<ProjectDirectory[]>([]);
  // 标记项目目录初始加载是否已完成（无论成功与否），用于守卫环路加载等工作空间确定后再发起请求
  const [directoriesReady, setDirectoriesReady] = useState(false);
  // —— 通用工具栏：跨模式的多选 id 列表 ——
  // 切换 listMode 时清空，避免不同模式 id 串台（todo/loop 都是 number id）
  const [selectedIds, setSelectedIds] = useState<number[]>([]);
  // 批量更换执行器 Modal（事项模式）
  const [executorModalOpen, setExecutorModalOpen] = useState(false);
  const [pendingExecutorChangeIds, setPendingExecutorChangeIds] = useState<number[]>([]);
  // 强停确认 Modal（环路）
  const [forceStopModalOpen, setForceStopModalOpen] = useState(false);
  const [pendingForceStopIds, setPendingForceStopIds] = useState<number[]>([]);
  // 批量工作空间操作 Modal（事项/环路共用）
  const [workspaceBatchModalOpen, setWorkspaceBatchModalOpen] = useState(false);
  const [workspaceBatchMode, setWorkspaceBatchMode] = useState<'copy' | 'move'>('copy');
  // target 改为工作空间 id（project_directories.id 唯一键），不再用 path。
  const [workspaceBatchTarget, setWorkspaceBatchTarget] = useState<number | null>(null);
  const [pendingWorkspaceBatchIds, setPendingWorkspaceBatchIds] = useState<number[]>([]);
  const [workspaceBatchProcessing, setWorkspaceBatchProcessing] = useState(false);
  const [workspaceBatchContext, setWorkspaceBatchContext] = useState<'item' | 'loop'>('item');
  // 批量暂停/恢复周期执行 Modal
  const [schedulerBatchModalOpen, setSchedulerBatchModalOpen] = useState(false);
  const [schedulerBatchMode, setSchedulerBatchMode] = useState<'pause' | 'resume'>('pause');
  const [pendingSchedulerBatchIds, setPendingSchedulerBatchIds] = useState<number[]>([]);
  const [schedulerBatchProcessing, setSchedulerBatchProcessing] = useState(false);

  useEffect(() => {
    setIsLoading(false);
  }, []);

  // 外部导航强制指定 listMode 时（例如左侧主导航点击"事项/环路"），
  // 需要让内部状态跟随，否则中间列表会停留在用户上次切换的模式。
  useEffect(() => {
    if (!forcedListMode) return;
    if (forcedListMode === listMode) return;
    setListMode(forcedListMode);
  }, [forcedListMode, listMode]);

  // 进入页面时拉取项目目录；后续 Todo 抽屉新增/删除目录时也会主动重拉，确保分组始终准确。
  // 失败时静默处理：分组视图退化为只显示路径即可，不阻塞主流程。
  const reloadProjectDirectories = useCallback(() => {
    db.getProjectDirectories() // 从后端拉取全量目录列表
      .then(dirs => {
        setProjectDirectories(dirs);
        setDirectoriesReady(true);
      })
      .catch(() => {
        setDirectoriesReady(true); // 失败也标记为 ready，避免阻塞环路加载
      });
  }, []);

  // 首次加载目录后，如果没有选中工作空间且存在目录，自动选中第一个。
  // 这样保证用户必须工作在某个空间下，实现工作空间隔离。
  useEffect(() => {
    reloadProjectDirectories(); // 首次加载目录
    // 监听 TodoDrawer 快速新增项目目录的事件，及时刷新分组数据
    const handleDirAdded = () => reloadProjectDirectories();
    window.addEventListener('projectDirectoryAdded', handleDirAdded); // 跨组件通知
    return () => window.removeEventListener('projectDirectoryAdded', handleDirAdded); // 清理：卸载时移除监听
  }, [reloadProjectDirectories]);

  // 当目录列表加载完成后，若当前未选中任何工作空间且存在目录，自动选中第一个。
  // 自动选中用 id 而非 path —— selectedWorkspace 现已统一为 project_directories.id。
  useEffect(() => {
    if (projectDirectories.length > 0 && selectedWorkspace === null) {
      dispatch({ type: 'SELECT_WORKSPACE', payload: projectDirectories[0].id });
    }
  }, [projectDirectories, selectedWorkspace, dispatch]);

  // 当列表切换到「环路」时，自动加载 loop 列表；或环路变更时刷新
  // 必须等待项目目录加载完成且工作空间已确定，否则请求不携带 workspace 参数返回错误数据。
  // selectedWorkspace 现在就是 id（唯一键），直接传给 listLoops 即可，不需要再做 path→id 推导。
  useEffect(() => {
    if (listMode !== 'loop') return;
    if (!directoriesReady) return;
    // 已有目录但尚未选定工作空间 → 等待 auto-select 后再加载
    if (projectDirectories.length > 0 && selectedWorkspace === null) return;
    setLoopLoading(true);
    dbLoops.listLoops(selectedWorkspace)
      .then(setLoopList)
      .catch(() => setLoopList([]))
      .finally(() => setLoopLoading(false));
  }, [listMode, loopUpdateCount, selectedWorkspace, projectDirectories, directoriesReady]);

  // 切换工作空间后按需拉取该工作空间的 todo 列表。
  useEffect(() => {
    if (!directoriesReady) return;
    if (selectedWorkspace == null) return;
    if (listMode !== 'item') return;
    const currentBucket = state.todosByWorkspace?.[selectedWorkspace];
    if (currentBucket !== undefined) return;
    db.getAllTodos(selectedWorkspace).then(todos => {
      dispatch({ type: 'SET_TODOS_BY_WORKSPACE', workspaceId: selectedWorkspace, payload: todos });
    });
  }, [selectedWorkspace, directoriesReady, listMode, state.todosByWorkspace, dispatch]);

  // 持久化列表模式到 localStorage
  useEffect(() => {
    localStorage.setItem('ntd_list_mode', listMode);
  }, [listMode]);

  // 向壳层同步当前列表模式，便于左侧主导航高亮与全局路由状态保持一致。
  useEffect(() => {
    onListModeChange?.(listMode);
  }, [listMode, onListModeChange]);

  // 切换 listMode 时清空选择
  useEffect(() => {
    setSelectedIds([]);
    setSelectedLoopId(null);
  }, [listMode]);

  // 切换单条 id 的选中态
  const toggleSelect = useCallback((id: number) => {
    setSelectedIds(prev => prev.includes(id) ? prev.filter(x => x !== id) : [...prev, id]);
  }, []);

  const filteredTodos = useMemo(() => {
    if (listMode === 'loop') return [];
    let result = selectedTagId !== null
      ? todos.filter(t => t.tag_ids?.includes(selectedTagId))
      : todos;
    if (searchKeyword.trim()) {
      const keyword = searchKeyword.toLowerCase().trim();
      result = result.filter(todo => {
        const title = (todo.title || '').toLowerCase();
        const prompt = (todo.prompt || '').toLowerCase();
        return title.includes(keyword) || prompt.includes(keyword);
      });
    }
    return result;
  }, [todos, selectedTagId, searchKeyword, listMode]);

  const filteredLoopList = useMemo(() => {
    const keyword = searchKeyword.trim().toLowerCase();
    if (!keyword) return loopList;
    return loopList.filter(l => (l.name || '').toLowerCase().includes(keyword));
  }, [loopList, searchKeyword]);

  const visibleIds = useMemo<number[]>(() => {
    if (listMode === 'item') return filteredTodos.map(t => t.id);
    return filteredLoopList.map(l => l.id);
  }, [listMode, filteredTodos, filteredLoopList]);

  // 批量操作 handlers
  const openItemChangeExecutor = useCallback((ids: number[]) => {
    setPendingExecutorChangeIds(ids);
    setExecutorModalOpen(true);
  }, []);

  const openLoopForceStop = useCallback((ids: number[]) => {
    setPendingForceStopIds(ids);
    setForceStopModalOpen(true);
  }, []);

  const openItemCopyWorkspace = useCallback((ids: number[]) => {
    setWorkspaceBatchContext('item');
    setWorkspaceBatchMode('copy');
    setPendingWorkspaceBatchIds(ids);
    setWorkspaceBatchTarget(null);
    setWorkspaceBatchModalOpen(true);
  }, []);

  const openItemMoveWorkspace = useCallback((ids: number[]) => {
    setWorkspaceBatchContext('item');
    setWorkspaceBatchMode('move');
    setPendingWorkspaceBatchIds(ids);
    setWorkspaceBatchTarget(null);
    setWorkspaceBatchModalOpen(true);
  }, []);

  const openLoopCopyWorkspace = useCallback((ids: number[]) => {
    setWorkspaceBatchContext('loop');
    setWorkspaceBatchMode('copy');
    setPendingWorkspaceBatchIds(ids);
    setWorkspaceBatchTarget(null);
    setWorkspaceBatchModalOpen(true);
  }, []);

  const openLoopMoveWorkspace = useCallback((ids: number[]) => {
    setWorkspaceBatchContext('loop');
    setWorkspaceBatchMode('move');
    setPendingWorkspaceBatchIds(ids);
    setWorkspaceBatchTarget(null);
    setWorkspaceBatchModalOpen(true);
  }, []);

  const handleConfirmWorkspaceBatch = useCallback(async () => {
    const ids = pendingWorkspaceBatchIds;
    const target = workspaceBatchTarget;
    if (ids.length === 0 || target == null) return;
    setWorkspaceBatchProcessing(true);
    try {
      const mode = workspaceBatchMode;
      const context = workspaceBatchContext;
      let result: { updated_count: number; total: number };

      if (context === 'item') {
        if (mode === 'copy') {
          result = await db.batchCopyTodosWorkspace(ids, target);
        } else {
          result = await db.batchMoveTodosWorkspace(ids, target);
        }
      } else {
        if (mode === 'copy') {
          result = await dbLoops.batchCopyLoopsWorkspace(ids, target);
        } else {
          result = await dbLoops.batchMoveLoopsWorkspace(ids, target);
        }
      }

      const actionLabel = mode === 'copy' ? '复制' : '移动';
      const targetName = projectDirectories.find(d => d.id === target)?.name
        ?? projectDirectories.find(d => d.id === target)?.path
        ?? String(target);
      if (result.updated_count === result.total) {
        message.success(`已${actionLabel} ${result.updated_count} 项到「${targetName}」`);
      } else {
        message.warning(`${actionLabel}成功 ${result.updated_count} 条，失败 ${result.total - result.updated_count} 条`);
      }

      if (context === 'item') {
        const wid = selectedWorkspace;
        const allItems = wid != null ? await db.getAllTodos(wid) : [];
        if (wid != null) {
          dispatch({ type: 'SET_TODOS_BY_WORKSPACE', workspaceId: wid, payload: allItems });
        }
      } else {
        const loops = await dbLoops.listLoops(selectedWorkspace);
        setLoopList(loops);
      }

      setWorkspaceBatchModalOpen(false);
      setSelectedIds([]);
    } catch (err) {
      message.error(`操作失败: ${err instanceof Error ? err.message : '未知错误'}`);
    } finally {
      setWorkspaceBatchProcessing(false);
    }
  }, [pendingWorkspaceBatchIds, workspaceBatchTarget, workspaceBatchMode, workspaceBatchContext, message, dispatch, selectedWorkspace, projectDirectories]);

  const handleConfirmChangeExecutor = useCallback(async (executor: string) => {
    const ids = pendingExecutorChangeIds;
    if (ids.length === 0) return;
    setExecutorModalOpen(false);
    setPendingExecutorChangeIds([]);
    try {
      const result = await db.batchUpdateTodosExecutor(ids, executor);
      if (result.failed.length === 0) {
        message.success(`已为 ${result.updated.length} 项更换执行器为「${executor}」`);
      } else {
        message.warning(`成功 ${result.updated.length} 条，失败 ${result.failed.length} 条`);
      }
      if (listMode === 'item') {
        const wid = selectedWorkspace;
        if (wid != null) {
          const allItems = await db.getAllTodos(wid);
          dispatch({ type: 'SET_TODOS_BY_WORKSPACE', workspaceId: wid, payload: allItems });
        }
      }
    } catch {
      // axios 拦截器已弹错
    } finally {
      setSelectedIds([]);
    }
  }, [pendingExecutorChangeIds, listMode, message, dispatch]);

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
        message.warning(`环路强停功能开发中（已选 ${ids.length} 个）`);
      }
    } finally {
      setSelectedIds([]);
    }
  }, [pendingForceStopIds, message]);

  // 打开暂停周期执行确认 Modal
  const openItemPauseScheduler = useCallback((ids: number[]) => {
    setSchedulerBatchMode('pause');
    setPendingSchedulerBatchIds(ids);
    setSchedulerBatchModalOpen(true);
  }, []);

  // 打开恢复周期执行确认 Modal
  const openItemResumeScheduler = useCallback((ids: number[]) => {
    setSchedulerBatchMode('resume');
    setPendingSchedulerBatchIds(ids);
    setSchedulerBatchModalOpen(true);
  }, []);

  // 确认暂停/恢复周期执行
  const handleConfirmSchedulerBatch = useCallback(async () => {
    const ids = pendingSchedulerBatchIds;
    if (ids.length === 0) return;
    setSchedulerBatchProcessing(true);
    try {
      const isPause = schedulerBatchMode === 'pause';
      const result = isPause
        ? await db.batchPauseScheduler(ids)
        : await db.batchResumeScheduler(ids);
      const actionLabel = isPause ? '暂停' : '恢复';
      if (result.updated_count === result.total) {
        message.success(`已${actionLabel} ${result.updated_count} 项的周期执行`);
      } else {
        message.warning(`${actionLabel}成功 ${result.updated_count} 条，失败 ${result.total - result.updated_count} 条`);
      }
      // 刷新列表
      const wid = selectedWorkspace;
      if (wid != null) {
        const allItems = await db.getAllTodos(wid);
        dispatch({ type: 'SET_TODOS_BY_WORKSPACE', workspaceId: wid, payload: allItems });
      }
      setSchedulerBatchModalOpen(false);
      setSelectedIds([]);
    } catch (err) {
      message.error(`操作失败: ${err instanceof Error ? err.message : '未知错误'}`);
    } finally {
      setSchedulerBatchProcessing(false);
    }
  }, [pendingSchedulerBatchIds, schedulerBatchMode, message, selectedWorkspace, dispatch]);

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
        }, {
          key: 'copy-workspace',
          label: '复制到',
          icon: <CopyOutlined />,
          onClick: openItemCopyWorkspace,
        }, {
          key: 'move-workspace',
          label: '移动到',
          icon: <DragOutlined />,
          onClick: openItemMoveWorkspace,
        }, {
          key: 'pause-scheduler',
          label: '暂停周期执行',
          icon: <PauseCircleOutlined />,
          onClick: openItemPauseScheduler,
        }, {
          key: 'resume-scheduler',
          label: '恢复周期执行',
          icon: <PlayCircleOutlined />,
          onClick: openItemResumeScheduler,
        }],
      };
    }
    return {
      createLabel: '新建',
      batchActions: [{
        key: 'copy-workspace',
        label: '复制到',
        icon: <CopyOutlined />,
        onClick: openLoopCopyWorkspace,
      }, {
        key: 'move-workspace',
        label: '移动到',
        icon: <DragOutlined />,
        onClick: openLoopMoveWorkspace,
      }, {
        key: 'force-stop',
        label: '强停',
        icon: <StopOutlined />,
        danger: true,
        onClick: openLoopForceStop,
      }],
    };
  }, [listMode, openItemChangeExecutor, openItemCopyWorkspace, openItemMoveWorkspace, openLoopCopyWorkspace, openLoopMoveWorkspace, openLoopForceStop, openItemPauseScheduler, openItemResumeScheduler]);

  if (isLoading) {
    return (
      <div className="todo-list-container">
        <SkeletonList />
      </div>
    );
  }

  return (
    <div className="todo-list-container">
      {/* 搜索框：外部传入 searchKeyword 时由 ItemsPage 统一提供，本组件内不再重复渲染 */}
      {externalSearchKeyword === undefined && (
        <div style={{ padding: '8px 16px', borderBottom: '1px solid var(--color-border-light)' }}>
          <Input
            placeholder={
              listMode === 'item' ? '搜索标题或提示词...'
              : '搜索环路名称...'
            }
            prefix={<SearchOutlined style={{ color: '#bfbfbf' }} />}
            value={searchKeyword}
            onChange={(e) => setSearchKeyword(e.target.value)}
            allowClear
            size="small"
          />
        </div>
      )}

      {/* 通用操作工具栏 */}
      <ActionToolbar
        selectableIds={visibleIds}
        selectedIds={selectedIds}
        onSelectionChange={setSelectedIds}
        createLabel={toolbarConfig.createLabel}
        onCreate={
          listMode === 'item' ? onOpenCreateModal
          : onCreateLoop
        }
        batchActions={toolbarConfig.batchActions}
        hideCreate={hideCreateButton}
      />

      {listMode === 'loop' ? (
        <div style={{ flex: 1, minHeight: 0, overflow: 'auto' }}>
          {loopLoading ? (
            <Skeleton active style={{ padding: 16 }} />
          ) : (
            <LoopListPanel
              loops={filteredLoopList}
              selectedId={selectedLoopId}
              onSelect={(id: number) => {
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
            filteredTodos.map(todo => (
              <TodoItemRow
                key={todo.id}
                todo={todo}
                tags={tags}
                selectedTodoId={selectedTodoId}
                selectedIds={selectedIds}
                onSelectTodo={(id: number) => {
                  dispatch({ type: 'SELECT_TODO', payload: id });
                  onSelectTodo?.(id);
                }}
                onSelect={(id: number) => {
                  dispatch({ type: 'SELECT_TODO', payload: id });
                  onSelectTodo?.(id);
                }}
                onToggleSelect={toggleSelect}
              />
            ))
          )}
        </div>
      )}

      {/* 批量更换执行器 Modal */}
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
          onChange={(v: string) => handleConfirmChangeExecutor(v)}
        />
      </Modal>

      {/* 强停环路确认 Modal */}
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

      {/* 批量工作空间操作 Modal */}
      <Modal
        title={workspaceBatchMode === 'copy' ? '复制到工作空间' : '移动到工作空间'}
        open={workspaceBatchModalOpen}
        onOk={handleConfirmWorkspaceBatch}
        onCancel={() => { setWorkspaceBatchModalOpen(false); setPendingWorkspaceBatchIds([]); }}
        okText={workspaceBatchMode === 'copy' ? '确认复制' : '确认移动'}
        cancelText="取消"
        confirmLoading={workspaceBatchProcessing}
        okButtonProps={{ disabled: workspaceBatchTarget == null }}
        destroyOnClose
      >
        <p>
          {workspaceBatchMode === 'copy' ? '复制' : '移动'} <strong>{pendingWorkspaceBatchIds.length}</strong> 项到目标工作空间：
        </p>
        <div style={{ marginTop: 12 }}>
          <WorkspaceSwitcher
            value={workspaceBatchTarget}
            showAddOption={false}
            onChange={(v) => setWorkspaceBatchTarget(v)}
          />
        </div>
        {workspaceBatchMode === 'copy' && (
          <p style={{ color: 'var(--color-text-tertiary)', fontSize: 12, marginTop: 8 }}>
            复制后，原工作空间和目标工作空间中各有一份相同的条目。
          </p>
        )}
      </Modal>

      {/* 批量暂停/恢复周期执行确认 Modal */}
      <Modal
        title={schedulerBatchMode === 'pause' ? '暂停周期执行' : '恢复周期执行'}
        open={schedulerBatchModalOpen}
        onOk={handleConfirmSchedulerBatch}
        onCancel={() => { setSchedulerBatchModalOpen(false); setPendingSchedulerBatchIds([]); }}
        okText="确认"
        cancelText="取消"
        confirmLoading={schedulerBatchProcessing}
        destroyOnClose
      >
        <p>
          {schedulerBatchMode === 'pause'
            ? <>确定暂停 <strong>{pendingSchedulerBatchIds.length}</strong> 项的周期执行吗？暂停后定时调度将不再触发。</>
            : <>确定恢复 <strong>{pendingSchedulerBatchIds.length}</strong> 项的周期执行吗？将使用原有的调度配置继续运行。</>
          }
        </p>
      </Modal>
    </div>
  );
}
