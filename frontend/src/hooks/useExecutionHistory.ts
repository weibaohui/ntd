/**
 * useExecutionHistory — execution record & log loading for a given todo.
 *
 * Encapsulates:
 * - Execution record pagination, filtering, and selection state
 * - Single-record detail loading (with active-request guard via activeRecordIdRef)
 * - Log pagination
 * - Auto-refresh when an execution finishes (prevIsExecutingRef pattern)
 *
 * All state is local to the hook; callers pass selectedTodoId and receive
 * derived values. The store dispatch is used internally to sync records.
 */

import { useState, useEffect, useRef, useCallback } from 'react';
import type { ExecutionRecord, LogEntry, ExecutionSummary } from '@/types';
import * as db from '@/utils/database';

interface UseExecutionHistoryOptions {
  selectedTodoId: number | null;
  /** Records already in the global store for this todo */
  storeRecords: ExecutionRecord[];
  /** Dispatch from useApp() — used to sync fetched records back into global state */
  dispatch: React.Dispatch<unknown>;
}

interface UseExecutionHistoryResult {
  // Record selection
  selectedHistoryRecordId: number | null;
  setSelectedHistoryRecordId: (id: number | null) => void;

  // Record list
  records: ExecutionRecord[];
  historyPage: number;
  historyLimit: number;
  historyTotal: number;
  historyStatusFilter: 'all' | 'running' | 'success' | 'failed';
  setHistoryStatusFilter: (f: 'all' | 'running' | 'success' | 'failed') => void;
  isExecuting: boolean;
  summary: ExecutionSummary | null;

  // Record detail
  selectedHistoryRecord: ExecutionRecord | null;
  isLoadingDetail: boolean;

  // Logs
  paginatedLogs: LogEntry[];
  logsTotal: number;
  logsPage: number;
  logsPerPage: number;
  isLoadingLogs: boolean;

  // Actions
  loadExecutionRecords: (page?: number, limit?: number) => Promise<void>;
  loadLogs: (recordId: number, page: number) => Promise<void>;
  refreshSingleRecord: (recordId: number) => Promise<void>;
  handleHistoryPageChange: (page: number, pageSize: number) => void;
}

