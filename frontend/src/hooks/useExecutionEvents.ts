import { useEffect, useRef } from 'react';
import { useApp } from './useApp';
import type { LogEntry, TodoItem, ExecutionStats } from '@/types';
import { TODO_LIST_REFRESH_EVENT } from '@/constants';

/**
 * WS（重）连后端推全量 Sync 快照时广播的 window 事件名。
 *
 * 091：讨论帖、运行看板原本各挂一个 setInterval（4s / 60s）做断线兜底轮询，
 * 现统一改为纯事件驱动——用这条「重连即全量同步」信号触发一次单次刷新，
 * 纠正断线期间漏掉的执行态变化，替代定时轮询（事件而非轮询，无变化时不打请求）。
 */
export const EXECUTION_SYNC_EVENT = 'executionSync';

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
    // 091：该任务执行日志的全量条数（logs 只含最近 RECONNECT_LOG_CAP 条）。
    log_total: number;
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

/** 黑板防抖状态：双进度条倒计时数据 */
export interface BlackboardDebounceStatus {
  workspace_id: number;
  pending_count: number;
  threshold: number;
  debounce_secs: number;
  remaining_secs: number; // -1 表示无 active timer
  refreshing: boolean;
}

/** Wiki 对话开始事件：用户发起对话、执行器启动时推送 */
export interface WikiChatStartedEvent {
  type: 'WikiChatStarted';
  task_id: string;
  workspace_id: number;
  executor: string;
  message: string;
}

/** Wiki 对话输出事件：执行器每解析出一行日志就推送一次 */
export interface WikiChatOutputEvent {
  type: 'WikiChatOutput';
  task_id: string;
  workspace_id: number;
  entry: LogEntry;
}

/** Wiki 对话完成事件：执行器退出时推送，携带最终结果 */
export interface WikiChatFinishedEvent {
  type: 'WikiChatFinished';
  task_id: string;
  workspace_id: number;
  success: boolean;
  result: string | null;
  duration_secs: number;
}

type ExecEvent = ExecEventStarted | ExecEventOutput | ExecEventFinished | ExecEventSync | ExecEventTodoProgress | ExecEventExecutionStats | ExecEventReviewStatusChanged | ExecEventLoopFinished | { type: 'BlackboardDebounceStatus' } & BlackboardDebounceStatus | WikiChatStartedEvent | WikiChatOutputEvent | WikiChatFinishedEvent;

// ─── 模块级共享状态 ─────────────────────────────────────────────
//
// 为什么用模块级单例而不是 React state/ref：
// useExecutionEvents 可能在多个组件中被调用（App.tsx + LoopStudioExecutionsPanel），
// 如果每个调用方都创建独立的 WebSocket，事件会被重复处理 → 执行日志翻倍。
// 模块级变量在所有组件实例间共享，确保全局只有一个 WebSocket 连接。
// （见 issue #720 分析：https://github.com/weibaohui/ntd/issues/720）

/** 全局唯一 WebSocket 连接实例 */
let sharedWs: WebSocket | null = null;
/** 094：当前连接声明的 workspace。null = 未声明（服务端全推，兼容旧行为）。
 *  模块级记忆的原因：onclose 自动重连不在 React effect 内，拿不到最新 state，
 *  必须沿用它——保证「重连不改变订阅范围」。 */
