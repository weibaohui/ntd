import { useState, useCallback, useEffect, useRef } from 'react';
import * as db from '@/utils/database';
import type { ExecutionRecord, ScheduledTodo, RunningBoardColumn } from '@/types';

export interface RunningBoardState {
  records: ExecutionRecord[];
  scheduledTodos: ScheduledTodo[];
  loading: boolean;
  total: number;
  page: number;
  limit: number;
  refresh: () => Promise<void>;
  setPage: (page: number) => void;
}

export interface ColumnData {
  key: RunningBoardColumn;
  label: string;
  color: string;
  records: ExecutionRecord[];
  scheduledTodos: ScheduledTodo[];
}

export const RUNNING_BOARD_COLUMNS: { key: RunningBoardColumn; label: string; color: string }[] = [
  { key: 'scheduled', label: '待触发', color: '#8b5cf6' },
  { key: 'running', label: '运行中', color: '#f59e0b' },
  { key: 'completed', label: '已完成', color: '#22c55e' },
  { key: 'reviewing', label: '评审中', color: '#06b6d4' },
  { key: 'review_passed', label: '评审通过', color: '#10b981' },
  { key: 'failed', label: '失败', color: '#ef4444' },
];

function classifyRecord(record: ExecutionRecord): RunningBoardColumn {
  if (record.status === 'running') return 'running';
  if (record.status === 'failed') return 'failed';
  if (record.last_review_status === 'pending') return 'reviewing';
  if (record.last_review_status === 'success') return 'review_passed';
  if (record.last_review_status === 'failed' || record.last_review_status === 'interrupted') return 'failed';
  if (record.status === 'success') return 'completed';
  // 防御性兜底：理论上不可达（status 只有 running/success/failed 三种合法值）
  return 'failed';
}

export function useRunningBoard(workspaceId?: number | null): RunningBoardState {
  const [records, setRecords] = useState<ExecutionRecord[]>([]);
  const [scheduledTodos, setScheduledTodos] = useState<ScheduledTodo[]>([]);
  const [loading, setLoading] = useState(true);
  const [total, setTotal] = useState(0);
  const mountedRef = useRef(true);

  const refresh = useCallback(async () => {
    try {
      setLoading(true);
      // 运行看板不分页，拉取最近的一批数据即可
      const data = await db.getRunningBoardData(undefined, 200, workspaceId ?? undefined);
      if (mountedRef.current) {
        setRecords(data.records);
        setScheduledTodos(data.scheduled_todos);
        setTotal(data.total);
      }
    } catch {
      if (mountedRef.current) {
        setRecords([]);
        setScheduledTodos([]);
      }
    } finally {
      if (mountedRef.current) setLoading(false);
    }
  }, [workspaceId]);

  useEffect(() => {
    mountedRef.current = true;
    refresh();
    return () => { mountedRef.current = false; };
  }, [refresh]);

  return { records, scheduledTodos, loading, total, refresh, page: 1, limit: 200, setPage: () => {} };
}

export function classifyRecords(
  records: ExecutionRecord[],
  scheduledTodos: ScheduledTodo[],
): Record<RunningBoardColumn, { records: ExecutionRecord[]; scheduledTodos: ScheduledTodo[] }> {
  const groups: Record<RunningBoardColumn, { records: ExecutionRecord[]; scheduledTodos: ScheduledTodo[] }> = {
    scheduled: { records: [], scheduledTodos: [] },
    running: { records: [], scheduledTodos: [] },
    completed: { records: [], scheduledTodos: [] },
    reviewing: { records: [], scheduledTodos: [] },
    review_passed: { records: [], scheduledTodos: [] },
    failed: { records: [], scheduledTodos: [] },
  };

  groups.scheduled.scheduledTodos = scheduledTodos;

  for (const record of records) {
    const col = classifyRecord(record);
    groups[col].records.push(record);
  }

  return groups;
}

export function useAutoRefreshRunningBoard(refresh: () => Promise<void>): void {
  const refreshRef = useRef(refresh);
  refreshRef.current = refresh;

  useEffect(() => {
    const handleRefresh = () => { refreshRef.current(); };
    const handleFinished = () => { setTimeout(() => refreshRef.current(), 1000); };

    window.addEventListener('reviewStatusChanged', handleRefresh);
    window.addEventListener('executionStarted', handleRefresh);
    window.addEventListener('executionFinished', handleFinished);
    return () => {
      window.removeEventListener('reviewStatusChanged', handleRefresh);
      window.removeEventListener('executionStarted', handleRefresh);
      window.removeEventListener('executionFinished', handleFinished);
    };
  }, []);
}