export function useExecutionHistory({
  selectedTodoId,
  storeRecords,
  dispatch,
}: UseExecutionHistoryOptions): UseExecutionHistoryResult {
  // ─── Record list pagination ────────────────────────────────
  const [historyPage, setHistoryPage] = useState(1);
  const [historyLimit, setHistoryLimit] = useState(5);
  const [historyTotal, setHistoryTotal] = useState(0);
  const [historyStatusFilter, setHistoryStatusFilter] = useState<'all' | 'running' | 'success' | 'failed'>('all');
  const [summary, setSummary] = useState<ExecutionSummary | null>(null);

  // ─── Record selection ──────────────────────────────────────
  const [selectedHistoryRecordId, setSelectedHistoryRecordId] = useState<number | null>(null);
  const [selectedHistoryRecordDetail, setSelectedHistoryRecordDetail] = useState<ExecutionRecord | null>(null);
  const [isLoadingDetail, setIsLoadingDetail] = useState(false);

  // ─── Logs ─────────────────────────────────────────────────
  const [paginatedLogs, setPaginatedLogs] = useState<LogEntry[]>([]);
  const [logsTotal, setLogsTotal] = useState(0);
  const [logsPage, setLogsPage] = useState(1);
  const [logsPerPage] = useState(200);
  const [isLoadingLogs, setIsLoadingLogs] = useState(false);

  // Guards stale responses when selection changes mid-request
  const activeRecordIdRef = useRef<number | null>(null);

  // ─── Load records ──────────────────────────────────────────

  const loadExecutionRecords = useCallback(async (page = 1, limit = historyLimit) => {
    if (!selectedTodoId) return;
    try {
      const statusFilter = historyStatusFilter === 'all' ? undefined : historyStatusFilter;
      const pageData = await db.getExecutionRecords(selectedTodoId, page, limit, statusFilter);
      dispatch({
        type: 'SET_EXECUTION_RECORDS',
        payload: { todoId: selectedTodoId, records: pageData.records },
      });
      setHistoryPage(pageData.page);
      setHistoryLimit(pageData.limit);
      setHistoryTotal(pageData.total);
    } catch {
      // ignore: interceptor already shows error toast
    }
  }, [selectedTodoId, historyLimit, historyStatusFilter, dispatch]);

  const refreshSingleRecord = useCallback(async (recordId: number) => {
    if (!selectedTodoId) return;
    try {
      const record = await db.getExecutionRecord(recordId);
      dispatch({ type: 'UPDATE_EXECUTION_RECORD', payload: { todoId: selectedTodoId, record } });
      // 同步更新详情面板使用的本地 detail state，避免评分等同步更新后
      // 详情面板仍展示旧值（详情面板优先使用 selectedHistoryRecordDetail，
      // 这里只在被刷新的 record 正好是当前选中的那一条时才覆盖）。
      if (activeRecordIdRef.current === recordId) {
        setSelectedHistoryRecordDetail(record);
      }
    } catch { /* ignore */ }
  }, [selectedTodoId, dispatch]);

  // ─── Load logs ─────────────────────────────────────────────

  const loadLogs = useCallback(async (recordId: number, page: number) => {
    setIsLoadingLogs(true);
    try {
      const result = await db.getExecutionLogs(recordId, page, logsPerPage);
      if (activeRecordIdRef.current !== recordId) return;
      setPaginatedLogs(result.logs);
      setLogsTotal(result.total);
      setLogsPage(result.page);
    } catch {
      if (activeRecordIdRef.current === recordId) setPaginatedLogs([]);
    } finally {
      if (activeRecordIdRef.current === recordId) setIsLoadingLogs(false);
    }
  }, [logsPerPage]);

  // ─── Auto-select first record in wide mode ────────────────

  useEffect(() => {
    setSelectedHistoryRecordId(null);
  }, [selectedTodoId]);

  // ─── When todo changes, load initial records & summary ──────

  const cancelledRef = useRef(false);
  useEffect(() => {
    cancelledRef.current = false;
    if (selectedTodoId) {
      setHistoryPage(1);
      const statusFilter = historyStatusFilter === 'all' ? undefined : historyStatusFilter;
      db.getExecutionRecords(selectedTodoId, 1, historyLimit, statusFilter).then(pageData => {
        if (cancelledRef.current) return;
        dispatch({ type: 'SET_EXECUTION_RECORDS', payload: { todoId: selectedTodoId, records: pageData.records } });
        setHistoryPage(pageData.page);
        setHistoryTotal(pageData.total);
      }).catch(() => {});

      db.getExecutionSummary(selectedTodoId).then(sum => {
        if (!cancelledRef.current) setSummary(sum);
      }).catch(() => {});
    } else {
      setSummary(null);
    }
    return () => { cancelledRef.current = true; };
  }, [selectedTodoId, historyLimit, historyStatusFilter, dispatch]);

  // ─── Load detail + logs when selected record changes ─────────

  useEffect(() => {
    activeRecordIdRef.current = selectedHistoryRecordId;
    if (!selectedHistoryRecordId) {
      setSelectedHistoryRecordDetail(null);
      setPaginatedLogs([]);
      setLogsTotal(0);
      setLogsPage(1);
      return;
    }

    const requestId = selectedHistoryRecordId;
    const basicRecord = storeRecords.find(r => r.id === requestId);

    setIsLoadingDetail(true);
    db.getExecutionRecord(requestId)
      .then(detail => {
        if (activeRecordIdRef.current !== requestId) return;
        setSelectedHistoryRecordDetail(detail);
        if (selectedTodoId) {
          dispatch({ type: 'UPDATE_EXECUTION_RECORD', payload: { todoId: selectedTodoId, record: detail } });
        }
      })
      .catch(() => {
        if (activeRecordIdRef.current !== requestId) return;
        if (basicRecord) setSelectedHistoryRecordDetail(basicRecord);
      })
      .finally(() => {
        if (activeRecordIdRef.current === requestId) setIsLoadingDetail(false);
      });

    loadLogs(requestId, 1);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedHistoryRecordId]);

  // ─── Pagination helper ─────────────────────────────────────

  const handleHistoryPageChange = useCallback((page: number, pageSize: number) => {
    if (pageSize !== historyLimit) {
      setHistoryLimit(pageSize);
      loadExecutionRecords(1, pageSize);
    } else {
      loadExecutionRecords(page, historyLimit);
    }
  }, [historyLimit, loadExecutionRecords]);

  // ─── Derived values ────────────────────────────────────────

  const records = selectedTodoId ? (storeRecords ?? []) : [];
  const selectedHistoryRecord = selectedHistoryRecordDetail
    || (selectedHistoryRecordId ? records.find(r => r.id === selectedHistoryRecordId) || null : null);

  return {
    selectedHistoryRecordId,
    setSelectedHistoryRecordId,
    records,
    historyPage,
    historyLimit,
    historyTotal,
    historyStatusFilter,
    setHistoryStatusFilter,
    isExecuting: false, // determined by caller using runningTasks
    summary,
    selectedHistoryRecord,
    isLoadingDetail,
    paginatedLogs,
    logsTotal,
    logsPage,
    logsPerPage,
    isLoadingLogs,
    loadExecutionRecords,
    loadLogs,
    refreshSingleRecord,
    handleHistoryPageChange,
  };
}
