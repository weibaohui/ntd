/**
 * useApp — unified hook for backward compatibility.
 * Delegates to domain-specific contexts: TodoContext, ExecutionContext, UIContext.
 * New code should prefer: useTodos(), useExecution(), useUI() directly.
 */

import React, { useMemo, useCallback, useEffect } from 'react';
import * as db from '@/utils/database';

// ─── Direct imports (needed within this file) ─────────────────

import { useTodos } from './useTodoContext';
import type { TodoAction } from './useTodoContext';
import { useExecution } from './useExecutionContext';
import type { ExecutionAction } from './useExecutionContext';
import { useUI } from './useUIContext';
import type { UIAction } from './useUIContext';
import { TodoProvider } from './useTodoContext';
import { ExecutionProvider } from './useExecutionContext';
import { UIProvider } from './useUIContext';
// 091：执行日志独立 context。dispatch/state 双 context，useApp 只取 dispatch 做路由，
// 不订阅 logs state，从而日志变化不会触发 46 个 useApp 消费方重渲染。
import { LogsProvider, useLogsDispatch } from './useLogsContext';
import type { LogsAction } from './useLogsContext';

export function AppProvider({ children }: { children: React.ReactNode }) {
  return (
    <UIProvider>
      {/* LogsProvider 置于最内层：包裹 ExecutionProvider 与 children，
          使其内部的 useLogsDispatch（本文件 useApp 路由）与 ExecutionPanel 的 useTaskLogs 都能解析。 */}
      <LogsProvider>
        <ExecutionProvider>
          <TodoProvider>
            <DataLoader />
            {children}
          </TodoProvider>
        </ExecutionProvider>
      </LogsProvider>
    </UIProvider>
  );
}

// ─── DataLoader (启动加载基础数据) ───────────
//
// 056（决策 2a）：全局 todos 桶已删除，启动不再预拉任何 todo——
// 列表页自己按页拉取，单条详情走 useTodoById。
// 这里只加载 tags（全量，基数小）与决定初始 workspace。

function DataLoader() {
  const { dispatch: todoDispatch } = useTodos();
  const { dispatch: uiDispatch } = useUI();
  const { state } = useTodos();

  useEffect(() => {
    async function loadData() {
      try {
        // 1. 先拉目录列表，用来决定初始 workspace
        const dirs = await db.getProjectDirectories();
        // 持久化的 selectedWorkspace 若仍有效，优先用它；否则用第一个目录。
        const remembered = state.selectedWorkspace;
        const initialId =
          (remembered != null && dirs.some(d => d.id === remembered))
            ? remembered
            : (dirs[0]?.id ?? null);

        // 2. 只拉 tags（全量，基数小）；初始 workspace 同步进选择态
        const tags = await db.getAllTags();
        todoDispatch({ type: 'SET_TAGS', payload: tags });
        if (initialId != null && initialId !== state.selectedWorkspace) {
          todoDispatch({ type: 'SELECT_WORKSPACE', payload: initialId });
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
  // 只取 dispatch：useReducer 的 dispatch 引用恒定，订阅它不会因 logs state 变化而重渲染。
  const logsDispatch = useLogsDispatch();

  // 056：state.todos 已删除——列表走服务端分页，单条走 useTodoById。
  // 091：刻意不把 logsByTask 合并进 state——日志只活在 LogsContext，
  // 避免 ExecutionPanel 以外的 useApp 消费方因日志变化重渲染。
  const state = useMemo(() => ({
    ...todoState,
    ...execState,
    ...uiState,
  }), [todoState, execState, uiState]);

  // Combined dispatch routes actions to the appropriate sub-dispatcher
  // based on the action's `type` discriminator field.
  // 使用 TodoAction | ExecutionAction | UIAction | LogsAction 联合类型替代 unknown，保留类型安全。
  const dispatch = useCallback((action: TodoAction | ExecutionAction | UIAction | LogsAction) => {
    const t = action.type;
    if (
      t === 'SET_TAGS' || t === 'SELECT_TODO' ||
      t === 'SELECT_TAG' || t === 'SELECT_WORKSPACE' ||
      t === 'ADD_TAG' || t === 'DELETE_TAG'
    ) {
      todoDispatch(action);
    } else if (
      t === 'SET_EXECUTION_RECORDS' || t === 'ADD_EXECUTION_RECORD' ||
      t === 'UPDATE_EXECUTION_RECORD' || t === 'ADD_RUNNING_TASK' ||
      t === 'FINISH_TASK' ||
      t === 'REMOVE_RUNNING_TASK' || t === 'CLEAR_RUNNING_TASKS' ||
      t === 'SET_ACTIVE_TASK' || t === 'UPDATE_TASK_TODO_PROGRESS' ||
      t === 'UPDATE_TASK_EXECUTION_STATS'
    ) {
      execDispatch(action);
    } else if (
      t === 'SET_TASK_LOGS' || t === 'APPEND_TASK_LOGS' ||
      t === 'REMOVE_TASK_LOGS' || t === 'CLEAR_LOGS'
    ) {
      logsDispatch(action);
    } else if (t === 'SET_LOADING') {
      uiDispatch(action);
    }
  }, [todoDispatch, execDispatch, uiDispatch, logsDispatch]);

  const clearSelection = useCallback(() => {
    // workspace 是一级筛选：不再自动清除，用户希望切换视图时保持选择
    todoDispatch({ type: 'SELECT_TODO', payload: null });
    todoDispatch({ type: 'SELECT_TAG', payload: null });
  }, [todoDispatch]);

  return useMemo(() => ({ state, dispatch, clearSelection }), [state, dispatch, clearSelection]);
}
