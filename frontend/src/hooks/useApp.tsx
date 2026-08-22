/**
 * useApp — unified hook for backward compatibility.
 * Delegates to domain-specific contexts: TodoContext, ExecutionContext, UIContext.
 * New code should prefer: useTodos(), useExecution(), useUI() directly.
 */

import React, { useMemo, useCallback, useEffect } from 'react';
import * as db from '@/utils/database';

// ─── Direct imports (needed within this file) ─────────────────

import { useTodos, useTodosDispatch } from './useTodoContext';
import type { TodoAction } from './useTodoContext';
import { useExecution, useExecutionDispatch } from './useExecutionContext';
import type { ExecutionAction } from './useExecutionContext';
import { useUI, useUIDispatch } from './useUIContext';
import type { UIAction } from './useUIContext';
import { TodoProvider } from './useTodoContext';
import { ExecutionProvider } from './useExecutionContext';
import { UIProvider } from './useUIContext';
// 091：执行日志独立 context。dispatch/state 双 context，useApp 只取 dispatch 做路由，
// 不订阅 logs state，从而日志变化不会触发 46 个 useApp 消费方重渲染。
import { LogsProvider, useLogsDispatch } from './useLogsContext';
import type { LogsAction } from './useLogsContext';

export function AppProvider({ children }: { children: React.ReactNode }) {
  // 嵌套顺序的意图：UI（loading/theme）无依赖、被所有视图消费 → 最外层；
  // Todo/Execution 是平行的领域状态，相对顺序无依赖约束；LogsProvider 必须包住
  // ExecutionProvider 与 children——useApp 内的 useLogsDispatch（日志 action 路由）
  // 与 ExecutionPanel 的 useTaskLogs 都要从它解析，故夹在 UI 与 Execution 之间。
  // DataLoader 放最内层：它要用 todo/ui 的 dispatch 做启动初始化，须等所有 Provider 就位。
  return (
    <UIProvider>
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
// 这里只决定初始 workspace。

function DataLoader() {
  const { dispatch: todoDispatch } = useTodos();
  const { dispatch: uiDispatch } = useUI();
  const { state } = useTodos();

  useEffect(() => {
    async function loadData() {
      try {
        // 1. 先拉目录列表，用来决定初始 workspace
        const dirs = await db.getWorkspaces();
        // 持久化的 selectedWorkspace 若仍有效，优先用它；否则用第一个目录。
        const remembered = state.selectedWorkspace;
        const initialId =
          (remembered != null && dirs.some(d => d.id === remembered))
            ? remembered
            : (dirs[0]?.id ?? null);

        // 初始 workspace 同步进选择态
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

/// 093 批次2：只组合四域 dispatch、不订阅任何 state 的合并 dispatch。
///
/// 与 useApp() 的关键区别：useApp 内部调用 useTodos()/useExecution()/useUI()，
/// 即使只取 dispatch 也会订阅三域 state → 任一域变化调用方都重渲染。
/// 本 hook 全部走 dispatch-only context（引用恒定），订阅方零重渲染订阅——
/// WS 事件路由（useExecutionEvents）这类「只要 dispatch」的消费方专用。
export function useAppDispatch() {
  const todoDispatch = useTodosDispatch();
  const execDispatch = useExecutionDispatch();
  const uiDispatch = useUIDispatch();
  const logsDispatch = useLogsDispatch();

  // 与 useApp 的 dispatch 共用同一份路由逻辑（按 action.type 分派到对应域）
  return useCallback((action: TodoAction | ExecutionAction | UIAction | LogsAction) => {
    routeAppAction(action, { todoDispatch, execDispatch, uiDispatch, logsDispatch });
  }, [todoDispatch, execDispatch, uiDispatch, logsDispatch]);
}

/// useApp/useAppDispatch 共享的 action 路由表（单一事实源，防两份漂移）。
/// 与下方 useApp 的 dispatch 保持逐字对齐（当前 main 上 tags 已由组件本地状态接管，
/// todo 域只剩 SELECT_TODO/SELECT_WORKSPACE 两个 action）。
function routeAppAction(
  action: TodoAction | ExecutionAction | UIAction | LogsAction,
  dispatches: {
    todoDispatch: React.Dispatch<TodoAction>;
    execDispatch: React.Dispatch<ExecutionAction>;
    uiDispatch: React.Dispatch<UIAction>;
    logsDispatch: React.Dispatch<LogsAction>;
  },
) {
  const t = action.type;
  if (
    t === 'SELECT_TODO' ||
    t === 'SELECT_WORKSPACE'
  ) {
    dispatches.todoDispatch(action as TodoAction);
  } else if (
    t === 'SET_EXECUTION_RECORDS' || t === 'ADD_EXECUTION_RECORD' ||
    t === 'UPDATE_EXECUTION_RECORD' || t === 'ADD_RUNNING_TASK' ||
    t === 'FINISH_TASK' ||
    t === 'REMOVE_RUNNING_TASK' || t === 'CLEAR_RUNNING_TASKS' ||
    t === 'SET_ACTIVE_TASK' || t === 'UPDATE_TASK_TODO_PROGRESS' ||
    t === 'UPDATE_TASK_EXECUTION_STATS'
  ) {
    dispatches.execDispatch(action as ExecutionAction);
  } else if (
    t === 'SET_TASK_LOGS' || t === 'APPEND_TASK_LOGS' ||
    t === 'REMOVE_TASK_LOGS' || t === 'CLEAR_LOGS'
  ) {
    dispatches.logsDispatch(action as LogsAction);
  } else if (t === 'SET_LOADING') {
    dispatches.uiDispatch(action as UIAction);
  }
}

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
  // 093 批次2：路由逻辑收口到 routeAppAction（与 useAppDispatch 共用，防两份漂移）
  const dispatch = useCallback((action: TodoAction | ExecutionAction | UIAction | LogsAction) => {
    routeAppAction(action, { todoDispatch, execDispatch, uiDispatch, logsDispatch });
  }, [todoDispatch, execDispatch, uiDispatch, logsDispatch]);

  const clearSelection = useCallback(() => {
    // workspace 是一级筛选：不再自动清除，用户希望切换视图时保持选择
    todoDispatch({ type: 'SELECT_TODO', payload: null });
  }, [todoDispatch]);

  return useMemo(() => ({ state, dispatch, clearSelection }), [state, dispatch, clearSelection]);
}
