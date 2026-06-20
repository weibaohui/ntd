/**
 * useApp — unified hook for backward compatibility.
 * Delegates to domain-specific contexts: TodoContext, ExecutionContext, UIContext.
 * New code should prefer: useTodos(), useExecution(), useUI() directly.
 */

import React, { useMemo, useCallback, useEffect } from 'react';
import * as db from '@/utils/database';

// Re-export domain hooks and providers (they are defined in separate files)
export { useTodos, TodoProvider } from './useTodoContext';
export type { TodoAction } from './useTodoContext';
export { useExecution, ExecutionProvider } from './useExecutionContext';
export type { ExecutionAction } from './useExecutionContext';
export { useUI, UIProvider } from './useUIContext';

// ─── Direct imports (needed within this file) ─────────────────

import { useTodos } from './useTodoContext';
import { useExecution } from './useExecutionContext';
import { useUI } from './useUIContext';
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

// ─── DataLoader (loads initial todos/tags on mount) ───────────

function DataLoader() {
  const { dispatch: todoDispatch } = useTodos();
  const { dispatch: uiDispatch } = useUI();

  useEffect(() => {
    async function loadData() {
      try {
        const [todos, tags] = await Promise.all([db.getAllTodos(), db.getAllTags()]);
        todoDispatch({ type: 'SET_TODOS', payload: todos });
        todoDispatch({ type: 'SET_TAGS', payload: tags });
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

export type AppState = ReturnType<typeof useApp>['state'];

export function useApp() {
  const { state: todoState, dispatch: todoDispatch } = useTodos();
  const { state: execState, dispatch: execDispatch } = useExecution();
  const { state: uiState, dispatch: uiDispatch } = useUI();

  // Merge all sub-states into a flat object
  const state = useMemo(() => ({
    ...todoState,
    ...execState,
    ...uiState,
  }), [todoState, execState, uiState]);

  // Combined dispatch routes actions to the appropriate sub-dispatcher
  // based on the action's `type` discriminator field.
  const dispatch = useCallback((action: unknown) => {
    const t = (action as { type: string }).type;
    if (
      t === 'SET_TODOS' || t === 'SET_TAGS' || t === 'ADD_TODO' ||
      t === 'UPDATE_TODO' || t === 'DELETE_TODO' || t === 'SELECT_TODO' ||
      t === 'SELECT_TAG' || t === 'SELECT_WORKSPACE' || t === 'ADD_TAG' || t === 'DELETE_TAG' ||
      t === 'UPDATE_TODO_STATUS'
    ) {
      todoDispatch(action as Parameters<typeof todoDispatch>[0]);
    } else if (
      t === 'SET_EXECUTION_RECORDS' || t === 'ADD_EXECUTION_RECORD' ||
      t === 'UPDATE_EXECUTION_RECORD' || t === 'ADD_RUNNING_TASK' ||
      t === 'APPEND_TASK_LOG' || t === 'FINISH_TASK' ||
      t === 'REMOVE_RUNNING_TASK' || t === 'CLEAR_RUNNING_TASKS' ||
      t === 'SET_ACTIVE_TASK' || t === 'UPDATE_TASK_TODO_PROGRESS' ||
      t === 'UPDATE_TASK_EXECUTION_STATS'
    ) {
      execDispatch(action as Parameters<typeof execDispatch>[0]);
    } else if (t === 'SET_LOADING') {
      uiDispatch(action as Parameters<typeof uiDispatch>[0]);
    }
  }, [todoDispatch, execDispatch, uiDispatch]);

  const clearSelection = useCallback(() => {
    // workspace 是一级筛选：不再自动清除，用户希望切换视图时保持选择
    todoDispatch({ type: 'SELECT_TODO', payload: null });
    todoDispatch({ type: 'SELECT_TAG', payload: null });
  }, [todoDispatch]);

  return useMemo(() => ({ state, dispatch, clearSelection }), [state, dispatch, clearSelection]);
}
