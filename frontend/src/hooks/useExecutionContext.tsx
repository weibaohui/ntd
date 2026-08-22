import React, { createContext, useContext, useReducer, useMemo, ReactNode } from 'react';
import type { ExecutionRecord, RunningTask, TodoItem, ExecutionStats } from '@/types';

// ─── State & Reducer ─────────────────────────────────────────

interface ExecutionState {
  executionRecords: Record<number, ExecutionRecord[]>;
  runningTasks: Record<string, RunningTask>;
  activeTaskId: string | null;
}

type ExecutionAction =
  | { type: 'SET_EXECUTION_RECORDS'; payload: { todoId: number; records: ExecutionRecord[] } }
  | { type: 'ADD_EXECUTION_RECORD'; payload: { todoId: number; record: ExecutionRecord } }
  | { type: 'UPDATE_EXECUTION_RECORD'; payload: { todoId: number; record: ExecutionRecord } }
  | { type: 'ADD_RUNNING_TASK'; payload: RunningTask }
  | { type: 'FINISH_TASK'; payload: { taskId: string; todoId: number; success: boolean; result: string | null } }
  | { type: 'REMOVE_RUNNING_TASK'; payload: string }
  | { type: 'CLEAR_RUNNING_TASKS' }
  | { type: 'SET_ACTIVE_TASK'; payload: string | null }
  | { type: 'UPDATE_TASK_TODO_PROGRESS'; payload: { taskId: string; progress: TodoItem[] } }
  | { type: 'UPDATE_TASK_EXECUTION_STATS'; payload: { taskId: string; stats: ExecutionStats } };

const initialState: ExecutionState = {
  executionRecords: {},
  runningTasks: {},
  activeTaskId: null,
};

function reducer(state: ExecutionState, action: ExecutionAction): ExecutionState {
  switch (action.type) {
    case 'SET_EXECUTION_RECORDS':
      return { ...state, executionRecords: { ...state.executionRecords, [action.payload.todoId]: action.payload.records } };
    case 'ADD_EXECUTION_RECORD':
      return {
        ...state,
        executionRecords: {
          ...state.executionRecords,
          [action.payload.todoId]: [action.payload.record, ...(state.executionRecords[action.payload.todoId] || [])],
        },
      };
    case 'UPDATE_EXECUTION_RECORD':
      return {
        ...state,
        executionRecords: {
          ...state.executionRecords,
          [action.payload.todoId]: (state.executionRecords[action.payload.todoId] || []).map(
            r => r.id === action.payload.record.id ? action.payload.record : r
          ),
        },
      };
    case 'ADD_RUNNING_TASK': {
      const task = action.payload;
      return { ...state, runningTasks: { ...state.runningTasks, [task.taskId]: task }, activeTaskId: state.activeTaskId || task.taskId };
    }
    // 091：APPEND_TASK_LOG 已移除——日志改由 LogsContext（useLogsContext）的
    // APPEND_TASK_LOGS 处理，不再进入本执行态 reducer。
    case 'FINISH_TASK': {
      const { taskId, success, result } = action.payload;
      const task = state.runningTasks[taskId];
      if (!task) return state;
      const now = new Date().toISOString();
      return { ...state, runningTasks: { ...state.runningTasks, [taskId]: { ...task, status: 'finished' as const, success, result, finishedAt: now } } };
    }
    case 'REMOVE_RUNNING_TASK': {
      const taskId = action.payload;
      const { [taskId]: _, ...rest } = state.runningTasks;
      const remainingIds = Object.keys(rest);
      return { ...state, runningTasks: rest, activeTaskId: state.activeTaskId === taskId ? (remainingIds[0] || null) : state.activeTaskId };
    }
    case 'CLEAR_RUNNING_TASKS':
      return { ...state, runningTasks: {}, activeTaskId: null };
    case 'SET_ACTIVE_TASK':
      return { ...state, activeTaskId: action.payload };
    case 'UPDATE_TASK_TODO_PROGRESS': {
      const task = state.runningTasks[action.payload.taskId];
      if (!task) return state;
      return { ...state, runningTasks: { ...state.runningTasks, [action.payload.taskId]: { ...task, todoProgress: action.payload.progress } } };
    }
    case 'UPDATE_TASK_EXECUTION_STATS': {
      const task = state.runningTasks[action.payload.taskId];
      if (!task) return state;
      return { ...state, runningTasks: { ...state.runningTasks, [action.payload.taskId]: { ...task, executionStats: action.payload.stats } } };
    }
    default: return state;
  }
}

// ─── Context ──────────────────────────────────────────────────

const ExecutionContext = createContext<{ state: ExecutionState; dispatch: React.Dispatch<ExecutionAction> } | null>(null);
// 093 批次2：dispatch 独立 context——执行态高频变化（进度/统计推送），
// 只订阅 dispatch 的消费方不再被卷入重渲染。
const ExecutionDispatchContext = createContext<React.Dispatch<ExecutionAction> | null>(null);

// ─── Provider ─────────────────────────────────────────────────

export function ExecutionProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, initialState);
  const ctx = useMemo(() => ({ state, dispatch }), [state, dispatch]);
  return (
    <ExecutionDispatchContext.Provider value={dispatch}>
      <ExecutionContext.Provider value={ctx}>{children}</ExecutionContext.Provider>
    </ExecutionDispatchContext.Provider>
  );
}

// ─── Hook ─────────────────────────────────────────────────────

export function useExecution() {
  const ctx = useContext(ExecutionContext);
  if (!ctx) throw new Error('useExecution must be used within ExecutionProvider');
  return ctx;
}

/** 只取 dispatch（引用恒定）：订阅它不会因执行态变化重渲染。 */
export function useExecutionDispatch() {
  const dispatch = useContext(ExecutionDispatchContext);
  if (!dispatch) throw new Error('useExecutionDispatch must be used within ExecutionProvider');
  return dispatch;
}

export type { ExecutionAction };
