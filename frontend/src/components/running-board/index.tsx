// RunningBoard 组件：展示运行中的任务和执行记录。
//
// 子组件（ScheduledTodoCard / ExecutionRecordCard / RunningBoardColumnView / helpers）
// 在本目录内自用，不再从此 index 重新导出；外部 caller 走 RunningBoard.tsx。

import { useState, useMemo, useCallback, useRef, useEffect } from 'react';
import { Tabs, Skeleton, Card } from 'antd';
import { useApp } from '@/hooks/useApp';
import { useIsMobile } from '@/hooks/useIsMobile';
import { useViewState } from '@/hooks/useViewState';
import { RunningRecordDrawer } from '@/components/RunningRecordDrawer';
import {
  useRunningBoard,
  classifyRecords,
  RUNNING_BOARD_COLUMNS,
  useAutoRefreshRunningBoard,
} from '@/hooks/useRunningBoard';
import { STATUS_COLORS } from '@/constants';
import type { ExecutionRecord, RunningBoardColumn } from '@/types';
import { ScheduledTodoCard } from './ScheduledTodoCard';
import { ExecutionRecordCard } from './ExecutionRecordCard';
import { RunningBoardColumnView } from './RunningBoardColumnView';
import * as db from '@/utils/database';

export interface RunningBoardProps {
  searchText?: string;
  hours?: number;
}

