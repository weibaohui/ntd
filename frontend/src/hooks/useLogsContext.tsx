// 091 性能优化：执行日志独立 context。
//
// 历史问题：日志原本存在 runningTasks[taskId].logs（Execution state），
// 每条 WS Output 日志都 append → 整个 Execution state 引用变化 →
// 所有 useApp()/useExecution() 消费方（约 46 个组件）重渲染。
//
// 拆分后：日志只活在本 context，只有真正展示日志的 ExecutionPanel 订阅，
// 执行态消费方不再因日志重渲染。批量 APPEND_TASK_LOGS + 每任务 500 条上限，
// 进一步降低 reducer 往返与长跑任务的内存占用。
//
// 双 context：dispatch 与 state 分离。dispatch 引用恒定，useApp 只取 dispatch
// 做路由，不订阅 state，从而不会因日志变化重渲染。

import React, { createContext, useContext, useReducer, type ReactNode } from 'react';
import type { LogEntry } from '@/types';

/** 每任务内存日志上限：超过只保留最近 N 条，防长跑任务数组无限膨胀。 */
const LOG_CAP = 500;

interface LogsState {
  logsByTask: Record<string, LogEntry[]>;
  // 091：每任务日志「全量条数」（Sync 时由后端 log_total 给出，WS Output 追加时同步累加）。
  // 用于面板提示「共 N 条」——logs 因 RECONNECT_LOG_CAP/cap() 只保留最近一段，total 告知还有更多历史。
  totalByTask: Record<string, number>;
}

type LogsAction =
  // 重连 Sync：整体替换某任务的日志（用后端回传的最近 N 条）；total 为后端全量条数。
  | { type: 'SET_TASK_LOGS'; payload: { taskId: string; logs: LogEntry[]; total?: number } }
  // 批量追加：WS Output 50ms 缓冲合并后一次写入。
  | { type: 'APPEND_TASK_LOGS'; payload: { taskId: string; logs: LogEntry[] } }
  // 任务从 runningTasks 移除时同步清掉其日志，释放内存。
  | { type: 'REMOVE_TASK_LOGS'; payload: string }
  // 重连 Sync 开头：清空全部任务日志（与 CLEAR_RUNNING_TASKS 配对）。
  | { type: 'CLEAR_LOGS' };

const initialState: LogsState = { logsByTask: {}, totalByTask: {} };

/** 截断到最近 LOG_CAP 条：超限只留尾部，保证「最近」语义而非「最早」。 */
function cap(logs: LogEntry[]): LogEntry[] {
  return logs.length > LOG_CAP ? logs.slice(logs.length - LOG_CAP) : logs;
}

function reducer(state: LogsState, action: LogsAction): LogsState {
  switch (action.type) {
    case 'SET_TASK_LOGS':
      return {
        logsByTask: { ...state.logsByTask, [action.payload.taskId]: cap(action.payload.logs) },
        // total 缺省时用 logs.length 兜底（非 Sync 路径调用方未传 total 的安全退化）。
        totalByTask: {
          ...state.totalByTask,
          [action.payload.taskId]: action.payload.total ?? action.payload.logs.length,
        },
      };
    case 'APPEND_TASK_LOGS': {
      const { taskId, logs } = action.payload;
      // 空批次直接返回原 state：reducer 必须返回同引用，避免无变化也触发消费方重渲染。
      if (logs.length === 0) return state;
      const prev = state.logsByTask[taskId] || [];
      const prevTotal = state.totalByTask[taskId] ?? 0;
      return {
        logsByTask: { ...state.logsByTask, [taskId]: cap(prev.concat(logs)) },
        // 实时追加的日志也计入全量条数，保持 total 与 logs 同步增长。
        totalByTask: { ...state.totalByTask, [taskId]: prevTotal + logs.length },
      };
    }
    case 'REMOVE_TASK_LOGS': {
      const { [action.payload]: _removed, ...restLogs } = state.logsByTask;
      const { [action.payload]: _removedTotal, ...restTotals } = state.totalByTask;
      return { logsByTask: restLogs, totalByTask: restTotals };
    }
    case 'CLEAR_LOGS':
      return { logsByTask: {}, totalByTask: {} };
    default:
      return state;
  }
}

// 双 context：dispatch 恒定（useReducer 保证），state 随日志变化。
const LogsDispatchContext = createContext<React.Dispatch<LogsAction> | null>(null);
const LogsStateContext = createContext<LogsState | null>(null);

export function LogsProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, initialState);
  // state 变化只让 LogsStateContext 消费者重渲染；dispatch 恒定，订阅它的 useApp 不受影响。
  return (
    <LogsDispatchContext.Provider value={dispatch}>
      <LogsStateContext.Provider value={state}>
        {children}
      </LogsStateContext.Provider>
    </LogsDispatchContext.Provider>
  );
}

/** 取 dispatch（引用恒定）：供 useApp 路由日志 action，不订阅 state。 */
export function useLogsDispatch(): React.Dispatch<LogsAction> {
  const d = useContext(LogsDispatchContext);
  if (!d) throw new Error('useLogsDispatch must be used within LogsProvider');
  return d;
}

/** 复用的空数组常量：无日志时返回同一引用，配合 React.memo 避免无谓重渲染。 */
const EMPTY_LOGS: LogEntry[] = [];

/**
 * 选择器：取单个任务的日志（taskId 为空或无日志返回 EMPTY_LOGS 稳定引用）。
 * 订阅 LogsStateContext：任意任务日志变化都会让调用者重渲染，但配合 React.memo 的 LogList，
 * 只有日志引用真变的任务才会触发实际 DOM diff。
 */
export function useTaskLogs(taskId: string | null | undefined): LogEntry[] {
  const state = useContext(LogsStateContext);
  if (!state) throw new Error('useTaskLogs must be used within LogsProvider');
  if (!taskId) return EMPTY_LOGS;
  return state.logsByTask[taskId] || EMPTY_LOGS;
}

/**
 * 选择器：取单个任务的日志全量条数（Sync 给的历史快照 + 实时追加累加）。
 * 无该任务记录时返回 undefined（调用方据此决定是否显示「共 N 条」提示）。
 */
export function useTaskLogTotal(taskId: string | null | undefined): number | undefined {
  const state = useContext(LogsStateContext);
  if (!state) throw new Error('useTaskLogTotal must be used within LogsProvider');
  if (!taskId) return undefined;
  return state.totalByTask[taskId];
}

export type { LogsAction };
