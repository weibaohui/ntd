import { useState, useMemo, useCallback, useRef } from 'react';
import { Tabs, Tag, Skeleton, Card } from 'antd';
import {
  ClockCircleOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
  LoadingOutlined,
  EyeOutlined,
  TrophyOutlined,
} from '@ant-design/icons';
import { useApp } from '@/hooks/useApp';
import { useIsMobile } from '@/hooks/useIsMobile';
import { useViewState } from '@/hooks/useViewState';
import { ExecutorBadge } from './ExecutorBadge';
import { RunningRecordDrawer } from './RunningRecordDrawer';
import {
  useRunningBoard,
  classifyRecords,
  RUNNING_BOARD_COLUMNS,
  useAutoRefreshRunningBoard,
} from '@/hooks/useRunningBoard';
import { formatRelativeTime } from '@/utils/datetime';
import { formatDuration } from '@/utils/format';
import { STATUS_COLORS } from '@/constants';
import type { ExecutionRecord, ScheduledTodo, RunningBoardColumn } from '@/types';

/* ─── Helpers ─── */

function formatNextRunAt(nextRunAt: string | null): string {
  if (!nextRunAt) return '-';
  const now = Date.now();
  const target = new Date(nextRunAt).getTime();
  const diff = target - now;
  if (diff < 0) return '即将触发';
  if (diff < 60_000) return `${Math.floor(diff / 1000)}s 后`;
  if (diff < 3600_000) return `${Math.floor(diff / 60_000)}m 后`;
  return `${(diff / 3600_000).toFixed(1)}h 后`;
}

const COLUMN_ICONS: Record<RunningBoardColumn, React.ReactNode> = {
  scheduled: <ClockCircleOutlined />,
  running: <LoadingOutlined />,
  completed: <CheckCircleOutlined />,
  reviewing: <EyeOutlined />,
  review_passed: <TrophyOutlined />,
  failed: <CloseCircleOutlined />,
};

/* ─── Scheduled Todo Card ─── */

function ScheduledTodoCard({ todo, onSelectTodo }: { todo: ScheduledTodo; onSelectTodo?: (id: number) => void }) {
  const handleClick = useCallback(() => {
    onSelectTodo?.(todo.id);
  }, [todo.id, onSelectTodo]);

  return (
    <div className="running-card scheduled-card">
      <div className="running-card-header">
        <span className="running-card-title" onClick={handleClick}>{todo.title}</span>
      </div>
      <div className="running-card-meta">
        {todo.executor && <ExecutorBadge executor={todo.executor} />}
        <Tag color="purple" style={{ marginLeft: 4 }}>
          {todo.scheduler_config || 'cron'}
        </Tag>
        {todo.scheduler_timezone && (
          <Tag style={{ marginLeft: 4 }}>{todo.scheduler_timezone}</Tag>
        )}
      </div>
      <div className="running-card-footer">
        <span className="running-card-next-run">
          下次: {formatNextRunAt(todo.scheduler_next_run_at)}
        </span>
      </div>
    </div>
  );
}

/* ─── Execution Record Card ─── */

function ExecutionRecordCard({
  record,
  todoTitle,
  onSelectTodo,
  onCardClick,
}: {
  record: ExecutionRecord;
  todoTitle?: string;
  onSelectTodo?: (id: number) => void;
  onCardClick?: (record: ExecutionRecord) => void;
}) {
  const duration = record.usage?.duration_ms || null;
  const cost = record.usage?.total_cost_usd;

  const handleTitleClick = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    onSelectTodo?.(record.todo_id);
  }, [record.todo_id, onSelectTodo]);

  const handleCardClick = useCallback(() => {
    onCardClick?.(record);
  }, [record, onCardClick]);

  const statusTag = useMemo(() => {
    if (record.status === 'running') return <Tag color="orange">运行中</Tag>;
    if (record.status === 'failed') return <Tag color="red">失败</Tag>;
    if (record.last_review_status === 'pending') return <Tag color="cyan">评审中</Tag>;
    if (record.last_review_status === 'success') return <Tag color="green">评审通过</Tag>;
    if (record.last_review_status === 'failed') return <Tag color="red">评审失败</Tag>;
    return <Tag color="green">成功</Tag>;
  }, [record.status, record.last_review_status]);

  return (
    <div className={`running-card execution-card status-${record.status}`} onClick={handleCardClick}>
      <div className="running-card-header">
        <span className="running-card-title" onClick={handleTitleClick}>
          {todoTitle || `Todo #${record.todo_id}`}
        </span>
        {statusTag}
      </div>
      <div className="running-card-meta">
        {record.executor && <ExecutorBadge executor={record.executor} />}
        {record.model && <Tag style={{ marginLeft: 4 }}>{record.model}</Tag>}
        {record.trigger_type && record.trigger_type !== 'manual' && (
          <Tag color="blue" style={{ marginLeft: 4 }}>{record.trigger_type}</Tag>
        )}
      </div>
      <div className="running-card-time">
        {formatRelativeTime(record.started_at)}
        {duration != null && <span className="running-card-duration"> · {formatDuration(duration)}</span>}
      </div>
      {(record.rating != null || cost != null) && (
        <div className="running-card-stats">
          {record.rating != null && (
            <Tag color={record.rating >= 80 ? 'green' : record.rating >= 50 ? 'orange' : 'red'}>
              {record.rating}分
            </Tag>
          )}
          {cost != null && cost > 0 && (
            <span className="running-card-cost">${cost.toFixed(4)}</span>
          )}
        </div>
      )}
    </div>
  );
}

