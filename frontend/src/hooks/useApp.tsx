/**
 * useApp — unified hook for backward compatibility.
 * Delegates to domain-specific contexts: TodoContext, ExecutionContext, UIContext.
 * New code should prefer: useTodos(), useExecution(), useUI() directly.
 */

import React, { useMemo, useCallback, useEffect } from 'react';
import * as db from '@/utils/database';
import type { Todo } from '@/types';

// ─── Direct imports (needed within this file) ─────────────────

import { useTodos, useVisibleTodos } from './useTodoContext';
import type { TodoAction } from './useTodoContext';
import { useExecution } from './useExecutionContext';
import type { ExecutionAction } from './useExecutionContext';
import { useUI } from './useUIContext';
import type { UIAction } from './useUIContext';
import { TodoProvider } from './useTodoContext';
import { ExecutionProvider } from './useExecutionContext';
import { UIProvider } from './useUIContext';

export function AppProvider({ children }: { children: React.ReactNode }) {
  return (
    <UIProvider>
      <ExecutionProvider>
        <TodoProvider>
          <DataLoader />
          {children}
        </TodoProvider>
      </ExecutionProvider>
    </UIProvider>
  );
}

// ─── DataLoader (按需加载第一个 workspace 的 todos) ───────────
//
// 性能优化（perf/todo-by-workspace）：
// - 不再启动时 getAllTodos() 拉全量；改为先拉 project_directories 选第一个
//   workspace（按持久化的 selectedWorkspace > 第一个），然后只拉那一个桶。
// - 多 workspace 用户切到新 workspace 时由 TodoList 等组件触发按需拉，本组件
//   不主动拉取。
// - tags 仍是全量加载（基数小，按 id 查找频率高，缓存整集合值得）。

function DataLoader() {
  const { dispatch: todoDispatch } = useTodos();
  const { dispatch: uiDispatch } = useUI();
  const { state } = useTodos();

  useEffect(() => {
    async function loadData() {
      try {
        // 1. 先拉目录列表，用来决定第一个 workspace
        const dirs = await db.getProjectDirectories();
        // 持久化的 selectedWorkspace 若仍有效，优先用它；否则用第一个目录。
        const remembered = state.selectedWorkspace;
        const initialId =
          (remembered != null && dirs.some(d => d.id === remembered))
            ? remembered
            : (dirs[0]?.id ?? null);

        // 2. 并行加载：tags（全量）+ 第一个 workspace 的 todos（按 workspace_id）
        const [tags, initialTodos] = await Promise.all([
          db.getAllTags(),
          initialId != null ? db.getAllTodos(initialId) : Promise.resolve([] as Todo[]),
        ]);
        todoDispatch({ type: 'SET_TAGS', payload: tags });
        if (initialId != null) {
          todoDispatch({ type: 'SET_TODOS_BY_WORKSPACE', workspaceId: initialId, payload: initialTodos });
        }
      } catch {
        // Non-fatal: app will show empty state
      } finally {
        uiDispatch({ type: 'SET_LOADING', payload: false });
      }
    }
    loadData();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return null;
}

// ─── Unified useApp hook (backward compatibility) ─────────────

export function useApp() {
  const { state: todoState, dispatch: todoDispatch } = useTodos();
  const { state: execState, dispatch: execDispatch } = useExecution();
  const { state: uiState, dispatch: uiDispatch } = useUI();
  // visibleTodos：按 selectedWorkspace 返回当前桶；null 时聚合所有桶。
  // 把派生字段挂在 state 上，让 ~30 个老调用方读 state.todos 不用改。
  const visibleTodos = useVisibleTodos();

  // Merge all sub-states into a flat object. todosByWorkspace 故意不展开成 todos：
  // 派生字段 `todos` 已经按 selectedWorkspace 过滤好，老调用方继续读 state.todos 即可。
  const state = useMemo(() => ({
    ...todoState,
    todos: visibleTodos,
    ...execState,
    ...uiState,
  }), [todoState, visibleTodos, execState, uiState]);

  // Combined dispatch routes actions to the appropriate sub-dispatcher
  // based on the action's `type` discriminator field.
  // 使用 TodoAction | ExecutionAction | UIAction 联合类型替代 unknown，保留类型安全。
  const dispatch = useCallback((action: TodoAction | ExecutionAction | UIAction) => {
    const t = action.type;
    if (
      t === 'SET_TODOS_BY_WORKSPACE' || t === 'SET_TAGS' || t === 'ADD_TODO' ||
      t === 'UPDATE_TODO' || t === 'DELETE_TODO' || t === 'SELECT_TODO' ||
      t === 'SELECT_TAG' || t === 'SELECT_WORKSPACE' ||
      t === 'ADD_TAG' || t === 'DELETE_TAG' ||
      t === 'UPDATE_TODO_STATUS'
    ) {
      todoDispatch(action);
    } else if (
      t === 'SET_EXECUTION_RECORDS' || t === 'ADD_EXECUTION_RECORD' ||
      t === 'UPDATE_EXECUTION_RECORD' || t === 'ADD_RUNNING_TASK' ||
      t === 'APPEND_TASK_LOG' || t === 'FINISH_TASK' ||
      t === 'REMOVE_RUNNING_TASK' || t === 'CLEAR_RUNNING_TASKS' ||
      t === 'SET_ACTIVE_TASK' || t === 'UPDATE_TASK_TODO_PROGRESS' ||
      t === 'UPDATE_TASK_EXECUTION_STATS'
    ) {
      execDispatch(action);
    } else if (t === 'SET_LOADING') {
      uiDispatch(action);
    }
  }, [todoDispatch, execDispatch, uiDispatch]);

  const clearSelection = useCallback(() => {
    // workspace 是一级筛选：不再自动清除，用户希望切换视图时保持选择
    todoDispatch({ type: 'SELECT_TODO', payload: null });
    todoDispatch({ type: 'SELECT_TAG', payload: null });
  }, [todoDispatch]);

  return useMemo(() => ({ state, dispatch, clearSelection }), [state, dispatch, clearSelection]);
}
