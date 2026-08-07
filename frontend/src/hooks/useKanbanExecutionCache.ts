/**
 * useKanbanExecutionCache — 看板视图执行记录数据源。
 *
 * 不再做本地缓存（之前因缓存导致切 workspace 后数据不刷新），
 * 所有数据直接走 API 或 storeRecords（全局 store 里的执行记录）。
 */

import { useState, useCallback } from 'react';
import type { ExecutionRecord } from '@/types';
import * as db from '@/utils/database';

interface UseKanbanExecutionCacheOptions {
  // 056：只需要 id 做执行记录查询，类型放宽以兼容 TodoBrief
  todos: Array<{ id: number }>;
  /** executionRecords from global store, keyed by todoId */
  storeRecords: Record<number, ExecutionRecord[]>;
  /** 当前选中的 workspace_id，透传到 API */
  workspaceId?: number | null;
}

interface UseKanbanExecutionCacheResult {
  // Per-todo run index selection
  selectedRunIndex: Record<number, number>;
  totalRunsCache: Record<number, number>;
  runDataCache: Record<number, (ExecutionRecord | null)[]>;
  loadingRunIndex: Record<number, number | null>;

  // Actions
  toggleResult: (todo: { id: number }) => Promise<string | null>;
  handleSelectRun: (todoId: number, runIndex: number) => Promise<void>;

  // Get the best available record for a todo (store > API)
  getRecordForTodo: (todoId: number) => ExecutionRecord | null;
}

export function useKanbanExecutionCache({
  storeRecords,
  workspaceId,
}: UseKanbanExecutionCacheOptions): UseKanbanExecutionCacheResult {
  const [selectedRunIndex, setSelectedRunIndex] = useState<Record<number, number>>({});
  const [totalRunsCache, setTotalRunsCache] = useState<Record<number, number>>({});
  const [runDataCache, setRunDataCache] = useState<Record<number, (ExecutionRecord | null)[]>>({});
  const [loadingRunIndex, setLoadingRunIndex] = useState<Record<number, number | null>>({});

  // 点击展开时从 API 拉取最新执行结果（不做本地缓存）
  const toggleResult = useCallback(async (todo: { id: number }): Promise<string | null> => {
    // 优先走 store 数据，不需要额外请求
    const storeRecord = storeRecords[todo.id]?.[0];
    if (storeRecord?.result) return storeRecord.result;

    try {
      // 091 修复参数错位：getExecutionRecords(workspaceId, todoId, page, limit, status, stepId)。
      // 旧代码把 todo.id 当 workspaceId、把 workspaceId 当 stepId，请求打到错误 workspace 且无结果。
      const page = await db.getExecutionRecords(workspaceId ?? 0, todo.id, 1, 1, undefined, undefined);
      return page.records[0]?.result ?? null;
    } catch {
      return null;
    }
  }, [storeRecords, workspaceId]);

  // 选择执行轮次时从 API 拉取
  const handleSelectRun = useCallback(async (todoId: number, runIndex: number) => {
    if (selectedRunIndex[todoId] === runIndex) return;
    setSelectedRunIndex(prev => ({ ...prev, [todoId]: runIndex }));

    if (runDataCache[todoId]?.[runIndex]) return;

    // runIndex=0 用 store 数据或 API
    if (runIndex === 0) {
      const record = storeRecords[todoId]?.[0];
      if (record) {
        setRunDataCache(prev => {
          const arr = prev[todoId] || [];
          const next = [...arr];
          next[0] = record;
          return { ...prev, [todoId]: next };
        });
      }
      return;
    }

    setLoadingRunIndex(prev => ({ ...prev, [todoId]: runIndex }));
    try {
      // 091 修复参数错位：第一参是 workspaceId（旧代码误传 todoId），末参 stepId 不再误传 workspaceId。
      const page = await db.getExecutionRecords(workspaceId ?? 0, todoId, runIndex + 1, 1, undefined, undefined);
      if (page.records.length > 0) {
        setRunDataCache(prev => {
          const arr = prev[todoId] || [];
          const next = [...arr];
          next[runIndex] = page.records[0];
          return { ...prev, [todoId]: next };
        });
        if (!totalRunsCache[todoId] && page.total > 0) {
          setTotalRunsCache(prev => ({ ...prev, [todoId]: page.total }));
        }
      }
    } catch { /* ignore */ }
    finally {
      setLoadingRunIndex(prev => ({ ...prev, [todoId]: null }));
    }
  }, [selectedRunIndex, runDataCache, storeRecords, totalRunsCache, workspaceId]);

  const getRecordForTodo = useCallback((todoId: number): ExecutionRecord | null => {
    return storeRecords[todoId]?.[0] ?? null;
  }, [storeRecords]);

  return {
    selectedRunIndex,
    totalRunsCache,
    runDataCache,
    loadingRunIndex,
    toggleResult,
    handleSelectRun,
    getRecordForTodo,
  };
}