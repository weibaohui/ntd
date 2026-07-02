import { useEffect, useRef } from 'react';
import { useApp } from './useApp';
import type { LogEntry, TodoItem, ExecutionStats } from '@/types';

// ─── 类型定义 ───────────────────────────────────────────────────

interface ExecEventStarted {
  type: 'Started';
  task_id: string;
  todo_id: number;
  todo_title: string;
  executor: string;
  workspace_id: number | null;
}

interface ExecEventOutput {
  type: 'Output';
  task_id: string;
  entry: LogEntry;
  workspace_id: number | null;
}

interface ExecEventFinished {
  type: 'Finished';
  task_id: string;
  todo_id: number;
  success: boolean;
  result: string | null;
  duration_secs: number;
  total_tokens: number;
  workspace_id: number | null;
}

interface ExecEventSync {
  type: 'Sync';
  tasks: Array<{
    task_id: string;
    todo_id: number;
    todo_title: string;
    executor: string;
    logs: string;
  }>;
}

interface ExecEventTodoProgress {
  type: 'TodoProgress';
  task_id: string;
  progress: TodoItem[];
  workspace_id: number | null;
}

interface ExecEventExecutionStats {
  type: 'ExecutionStats';
  task_id: string;
  stats: ExecutionStats;
  workspace_id: number | null;
}

interface ExecEventReviewStatusChanged {
  type: 'ReviewStatusChanged';
  record_id: number;
  todo_id: number;
  review_status: string;
}

interface ExecEventLoopFinished {
  type: 'LoopFinished';
  loop_execution_id: number;
  loop_id: number;
  loop_title: string;
  status: string;
  total_steps: number;
  completed_steps: number;
  failed_steps: number;
  duration_secs: number;
  total_tokens: number;
  workspace_id: number | null;
}

type ExecEvent = ExecEventStarted | ExecEventOutput | ExecEventFinished | ExecEventSync | ExecEventTodoProgress | ExecEventExecutionStats | ExecEventReviewStatusChanged | ExecEventLoopFinished;

// ─── 模块级共享状态 ─────────────────────────────────────────────
//
// 为什么用模块级单例而不是 React state/ref：
// useExecutionEvents 可能在多个组件中被调用（App.tsx + LoopStudioExecutionsPanel），
// 如果每个调用方都创建独立的 WebSocket，事件会被重复处理 → 执行日志翻倍。
// 模块级变量在所有组件实例间共享，确保全局只有一个 WebSocket 连接。
// （见 issue #720 分析：https://github.com/weibaohui/nothing-todo/issues/720）

/** 全局唯一 WebSocket 连接实例 */
let sharedWs: WebSocket | null = null;
/** 断线重连定时器 */
let sharedReconnectTimer: ReturnType<typeof setTimeout> | null = null;
/** 重连尝试次数（指数退避） */
let sharedReconnectAttempt = 0;
/** 是否允许重连（true=允许，false=所有调用方已卸载） */
let sharedShouldReconnect = true;
/** 由各调用方注册的 onRefresh 回调 ref 数组，收到事件时全部触发。
 *  注意：存的是 ref 对象而非函数，因为 effect 只执行一次，但 onRefresh 函数的引用
 *  可能因 useCallback 依赖变化而改变。存 ref 对象后，触发时读 ref.current 总能拿到最新值。 */
let sharedOnRefreshRefs: Array<React.MutableRefObject<(() => void) | undefined>> = [];
/** 自动清除已结束任务的定时器集合 */
let sharedRemoveTaskTimers = new Set<ReturnType<typeof setTimeout>>();
/** 全局 dispatch 函数（从第一个调用方的 useApp() 获取，后续复用） */
let sharedDispatch: ReturnType<typeof useApp>['dispatch'] | null = null;
/** 当前活跃的调用方数量（当计数归零时关闭 WS） */
let sharedInstanceCount = 0;

/**
 * 指数退避 + 随机抖动：min(2^n * 1000, 30000) + random(0, 1000)
 * 避免多个客户端同时重连造成 thundering herd。
 */
function getReconnectDelay(): number {
  const base = Math.min(Math.pow(2, sharedReconnectAttempt) * 1000, 30000);
  const jitter = Math.floor(Math.random() * 1000);
  return base + jitter;
}

