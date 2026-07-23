import { useState, useMemo, useCallback, useRef, useEffect } from 'react';
import { Input, Segmented, App, Tabs, Select } from 'antd';
import { SearchOutlined, FolderOutlined } from '@ant-design/icons';
import { useApp } from '@/hooks/useApp';
import { useIsMobile } from '@/hooks/useIsMobile';
import { useKanbanExecutionCache } from '@/hooks/useKanbanExecutionCache';
import { TodoCard } from './TodoCard';
import * as db from '@/utils/database';
import { formatRelativeTime } from '@/utils/datetime';
import type { Todo, ExecutionRecord, ProjectDirectory } from '@/types';
import { TIME_OPTIONS, COLUMNS } from './kanban/constants';
import { getColumnForStatus } from './kanban/helpers';
import type { ColumnDef } from './kanban/constants';

/* ─── Component ─── */

export function KanbanBoard({ searchText: externalSearch, hours: externalHours, onSearchChange, onHoursChange }: { searchText?: string; hours?: number; onSearchChange?: (v: string) => void; onHoursChange?: (h: number) => void } = {}) {
  const { state, dispatch } = useApp();
  const { message } = App.useApp();
  const { todos, tags, selectedTodoId } = state;
  // 本地时间过滤后的 todo 列表：当 hours 有值时，按 API 过滤结果只保留最近 N 小时的 todo。
  // 不与全局 store 共享，避免污染其他视图。
  const [kanbanTodos, setKanbanTodos] = useState<Todo[] | null>(null);
  const [projectDirectories, setProjectDirectories] = useState<ProjectDirectory[]>([]);
  // selectedProject 使用 workspace_id（project_directories.id），不再用 path。
  const [selectedProject, setSelectedProject] = useState<number | null>(null);

  const [internalSearch, setInternalSearch] = useState('');
  const [internalHours, setInternalHours] = useState(24);
  const searchText = externalSearch ?? internalSearch;
  const hours = externalHours ?? internalHours;
  const handleSearchChange = (v: string) => { if (onSearchChange) onSearchChange(v); else setInternalSearch(v); };
  const handleHoursChange = (h: number) => { if (onHoursChange) onHoursChange(h); else setInternalHours(h); };
  const [draggingId, setDraggingId] = useState<number | null>(null);
  const [dragOverStatus, setDragOverStatus] = useState<Todo['status'] | null>(null);
  const [expandedPromptIds, setExpandedPromptIds] = useState<Set<number>>(new Set());
  const [expandedResultIds, setExpandedResultIds] = useState<Set<number>>(new Set());
  const isMobile = useIsMobile();
  const [activeKey, setActiveKey] = useState<Todo['status']>('pending');

  // 渲染用的 todo 列表：有本地过滤结果则用，否则用全局 store
  const displayTodos = kanbanTodos ?? todos;

  /* ─── Execution record cache (delegated to hook) ─── */
  const cache = useKanbanExecutionCache({ todos: displayTodos, storeRecords: state.executionRecords, workspaceId: state.selectedWorkspace });

  // 切换工作空间后立即拉取该 workspace 的 todo，保证数据最新。
  // 先 dispatch 空数组占位清除旧数据，避免闪烁。
  useEffect(() => {
    const wid = state.selectedWorkspace;
    if (wid == null) return;
    dispatch({ type: 'SET_TODOS_BY_WORKSPACE', workspaceId: wid, payload: [] });
    db.getAllTodos(wid).then(todos => {
      dispatch({ type: 'SET_TODOS_BY_WORKSPACE', workspaceId: wid, payload: todos });
    });
  }, [state.selectedWorkspace, dispatch]);

  // 时间分段按钮切换时，用 hours 参数重新拉取（只影响本地 kanbanTodos，不污染全局 store）。
  useEffect(() => {
    const wid = state.selectedWorkspace;
    if (wid == null) return;
    if (hours == null) { setKanbanTodos(null); return; }
    db.getAllTodos(wid, hours).then(setKanbanTodos).catch(() => {});
  }, [state.selectedWorkspace, hours, dispatch]);

  // 加载项目目录列表，供项目维度过滤使用。
  // 监听 'projectDirectoryAdded' 事件：当 TodoDrawer 中快速新增目录后，
  // 此处会重拉列表，保证下拉选项和过滤条件与最新数据同步。
  // 失败时静默回退为空数组，不阻塞看板主流程。
  useEffect(() => {
    const reload = () => {
      db.getProjectDirectories() // 从后端拉取全量目录，数据量小无需分页
        .then(setProjectDirectories) // 更新下拉数据源
        .catch(() => setProjectDirectories([])); // 静默失败：过滤条件缺失时视为"无过滤"
    };
    reload(); // 首次挂载时立即加载
    window.addEventListener('projectDirectoryAdded', reload); // 跨组件刷新：TodoDrawer 新增后通知
    return () => window.removeEventListener('projectDirectoryAdded', reload); // 清理：避免泄漏
  }, []);

  /* ─── Filter by search + time ─── */
  const filteredTodos = useMemo(() => {
    let result = displayTodos;
    const cutoff = hours ? Date.now() - hours * 3600 * 1000 : 0;
    return result.filter(t => {
      // Time filter: only for completed/failed todos
      if ((t.status === 'completed' || t.status === 'failed') && cutoff > 0) {
        const tUpdated = new Date(t.updated_at).getTime();
        if (isNaN(tUpdated) || tUpdated < cutoff) return false;
      }
      // Search filter
      if (searchText.trim()) {
        const q = searchText.toLowerCase();
        return t.title.toLowerCase().includes(q) ||
          (t.prompt && t.prompt.toLowerCase().includes(q));
      }
      return true;
    });
  }, [displayTodos, searchText, hours]);

  /* ─── Group by status ─── */
  const grouped = useMemo(() => {
    const map: Record<Todo['status'], Todo[]> = {
      pending: [],
      running: [],
      completed: [],
      failed: [],
    };
    for (const todo of filteredTodos) {
      if (map[todo.status]) {
        map[todo.status].push(todo);
      } else {
        map.pending.push(todo);
      }
    }
    return map;
  }, [filteredTodos]);

  /* ─── Stats ─── */
  const totalCount = filteredTodos.length;
  const stats = useMemo(() => ({
    pending: grouped.pending.length,
    running: grouped.running.length,
    completed: grouped.completed.length,
    failed: grouped.failed.length,
  }), [grouped]);

  /* ─── Drag & Drop Handlers ─── */

  const handleDragStart = useCallback((todoId: number, e: React.DragEvent) => {
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData('text/plain', String(todoId));
    setDraggingId(todoId);
  }, []);

  const handleDragEnd = useCallback(() => {
    setDraggingId(null);
    setDragOverStatus(null);
  }, []);

  const handleDragOver = useCallback((status: Todo['status'], e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    setDragOverStatus(status);
  }, []);

  const handleDragLeave = useCallback((status: Todo['status']) => {
    setDragOverStatus(prev => prev === status ? null : prev);
  }, []);

  const handleDrop = useCallback(async (targetStatus: Todo['status'], e: React.DragEvent) => {
    e.preventDefault();
    setDraggingId(null);
    setDragOverStatus(null);

    const todoId = parseInt(e.dataTransfer.getData('text/plain'), 10);
    if (isNaN(todoId)) return;

    const todo = todos.find(t => t.id === todoId);
    if (!todo || todo.status === targetStatus) return;

    try {
      const updated = await db.updateTodo(
        todo.workspace_id!,
        todoId,
        todo.title,
        todo.prompt || '',
        targetStatus,
        todo.executor,
      );
      dispatch({ type: 'UPDATE_TODO', payload: updated });
      message.success(`已移动到「${getColumnForStatus(targetStatus).label}」`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : '更新状态失败';
      message.error(msg);
    }
  }, [todos, dispatch, message]);

  /* ─── Toggle expand prompt ─── */
  const togglePrompt = useCallback((todoId: number) => {
    setExpandedPromptIds(prev => {
      const next = new Set(prev);
      if (next.has(todoId)) next.delete(todoId); else next.add(todoId);
      return next;
    });
  }, []);

  /* ─── Toggle result card expansion ─────────────────────────────
   * Responsibilities are intentionally split:
   *   - KanbanBoard manages expandedResultIds (local UI Set state)
   *   - cache.toggleResult handles lazy-fetch of result text
   * Splitting avoids duplicating UI state into the hook and keeps
   * the hook purely about data fetching. ─── */
  const handleToggleResult = useCallback(async (todo: Todo) => {
    // Update local UI expansion state
    setExpandedResultIds(prev => {
      const next = new Set(prev);
      if (next.has(todo.id)) next.delete(todo.id); else next.add(todo.id);
      return next;
    });
    // Trigger lazy-fetch of result text (hook manages cache + loading state)
    await cache.toggleResult(todo);
  }, [cache]);

  /* ─── Render Card ─── */
  const renderCard = (todo: Todo) => {
    const column = getColumnForStatus(todo.status);
    const todoTags = tags.filter(t => todo.tag_ids?.includes(t.id));
    const projectDir = projectDirectories.find(d => d.id === todo.workspace_id);
    const projectName = projectDir?.name || null;
    const isDragging = draggingId === todo.id;
    const isSuccess = todo.status === 'completed';
    const isFinished = todo.status === 'completed' || todo.status === 'failed';
    const promptExpanded = expandedPromptIds.has(todo.id);
    const resultExpanded = expandedResultIds.has(todo.id);
    const todoExecutionRecord = cache.getRecordForTodo(todo.id);

    // Run history: determine which run to display
    const runIdx = cache.selectedRunIndex[todo.id] ?? 0;
    const cachedRun = cache.runDataCache[todo.id]?.[runIdx];
    let resultText: string;
    let displayModel: string | null | undefined;
    let displayUsage: ExecutionRecord['usage'] | null | undefined;
    let displayTriggerType: string | undefined;

    if (runIdx === 0) {
      const recordResult = state.executionRecords[todo.id]?.[0]?.result;
      resultText = recordResult || '';
      displayModel = todoExecutionRecord?.model;
      displayUsage = todoExecutionRecord?.usage;
      displayTriggerType = todoExecutionRecord?.trigger_type;
    } else if (cachedRun) {
      resultText = cachedRun.result || '';
      displayModel = cachedRun.model;
      displayUsage = cachedRun.usage;
      displayTriggerType = cachedRun.trigger_type;
    } else {
      resultText = '';
      displayModel = null;
      displayUsage = null;
    }

    // Rating belongs to the ExecutionRecord, so we must surface the rating
    // for the currently selected run — not just run 0. Each historical run
    // lives in runDataCache; the latest run may also be cached under
    // todoExecutionRecord when available from the store.
    let displayRating: number | null | undefined;
    if (runIdx === 0) {
      displayRating = todoExecutionRecord?.rating;
    } else if (cachedRun) {
      displayRating = cachedRun.rating;
    }

    const isLoadingRun = cache.loadingRunIndex[todo.id] != null && cache.loadingRunIndex[todo.id] === runIdx && runIdx > 0;
    const runCount = cache.totalRunsCache[todo.id] ?? (isFinished ? 1 : 0);

    return (
      <div
        key={todo.id}
        className={`kanban-card ${selectedTodoId === todo.id ? 'selected' : ''} ${isDragging ? 'dragging' : ''} ${isFinished && resultText ? 'has-result' : ''}`}
        draggable
        onDragStart={e => handleDragStart(todo.id, e)}
        onDragEnd={handleDragEnd}
        style={{ borderTop: `3px solid ${column.color}` }}
      >
        <TodoCard
          id={todo.id}
          title={todo.title}
          prompt={todo.prompt}
          resultText={resultText}
          isSuccess={isSuccess}
          showResultSection={isFinished}
          executor={todo.executor}
          time={formatRelativeTime(todo.updated_at)}
          model={displayModel}
          projectName={projectName}
          tags={todoTags}
          usage={displayUsage}
          triggerType={displayTriggerType}
          promptExpanded={promptExpanded}
          resultExpanded={resultExpanded}
          onTogglePrompt={() => togglePrompt(todo.id)}
          onToggleResult={() => handleToggleResult(todo)}
          isLoadingResult={false}
          runCount={runCount}
          selectedRun={runIdx}
          onSelectRun={(index) => cache.handleSelectRun(todo.id, index)}
          isLoadingRun={isLoadingRun}
          rating={displayRating}
        />
      </div>
    );
  };

  /* ─── Render Column ─── */
  const renderColumn = (column: ColumnDef) => {
    const items = grouped[column.status];
    const isOver = dragOverStatus === column.status;

    return (
      <div
        key={column.status}
        className={`kanban-column ${isOver ? 'drag-over' : ''}`}
        onDragOver={e => handleDragOver(column.status, e)}
        onDragLeave={() => handleDragLeave(column.status)}
        onDrop={e => handleDrop(column.status, e)}
      >
        {/* Column Header */}
        <div className="kanban-column-header" style={{ borderBottomColor: column.color }}>
          <div className="kanban-column-title">
            <div
              className="kanban-column-dot"
              style={{ backgroundColor: column.color }}
            />
            <span>{column.label}</span>
            <span className="kanban-column-count">{items.length}</span>
          </div>
        </div>

        {/* Column Body */}
        <div className="kanban-column-body">
          {items.length === 0 ? (
            <div className="kanban-column-empty">
              暂无任务
            </div>
          ) : (
            items.map(renderCard)
          )}
        </div>
      </div>
    );
  };

  /* ─── Touch Swipe Handlers for Mobile Tabs ─── */
  const mobileTabItems = COLUMNS.map(col => ({
    key: col.status,
    label: `${col.label} (${grouped[col.status].length})`,
    children: (
      <div className="kanban-mobile-list">
        {grouped[col.status].length === 0 ? (
          <div className="kanban-column-empty">暂无任务</div>
        ) : (
          grouped[col.status].map(renderCard)
        )}
      </div>
    ),
  }));

  const touchStartRef = useRef<{ x: number; y: number; time: number } | null>(null);

  const handleTouchStart = useCallback((e: React.TouchEvent) => {
    const touch = e.touches[0];
    touchStartRef.current = {
      x: touch.clientX,
      y: touch.clientY,
      time: Date.now(),
    };
  }, []);

  const handleTouchEnd = useCallback((e: React.TouchEvent) => {
    if (!touchStartRef.current) return;

    const touch = e.changedTouches[0];
    const deltaX = touch.clientX - touchStartRef.current.x;
    const deltaY = touch.clientY - touchStartRef.current.y;
    const deltaTime = Date.now() - touchStartRef.current.time;

    // Detect horizontal swipe: threshold 50px, max time 300ms, and horizontal movement > vertical
    if (Math.abs(deltaX) > 50 && deltaTime < 300 && Math.abs(deltaX) > Math.abs(deltaY)) {
      const currentIndex = COLUMNS.findIndex(col => col.status === activeKey);
      let nextIndex = currentIndex;

      if (deltaX > 0 && currentIndex > 0) {
        // Swipe right -> go to previous tab
        nextIndex = currentIndex - 1;
      } else if (deltaX < 0 && currentIndex < COLUMNS.length - 1) {
        // Swipe left -> go to next tab
        nextIndex = currentIndex + 1;
      }

      if (nextIndex !== currentIndex) {
        setActiveKey(COLUMNS[nextIndex].status);
      }
    }

    touchStartRef.current = null;
  }, [activeKey]);

  /* ─── Render ─── */
  return (
    <div className="kanban-board">
      {/* Top Bar — hidden when parent controls filters */}
      {externalSearch === undefined ? (
        <div className="kanban-topbar">
          <div className="kanban-topbar-left">
            <Input
              className="kanban-search"
              placeholder="搜索任务…"
              prefix={<SearchOutlined style={{ color: 'var(--color-text-tertiary)' }} />}
              value={searchText}
              onChange={e => handleSearchChange(e.target.value)}
              allowClear
              size="small"
              style={{ width: 220 }}
            />
            <Segmented
              size="small"
              options={TIME_OPTIONS.map(o => ({ label: o.label, value: o.label }))}
              value={TIME_OPTIONS.find(o => o.value === hours)?.label || '24h'}
              onChange={label => {
                const opt = TIME_OPTIONS.find(o => o.label === label);
                if (opt) handleHoursChange(opt.value);
              }}
              style={{ marginLeft: 8 }}
            />
            <Select
              size="small"
              placeholder="项目过滤"
              allowClear
              value={selectedProject}
              onChange={setSelectedProject}
              style={{ width: 150, marginLeft: 8 }}
              suffixIcon={<FolderOutlined />}
              options={projectDirectories.map(d => ({
                value: d.id, // value 用 workspace_id（唯一键）
                label: d.name,
              }))}
            />
          </div>
          <div className="kanban-topbar-right">
            <span className="kanban-summary-item" style={{ color: '#3b82f6' }}>
              待办 <strong>{stats.pending}</strong>
            </span>
            <span className="kanban-summary-divider" />
            <span className="kanban-summary-item" style={{ color: '#f59e0b' }}>
              进行中 <strong>{stats.running}</strong>
            </span>
            <span className="kanban-summary-divider" />
            <span className="kanban-summary-item" style={{ color: '#22c55e' }}>
              已完成 <strong>{stats.completed}</strong>
            </span>
            <span className="kanban-summary-divider" />
            <span className="kanban-summary-item" style={{ color: '#ef4444' }}>
              失败 <strong>{stats.failed}</strong>
            </span>
            <span className="kanban-summary-divider" />
            <span className="kanban-summary-item" style={{ color: 'var(--color-text-secondary)' }}>
              共 <strong>{totalCount}</strong>
            </span>
          </div>
        </div>
      ) : null}

      {/* Desktop: Columns */}
      {!isMobile && (
        <div className="kanban-columns-container">
          {COLUMNS.map(renderColumn)}
        </div>
      )}

      {/* Mobile: Swipeable Tabs */}
      {isMobile && (
        <div
          onTouchStart={handleTouchStart}
          onTouchEnd={handleTouchEnd}
        >
          <Tabs
            className="kanban-mobile-tabs"
            activeKey={activeKey}
            onChange={(key) => setActiveKey(key as Todo['status'])}
            items={mobileTabItems}
          />
        </div>
      )}
    </div>
  );
}