let sharedWorkspaceId: number | null = null;
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
/** 自动清除已结束任务的定时器：key=taskId（056 改为 Map，支持 Sync 时按任务撤销）。 */
let sharedRemoveTaskTimers = new Map<string, ReturnType<typeof setTimeout>>();
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
  // 094：声明 workspace 订阅范围，服务端按 workspace 过滤事件（预序列化 + 隔离）；
  // null 时不带参数——服务端视为未声明，保持全推兼容口
  const wsParam = sharedWorkspaceId != null ? `?workspace_id=${sharedWorkspaceId}` : '';
  const ws = new WebSocket(`${protocol}//${window.location.host}/api/events${wsParam}`);
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
      // 091：改为 trailing debounce，避免 Output 日志洪峰下每行日志都触发一次调用方刷新。
      triggerOnRefreshDebounced();

      switch (data.type) {
        case 'Sync': {
          dispatch({ type: 'CLEAR_RUNNING_TASKS' });
          // 091：执行态与日志解耦后，Sync 需分别清两个 context。
          // Sync 带的是后端权威日志快照，缓冲里断线前的残余日志一律作废。
          dispatch({ type: 'CLEAR_LOGS' });
          outputBuffer.clear();
          data.tasks.forEach(task => {
            let parsedLogs: LogEntry[] = [];
            try { parsedLogs = JSON.parse(task.logs || '[]'); } catch {}
            // 056（L6 修复）：Sync 把该任务重置为 running，撤销可能存在的自动移除定时器，
            // 否则重连后旧定时器会把 Sync 恢复的任务再次移除（任务闪现/提前消失）。
            cancelRemoveTimer(task.task_id);
            // 091：RunningTask 不再携带 logs 字段（已迁至 LogsContext）。
            dispatch({ type: 'ADD_RUNNING_TASK', payload: { taskId: task.task_id, todoId: task.todo_id, todoTitle: task.todo_title, executor: task.executor || 'claudecode', status: 'running', startedAt: new Date().toISOString() } });
            // 091：日志整体替换走 SET_TASK_LOGS，与执行态 reducer 完全隔离；
            // 带上 log_total（全量条数）供面板提示「共 N 条」，logs 只含最近 N 条。
            dispatch({ type: 'SET_TASK_LOGS', payload: { taskId: task.task_id, logs: parsedLogs, total: task.log_total } });
          });
          // 056：全局 todos 桶已删除，列表页改从服务端拉取——Sync 到达即通知列表刷新
          dispatchListRefreshDebounced();
          // 091：广播重连/全量同步信号，让纯事件驱动的视图（讨论帖、运行看板）
          // 在断线重连后补刷一次，替代被移除的兜底定时轮询。
          window.dispatchEvent(new Event(EXECUTION_SYNC_EVENT));
          break;
        }
        case 'Started': {
          // 091：新任务起始无日志，logs 已不在执行态；不预置 logs 避免触发 LogsContext 写入。
          dispatch({ type: 'ADD_RUNNING_TASK', payload: { taskId: data.task_id, todoId: data.todo_id, todoTitle: data.todo_title, executor: data.executor || 'claudecode', status: 'running', startedAt: new Date().toISOString() } });
          // 056：状态不再写全局桶，改为通知列表页刷新（服务端数据源为准）
          dispatchListRefreshDebounced();
          window.dispatchEvent(new CustomEvent('executionStarted', { detail: { todoId: data.todo_id } }));
          break;
        }
        case 'Output': {
          // 091：逐条 dispatch 会让 reducer 每行深拷贝一次日志数组；
          // 攒进 outputBuffer，50ms 后按 task 合并一次 APPEND_TASK_LOGS。
          const buf = outputBuffer.get(data.task_id) || [];
          buf.push(data.entry);
          outputBuffer.set(data.task_id, buf);
          scheduleOutputFlush();
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
          // 3 秒后自动从 runningTasks 中移除已结束的任务；若期间 WS 重连收到 Sync，
          // Sync 会撤销该定时器（任务被重置为 running），避免误移除。
          const timer = setTimeout(() => {
            sharedRemoveTaskTimers.delete(data.task_id);
            dispatch({ type: 'REMOVE_RUNNING_TASK', payload: data.task_id });
            // 091：任务移出 runningTasks 时同步释放其日志，避免长跑后残留内存。
            dispatch({ type: 'REMOVE_TASK_LOGS', payload: data.task_id });
          }, 3000);
          sharedRemoveTaskTimers.set(data.task_id, timer);
          // 056：终态落定，通知列表页刷新（服务端数据源为准）
          dispatchListRefreshDebounced();
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
        case 'BlackboardDebounceStatus': {
          window.dispatchEvent(new CustomEvent('blackboardDebounceStatus', { detail: data }));
          break;
        }
        case 'WikiChatStarted': {
          window.dispatchEvent(new CustomEvent('wikiChatStarted', { detail: data }));
          break;
        }
        case 'WikiChatOutput': {
          window.dispatchEvent(new CustomEvent('wikiChatOutput', { detail: data }));
          break;
        }
        case 'WikiChatFinished': {
          window.dispatchEvent(new CustomEvent('wikiChatFinished', { detail: data }));
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
  // 056：清掉未触发的刷新 debounce，避免 teardown 后还向已卸载页面发事件
  if (refreshDebounceTimer) {
    clearTimeout(refreshDebounceTimer);
    refreshDebounceTimer = null;
  }
  // 091：同步清掉 onRefresh debounce，避免 teardown 后回调仍向已卸载页面触发刷新。
  if (onRefreshDebounceTimer) {
    clearTimeout(onRefreshDebounceTimer);
    onRefreshDebounceTimer = null;
  }
  // 091：关 WS 前先冲刷日志缓冲，避免最后一批日志丢失；再取消未触发定时器。
  flushOutputBuffer();
  if (outputFlushTimer) {
    clearTimeout(outputFlushTimer);
    outputFlushTimer = null;
  }
  if (sharedWs) {
    sharedWs.close();
    sharedWs = null;
  }
}

/** 056：撤销指定任务的自动移除定时器（Sync 重置 running 集时调用）。 */
function cancelRemoveTimer(taskId: string) {
  const timer = sharedRemoveTaskTimers.get(taskId);
  if (timer !== undefined) {
    clearTimeout(timer);
    sharedRemoveTaskTimers.delete(taskId);
  }
}

/** 列表刷新事件的共享 trailing debounce 定时器。 */
let refreshDebounceTimer: ReturnType<typeof setTimeout> | null = null;
/** onRefresh 回调的 debounce 定时器（091：每条事件都立即触发调用方刷新，
 *  Output 日志洪峰下会变成「每行日志一次刷新」，trailing 合并成 500ms 一次）。 */
let onRefreshDebounceTimer: ReturnType<typeof setTimeout> | null = null;

/** 列表刷新事件的 debounce 毫秒数（评审 I1：Loop 连续执行时 Started/Finished 密集触发，
 *  每条都让各列表视图重拉一页，30 步 Loop = 60 次全视图 HTTP。合并为 trailing 一次）。 */
const REFRESH_DEBOUNCE_MS = 500;

/** 500ms trailing debounce 派发列表刷新事件：密集触发只保留最后一次。 */
function dispatchListRefreshDebounced() {
  if (refreshDebounceTimer) clearTimeout(refreshDebounceTimer);
  refreshDebounceTimer = setTimeout(() => {
    refreshDebounceTimer = null;
    window.dispatchEvent(new Event(TODO_LIST_REFRESH_EVENT));
  }, REFRESH_DEBOUNCE_MS);
}

/** 500ms trailing debounce 触发 onRefresh 回调（模板同 dispatchListRefreshDebounced，091 性能优化）。
 *  onmessage 对每条事件都触发，Output 日志洪峰下立即触发会让调用方每行日志刷新一次。 */
function triggerOnRefreshDebounced() {
  if (onRefreshDebounceTimer) clearTimeout(onRefreshDebounceTimer);
  onRefreshDebounceTimer = setTimeout(() => {
    onRefreshDebounceTimer = null;
    sharedOnRefreshRefs.forEach(ref => ref.current?.());
  }, REFRESH_DEBOUNCE_MS);
}

// 091：Output 日志批量缓冲。
// WS Output 在执行器输出密集时几乎逐行到达（每条一条 LogEntry）。
// 若逐条 dispatch APPEND_TASK_LOGS，LogsContext reducer 会让每个 task 每行都
// 深拷贝整段日志数组并触发 ExecutionPanel 重渲染。改为 50ms 攒一批按 task 合并
// dispatch，把「每行一次 reducer」压成「每 50ms 一次」。
const OUTPUT_FLUSH_MS = 50;
/** 按 task 聚合的待刷日志缓冲：key=taskId，value=待追加的 LogEntry 数组。 */
let outputBuffer = new Map<string, LogEntry[]>();
/** 单个 flush 定时器：已有未触发定时器时复用，保证窗口内至多一个 flush。 */
let outputFlushTimer: ReturnType<typeof setTimeout> | null = null;

/** 安排一次 flush：窗口内多次 Output 只挂一个定时器，合并触发。 */
function scheduleOutputFlush() {
  if (outputFlushTimer) return;
  outputFlushTimer = setTimeout(() => {
    outputFlushTimer = null;
    flushOutputBuffer();
  }, OUTPUT_FLUSH_MS);
}

/** 取出缓冲并按 task 批量 dispatch APPEND_TASK_LOGS。
 *  先快照再置空：flush 期间新到达的日志进入新缓冲，不会被本次或下次误丢。 */
function flushOutputBuffer() {
  // sharedDispatch 在 effect 内赋值；WS 关闭后可能为 null，flush 应安全跳过。
  // 取到局部 const 再用：模块级 let 变量进闭包后会被 TS 还原为可空，无法直接调用。
  const dispatch = sharedDispatch;
  if (!dispatch) return;
  if (outputBuffer.size === 0) return;
  const batch = outputBuffer;
  outputBuffer = new Map();
  batch.forEach((logs, taskId) => {
    if (logs.length === 0) return;
    dispatch({ type: 'APPEND_TASK_LOGS', payload: { taskId, logs } });
  });
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
  const { state, dispatch } = useApp();
  // 094：订阅范围跟随全局 workspace 选择态
  const selectedWorkspace = state.selectedWorkspace;

  // 用 ref 持有 onRefresh，使其始终指向最新值但不触发 effect 重新执行
  const onRefreshRef = useRef(onRefresh);
  onRefreshRef.current = onRefresh;

  useEffect(() => {
    // 在 effect 内初始化全局 dispatch，避免渲染期间的副作用。
    // dispatch 引用稳定（useCallback），只在首次挂载时赋值即可。
    if (!sharedDispatch) {
      sharedDispatch = dispatch;
    }

    // 094：首连前同步订阅范围——初始挂载时把当前 workspace 写入模块级记忆，
    // connectShared 建连时读取（此后 onclose 自动重连沿用同一范围）
    sharedWorkspaceId = selectedWorkspace;

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
        // 094：卸载复位订阅范围，下次挂载不受残留值影响
        sharedWorkspaceId = null;
      }
    };
    // dispatch 引用稳定（useCallback），不会导致 effect 重跑；
    // selectedWorkspace 的响应式处理在下方独立 effect（避免首连/重连双重触发）。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 094：workspace 切换 → 重建 WS 连接（订阅范围随连接参数生效，只能重连切换）。
  // 跳过首次运行：首连已在上方 effect 内带参建立，此处重复会多一次无谓断连。
  const prevWorkspaceRef = useRef(selectedWorkspace);
  useEffect(() => {
    if (prevWorkspaceRef.current === selectedWorkspace) return;
    prevWorkspaceRef.current = selectedWorkspace;
    // 无活跃连接时只更新记忆（下一个调用方挂载时会用新范围建连）
    sharedWorkspaceId = selectedWorkspace;
    if (!sharedWs) return;
    // teardown 会把 sharedShouldReconnect 置 false（语义是「不再续命」），
    // 而切换场景需要「断旧连新」，故重置之；计数清零让新连接跳过退避立即建立
    teardownShared();
    sharedShouldReconnect = true;
    sharedReconnectAttempt = 0;
    if (sharedDispatch) connectShared(sharedDispatch);
    // selectedWorkspace 每次真实变化都需要重连；teardown/connect 均为模块级稳定函数
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedWorkspace]);
}