/* ─── Column Component ─── */

function RunningBoardColumnView({
  columnKey,
  label,
  color,
  records,
  scheduledTodos,
  onSelectTodo,
  onCardClick,
  getTodoTitle,
}: {
  columnKey: RunningBoardColumn;
  label: string;
  color: string;
  records: ExecutionRecord[];
  scheduledTodos: ScheduledTodo[];
  onSelectTodo?: (id: number) => void;
  onCardClick?: (record: ExecutionRecord) => void;
  getTodoTitle?: (id: number) => string | undefined;
}) {
  const count = records.length + scheduledTodos.length;

  return (
    <div className="running-column">
      <div className="running-column-header" style={{ borderBottomColor: color }}>
        <div className="running-column-title">
          <div className="running-column-dot" style={{ backgroundColor: color }} />
          <span className="running-column-icon" style={{ color }}>{COLUMN_ICONS[columnKey]}</span>
          <span>{label}</span>
          <span className="running-column-count">{count}</span>
        </div>
      </div>
      <div className="running-column-body">
        {count === 0 ? (
          <div className="running-column-empty">暂无</div>
        ) : (
          <>
            {scheduledTodos.map(todo => (
              <ScheduledTodoCard key={`scheduled-${todo.id}`} todo={todo} onSelectTodo={onSelectTodo} />
            ))}
            {records.map(record => (
              <ExecutionRecordCard
                key={`record-${record.id}`}
                record={record}
                todoTitle={getTodoTitle?.(record.todo_id)}
                onSelectTodo={onSelectTodo}
                onCardClick={onCardClick}
              />
            ))}
          </>
        )}
      </div>
    </div>
  );
}

/* ─── Main Component ─── */

export interface RunningBoardProps {
  searchText?: string;
  hours?: number;
  /** 工作空间 ID（project_directories.id），不再用 path。 */
  selectedProject?: number | null;
}

export function RunningBoard({ searchText, hours, selectedProject }: RunningBoardProps = {}) {
  const { state } = useApp();
  const { selectTodo } = useViewState();
  const isMobile = useIsMobile();
  const [activeKey, setActiveKey] = useState<RunningBoardColumn>('running');
  const [drawerRecord, setDrawerRecord] = useState<ExecutionRecord | null>(null);

  const { records, scheduledTodos, loading, refresh } = useRunningBoard();
  useAutoRefreshRunningBoard(refresh);

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

    // Project filter：按 workspace_id 匹配
    if (selectedProject != null) {
      result = result.filter(r => {
        const todo = todoById.get(r.todo_id);
        return todo?.workspace_id === selectedProject;
      });
    }

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
  }, [records, searchText, hours, selectedProject, todoById]);

  const filteredScheduledTodos = useMemo(() => {
    let result = scheduledTodos;

    if (selectedProject != null) {
      result = result.filter(t => t.workspace_id === selectedProject);
    }

    if (searchText?.trim()) {
      const q = searchText.toLowerCase();
      result = result.filter(t =>
        t.title.toLowerCase().includes(q) || t.prompt?.toLowerCase().includes(q)
      );
    }

    return result;
  }, [scheduledTodos, searchText, selectedProject]);

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