/** 创建全局 WebSocket 连接（如果尚未创建） */
function connectShared(dispatch: ReturnType<typeof useApp>['dispatch']) {
  if (!sharedShouldReconnect) return;
  // 已有连接则跳过（防止 React StrictMode 开发期双调用创建两个 WS）
  if (sharedWs) return;

  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const ws = new WebSocket(`${protocol}//${window.location.host}/api/events`);
  sharedWs = ws;

  ws.onopen = () => {
    sharedReconnectAttempt = 0;
  };

  ws.onmessage = (event) => {
    // 后端握手阶段的 "Connected" 文本不是 JSON，跳过解析
    if (event.data === 'Connected') return;
    try {
      const data: ExecEvent = JSON.parse(event.data);

      // 触发所有调用方注册的 onRefresh 回调（如 loop 面板刷新）。
      // 通过 ref 数组间接调用，确保总是拿到最新的回调函数引用。
      sharedOnRefreshRefs.forEach(ref => ref.current?.());

      switch (data.type) {
        case 'Sync': {
          dispatch({ type: 'CLEAR_RUNNING_TASKS' });
          data.tasks.forEach(task => {
            let parsedLogs: LogEntry[] = [];
            try { parsedLogs = JSON.parse(task.logs || '[]'); } catch {}
            dispatch({ type: 'ADD_RUNNING_TASK', payload: { taskId: task.task_id, todoId: task.todo_id, todoTitle: task.todo_title, executor: task.executor || 'claudecode', logs: parsedLogs, status: 'running', startedAt: new Date().toISOString() } });
            dispatch({ type: 'UPDATE_TODO_STATUS', payload: { id: task.todo_id, status: 'running' } });
          });
          break;
        }
        case 'Started': {
          dispatch({ type: 'ADD_RUNNING_TASK', payload: { taskId: data.task_id, todoId: data.todo_id, todoTitle: data.todo_title, executor: data.executor || 'claudecode', logs: [], status: 'running', startedAt: new Date().toISOString() } });
          dispatch({ type: 'UPDATE_TODO_STATUS', payload: { id: data.todo_id, status: 'running' } });
          window.dispatchEvent(new CustomEvent('executionStarted', { detail: { todoId: data.todo_id } }));
          break;
        }
        case 'Output': {
          dispatch({ type: 'APPEND_TASK_LOG', payload: { taskId: data.task_id, log: data.entry } });
          break;
        }
        case 'TodoProgress': {
          dispatch({ type: 'UPDATE_TASK_TODO_PROGRESS', payload: { taskId: data.task_id, progress: data.progress } });
          break;
        }
        case 'ExecutionStats': {
          dispatch({ type: 'UPDATE_TASK_EXECUTION_STATS', payload: { taskId: data.task_id, stats: data.stats } });
          break;
        }
        case 'Finished': {
          dispatch({ type: 'FINISH_TASK', payload: { taskId: data.task_id, todoId: data.todo_id, success: data.success, result: data.result } });
          dispatch({ type: 'UPDATE_TODO_STATUS', payload: { id: data.todo_id, status: data.success ? 'completed' : 'failed' } });
          // 3 秒后自动从 runningTasks 中移除已结束的任务
          const timer = setTimeout(() => {
            sharedRemoveTaskTimers.delete(timer);
            dispatch({ type: 'REMOVE_RUNNING_TASK', payload: data.task_id });
          }, 3000);
          sharedRemoveTaskTimers.add(timer);
          window.dispatchEvent(new CustomEvent('executionFinished', { detail: { todoId: data.todo_id, success: data.success } }));
          break;
        }
        case 'ReviewStatusChanged': {
          window.dispatchEvent(new CustomEvent('reviewStatusChanged', { detail: { recordId: data.record_id, todoId: data.todo_id, reviewStatus: data.review_status } }));
          break;
        }
        case 'LoopFinished': {
          window.dispatchEvent(new CustomEvent('loopExecutionFinished', { detail: { loopExecutionId: data.loop_execution_id, loopId: data.loop_id, status: data.status, totalSteps: data.total_steps, completedSteps: data.completed_steps, failedSteps: data.failed_steps, durationSecs: data.duration_secs, totalTokens: data.total_tokens } }));
          break;
        }
      }
    } catch {
      // JSON 解析失败的事件直接忽略（非关键路径，不影响核心流程）
    }
  };

  ws.onclose = () => {
    sharedWs = null;
    if (sharedShouldReconnect) {
      const delay = getReconnectDelay();
      sharedReconnectAttempt += 1;
      sharedReconnectTimer = setTimeout(() => {
        sharedReconnectTimer = null;
        connectShared(dispatch);
      }, delay);
    }
  };
  ws.onerror = () => {
    // onerror 后必然触发 onclose，由 onclose 统一处理重连
  };
}

/** 清理全局 WebSocket 及所有相关资源 */
function teardownShared() {
  sharedShouldReconnect = false;
  if (sharedReconnectTimer) {
    clearTimeout(sharedReconnectTimer);
    sharedReconnectTimer = null;
  }
  sharedRemoveTaskTimers.forEach(clearTimeout);
  sharedRemoveTaskTimers.clear();
  if (sharedWs) {
    sharedWs.close();
    sharedWs = null;
  }
}

/**
 * useExecutionEvents — 全局单例 WebSocket 事件订阅。
 *
 * 为什么是单例：
 * App.tsx 和 LoopStudioExecutionsPanel 都需要监听执行事件，但全局只需维护
 * **一个** WebSocket 连接。多个 WS 连接会使同一事件被重复 dispatch 到 state，
 * 导致执行日志翻倍、冗余状态更新等问题。
 *
 * @param onRefresh - 可选的回调，每次收到 WS 事件时触发（用于面板刷新等用途）
 */
export function useExecutionEvents(onRefresh?: () => void) {
  const { dispatch } = useApp();

  // 用 ref 持有 onRefresh，使其始终指向最新值但不触发 effect 重新执行
  const onRefreshRef = useRef(onRefresh);
  onRefreshRef.current = onRefresh;

  useEffect(() => {
    // 在 effect 内初始化全局 dispatch，避免渲染期间的副作用。
    // dispatch 引用稳定（useCallback），只在首次挂载时赋值即可。
    if (!sharedDispatch) {
      sharedDispatch = dispatch;
    }

    // 递增调用方计数，把 ref 推入数组（后续触发时读 ref.current 总能拿到最新回调）
    sharedInstanceCount += 1;
    sharedOnRefreshRefs.push(onRefreshRef);

    // 第一个调用方负责创建 WS 连接
    if (sharedInstanceCount === 1) {
      sharedShouldReconnect = true;
      sharedReconnectAttempt = 0;
      connectShared(sharedDispatch!);
    }

    return () => {
      // 递减调用方计数，从数组中移除 ref
      sharedInstanceCount -= 1;
      sharedOnRefreshRefs = sharedOnRefreshRefs.filter(r => r !== onRefreshRef);

      // 最后一个调用方卸载时，清理全局 WS 资源
      if (sharedInstanceCount <= 0) {
        teardownShared();
        sharedDispatch = null;
      }
    };
    // dispatch 引用稳定（useCallback），不会导致 effect 重跑。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}
