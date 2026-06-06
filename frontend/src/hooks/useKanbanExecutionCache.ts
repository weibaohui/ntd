/**
 * useKanbanExecutionCache — execution record caching for the Kanban board.
 *
 * Encapsulates:
 * - Eager cache prefetch for completed/failed todos (execRecordCache, totalRunsCache)
 * - Run-history switching with on-demand pagination (runDataCache, selectedRunIndex)
 * - Lazy result text fetching on card expansion (todoResults, loadingResults)
 *
 * All state is local; callers pass the full todo list and store records,
 * and receive derived cache state plus callbacks bound to the cache setters.
 */

import { useState, useEffect, useRef, useCallback } from 'react';
import type { Todo, ExecutionRecord } from '../types';
import * as db from '../utils/database';

interface UseKanbanExecutionCacheOptions {
  todos: Todo[];
  /** executionRecords from global store, keyed by todoId */
  storeRecords: Record<number, ExecutionRecord[]>;
}

interface UseKanbanExecutionCacheResult {
  // Cached "latest result" for each todo (used by collapsed cards)
  todoResults: Record<number, string>;
  loadingResults: Set<number>;

  // Per-todo run index selection
  selectedRunIndex: Record<number, number>;
  totalRunsCache: Record<number, number>;
  runDataCache: Record<number, (ExecutionRecord | null)[]>;
  loadingRunIndex: Record<number, number | null>;

  // Actions
  toggleResult: (todo: Todo) => Promise<void>;
  handleSelectRun: (todoId: number, runIndex: number) => Promise<void>;

  // Get the best available record for a todo (store > cache)
  getRecordForTodo: (todoId: number) => ExecutionRecord | null;
}

export function useKanbanExecutionCache({
  todos,
  storeRecords,
}: UseKanbanExecutionCacheOptions): UseKanbanExecutionCacheResult {
  // ─── Eager cache prefetch ────────────────────────────────
  // Cache of the "latest record" for each finished todo (used by collapsed cards)
  const [execRecordCache, setExecRecordCache] = useState<Record<number, ExecutionRecord>>({});
  // Tracks which todos we've already attempted to fetch (avoid duplicate requests)
  const fetchAttempted = useRef<Set<number>>(new Set());

  // ─── Run-history switching ───────────────────────────────
  const [selectedRunIndex, setSelectedRunIndex] = useState<Record<number, number>>({});
  const [totalRunsCache, setTotalRunsCache] = useState<Record<number, number>>({});
  const [runDataCache, setRunDataCache] = useState<Record<number, (ExecutionRecord | null)[]>>({});
  const [loadingRunIndex, setLoadingRunIndex] = useState<Record<number, number | null>>({});

  // ─── Lazy result text ───────────────────────────────────
  const [todoResults, setTodoResults] = useState<Record<number, string>>({});
  const [loadingResults, setLoadingResults] = useState<Set<number>>(new Set());

  // ─── Eagerly prefetch latest record for finished todos ───

  useEffect(() => {
    const finished = todos.filter(t => t.status === 'completed' || t.status === 'failed');
    for (const todo of finished) {
      if (fetchAttempted.current.has(todo.id)) continue;
      fetchAttempted.current.add(todo.id);

      // Prefer store data to avoid extra request
      const global = storeRecords[todo.id];
      if (global?.length) {
        setExecRecordCache(prev => {
          if (prev[todo.id]) return prev;
          return { ...prev, [todo.id]: global[0] };
        });
        setTotalRunsCache(prev => {
          if (prev[todo.id]) return prev;
          return { ...prev, [todo.id]: global.length };
        });
        continue;
      }

      // Lazy-fetch from API
      db.getExecutionRecords(todo.id, 1, 1).then(page => {
        if (page.records.length > 0) {
          setExecRecordCache(prev => {
            if (prev[todo.id]) return prev;
            return { ...prev, [todo.id]: page.records[0] };
          });
        }
        if (page.total > 0) {
          setTotalRunsCache(prev => {
            if (prev[todo.id]) return prev;
            return { ...prev, [todo.id]: page.total };
          });
        }
      }).catch(() => {});
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [todos]);

  // ─── Toggle result expansion & lazy-fetch ───────────────

  const toggleResult = useCallback(async (todo: Todo) => {
    const todoId = todo.id;

    if (!todoResults[todoId] && !storeRecords[todoId]?.length) {
      // Nothing cached yet and no store data — fetch
      if (loadingResults.has(todoId)) return;
      setLoadingResults(prev => { const n = new Set(prev); n.add(todoId); return n; });
      try {
        const page = await db.getExecutionRecords(todoId, 1, 1);
        if (page.records.length > 0 && page.records[0].result) {
          setTodoResults(prev => ({ ...prev, [todoId]: page.records[0].result! }));
        }
      } catch { /* ignore */ }
      finally {
        setLoadingResults(prev => { const n = new Set(prev); n.delete(todoId); return n; });
      }
    }
  }, [todoResults, loadingResults, storeRecords]);

  // ─── Run index selection ───────────────────────────────

  const handleSelectRun = useCallback(async (todoId: number, runIndex: number) => {
    if (selectedRunIndex[todoId] === runIndex) return;
    setSelectedRunIndex(prev => ({ ...prev, [todoId]: runIndex }));

    if (runDataCache[todoId]?.[runIndex]) return;

    if (runIndex === 0) {
      // Run 0 always maps to the cached latest record
      const record = execRecordCache[todoId] || storeRecords[todoId]?.[0];
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
      const page = await db.getExecutionRecords(todoId, runIndex + 1, 1);
      if (page.records.length > 0) {
        const record = page.records[0];
        setRunDataCache(prev => {
          const arr = prev[todoId] || [];
          const next = [...arr];
          next[runIndex] = record;
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
  }, [selectedRunIndex, runDataCache, execRecordCache, storeRecords, totalRunsCache]);

  // ─── Helper ─────────────────────────────────────────────

  const getRecordForTodo = useCallback((todoId: number): ExecutionRecord | null => {
    return storeRecords[todoId]?.[0] ?? execRecordCache[todoId] ?? null;
  }, [storeRecords, execRecordCache]);

  return {
    todoResults,
    loadingResults,
    selectedRunIndex,
    totalRunsCache,
    runDataCache,
    loadingRunIndex,
    toggleResult,
    handleSelectRun,
    getRecordForTodo,
  };
}