export function RunningBoard({ searchText, hours }: RunningBoardProps = {}) {
  const { state } = useApp();
  const { selectTodo } = useViewState();
  const isMobile = useIsMobile();
  const [activeKey, setActiveKey] = useState<RunningBoardColumn>('running');
  const [drawerRecord, setDrawerRecord] = useState<ExecutionRecord | null>(null);

  const { records, scheduledTodos, loading, refresh } = useRunningBoard(state.selectedWorkspace, hours);
  useAutoRefreshRunningBoard(refresh);

  // 切换工作空间后立即拉取该 workspace 的 todo，保证数据最新。
  const { dispatch } = useApp();
  useEffect(() => {
    const wid = state.selectedWorkspace;
    if (wid == null) return;
    db.getAllTodos(wid).then(todos => {
      dispatch({ type: 'SET_TODOS_BY_WORKSPACE', workspaceId: wid, payload: todos });
    });
  }, [state.selectedWorkspace, dispatch]);

  const handleCardClick = useCallback((record: ExecutionRecord) => {
    setDrawerRecord(record);
  }, []);

  const handleDrawerClose = useCallback(() => {
    setDrawerRecord(null);
  }, []);

  // O(1) todo lookup map
  const todoById = useMemo(() => new Map(state.todos.map(t => [t.id, t])), [state.todos]);

  // Apply filters from toolbar
  const filteredRecords = useMemo(() => {
    let result = records;

    // Time filter: only for completed/failed records
    if (hours && hours > 0) {
      const cutoff = Date.now() - hours * 3600 * 1000;
      result = result.filter(r => {
        if (r.status === 'running') return true;
        const finished = r.finished_at ? new Date(r.finished_at).getTime() : 0;
        const started = new Date(r.started_at).getTime();
        return (finished > cutoff) || (started > cutoff);
      });
    }

    // Search filter
    if (searchText?.trim()) {
      const q = searchText.toLowerCase();
      result = result.filter(r => {
        const todo = todoById.get(r.todo_id);
        return (
          (todo?.title?.toLowerCase().includes(q)) ||
          (todo?.prompt?.toLowerCase().includes(q)) ||
          (r.model?.toLowerCase().includes(q)) ||
          (r.executor?.toLowerCase().includes(q))
        );
      });
    }

    return result;
  }, [records, searchText, hours, todoById]);

  const filteredScheduledTodos = useMemo(() => {
    let result = scheduledTodos;

    if (searchText?.trim()) {
      const q = searchText.toLowerCase();
      result = result.filter(t =>
        t.title.toLowerCase().includes(q) || t.prompt?.toLowerCase().includes(q)
      );
    }

    return result;
  }, [scheduledTodos, searchText]);

  const grouped = useMemo(() => classifyRecords(filteredRecords, filteredScheduledTodos), [filteredRecords, filteredScheduledTodos]);

  const stats = useMemo(() => ({
    total: filteredRecords.length + filteredScheduledTodos.length,
    scheduled: grouped.scheduled.scheduledTodos.length,
    running: grouped.running.records.length,
    completed: grouped.completed.records.length,
    reviewing: grouped.reviewing.records.length,
    review_passed: grouped.review_passed.records.length,
    failed: grouped.failed.records.length,
  }), [filteredRecords, filteredScheduledTodos, grouped]);

  const handleSelectTodo = useCallback((todoId: number) => {
    selectTodo(todoId);
  }, [selectTodo]);

  const getTodoTitle = useCallback((todoId: number): string | undefined => {
    return todoById.get(todoId)?.title;
  }, [todoById]);

  // Mobile tab items
  const mobileTabItems = RUNNING_BOARD_COLUMNS.map(col => {
    const data = grouped[col.key];
    const count = data.records.length + data.scheduledTodos.length;
    return {
      key: col.key,
      label: `${col.label} (${count})`,
      children: (
        <div className="running-mobile-list">
          {count === 0 ? (
            <div className="running-column-empty">暂无</div>
          ) : (
            <>
              {data.scheduledTodos.map(todo => (
                <ScheduledTodoCard key={`scheduled-${todo.id}`} todo={todo} onSelectTodo={handleSelectTodo} />
              ))}
              {data.records.map(record => (
                <ExecutionRecordCard
                  key={`record-${record.id}`}
                  record={record}
                  todoTitle={getTodoTitle(record.todo_id)}
                  onSelectTodo={handleSelectTodo}
                  onCardClick={handleCardClick}
                />
              ))}
            </>
          )}
        </div>
      ),
    };
  });

  // Touch swipe for mobile
  const touchStartRef = useRef<{ x: number; y: number; time: number } | null>(null);

  const handleTouchStart = useCallback((e: React.TouchEvent) => {
    const touch = e.touches[0];
    touchStartRef.current = { x: touch.clientX, y: touch.clientY, time: Date.now() };
  }, []);

  const handleTouchEnd = useCallback((e: React.TouchEvent) => {
    if (!touchStartRef.current) return;
    const touch = e.changedTouches[0];
    const deltaX = touch.clientX - touchStartRef.current.x;
    const deltaY = touch.clientY - touchStartRef.current.y;
    const deltaTime = Date.now() - touchStartRef.current.time;
    if (Math.abs(deltaX) > 50 && deltaTime < 300 && Math.abs(deltaX) > Math.abs(deltaY)) {
      const currentIndex = RUNNING_BOARD_COLUMNS.findIndex(c => c.key === activeKey);
      let nextIndex = currentIndex;
      if (deltaX > 0 && currentIndex > 0) nextIndex = currentIndex - 1;
      else if (deltaX < 0 && currentIndex < RUNNING_BOARD_COLUMNS.length - 1) nextIndex = currentIndex + 1;
      if (nextIndex !== currentIndex) setActiveKey(RUNNING_BOARD_COLUMNS[nextIndex].key);
    }
    touchStartRef.current = null;
  }, [activeKey]);

  if (loading && records.length === 0 && scheduledTodos.length === 0) {
    return (
      <div className="running-board">
        <div className="running-board-loading">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="running-column">
              <Card size="small" bodyStyle={{ padding: 12 }}>
                <Skeleton active paragraph={{ rows: 3 }} />
              </Card>
            </div>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="running-board">
      {/* Stats bar */}
      <div className="running-board-stats">
        <span className="running-stat-item" style={{ color: STATUS_COLORS.scheduled }}>
          待触发 <strong>{stats.scheduled}</strong>
        </span>
        <span className="running-stat-divider" />
        <span className="running-stat-item" style={{ color: STATUS_COLORS.running }}>
          运行中 <strong>{stats.running}</strong>
        </span>
        <span className="running-stat-divider" />
        <span className="running-stat-item" style={{ color: STATUS_COLORS.success }}>
          已完成 <strong>{stats.completed}</strong>
        </span>
        <span className="running-stat-divider" />
        <span className="running-stat-item" style={{ color: STATUS_COLORS.reviewing }}>
          评审中 <strong>{stats.reviewing}</strong>
        </span>
        <span className="running-stat-divider" />
        <span className="running-stat-item" style={{ color: STATUS_COLORS.reviewPassed }}>
          评审通过 <strong>{stats.review_passed}</strong>
        </span>
        <span className="running-stat-divider" />
        <span className="running-stat-item" style={{ color: STATUS_COLORS.failed }}>
          失败 <strong>{stats.failed}</strong>
        </span>
        <span className="running-stat-divider" />
        <span className="running-stat-item" style={{ color: 'var(--color-text-secondary)' }}>
          共 <strong>{stats.total}</strong> 条
        </span>
      </div>

      {/* Desktop: 6 columns */}
      {!isMobile && (
        <div className="running-columns-container">
          {RUNNING_BOARD_COLUMNS.map(col => (
            <RunningBoardColumnView
              key={col.key}
              columnKey={col.key}
              label={col.label}
              color={col.color}
              records={grouped[col.key].records}
              scheduledTodos={grouped[col.key].scheduledTodos}
              onSelectTodo={handleSelectTodo}
              onCardClick={handleCardClick}
              getTodoTitle={getTodoTitle}
            />
          ))}
        </div>
      )}

      {/* Mobile: Swipeable Tabs */}
      {isMobile && (
        <div onTouchStart={handleTouchStart} onTouchEnd={handleTouchEnd}>
          <Tabs
            className="running-mobile-tabs"
            activeKey={activeKey}
            onChange={(key) => setActiveKey(key as RunningBoardColumn)}
            items={mobileTabItems}
          />
        </div>
      )}

      {/* Record Detail Drawer */}
      <RunningRecordDrawer
        record={drawerRecord}
        open={!!drawerRecord}
        onClose={handleDrawerClose}
        onRefresh={refresh}
      />
    </div>
  );
}
