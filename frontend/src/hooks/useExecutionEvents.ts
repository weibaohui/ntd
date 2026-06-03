import { useEffect, useRef } from 'react';
import { useApp } from './useApp';
import type { LogEntry, TodoItem, ExecutionStats } from '../types';

interface ExecEventStarted {
  type: 'Started';
  task_id: string;
  todo_id: number;
  todo_title: string;
  executor: string;
}

interface ExecEventOutput {
  type: 'Output';
  task_id: string;
  entry: LogEntry;
}

interface ExecEventFinished {
  type: 'Finished';
  task_id: string;
  todo_id: number;
  success: boolean;
  result: string | null;
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
}

interface ExecEventExecutionStats {
  type: 'ExecutionStats';
  task_id: string;
  stats: ExecutionStats;
}

type ExecEvent = ExecEventStarted | ExecEventOutput | ExecEventFinished | ExecEventSync | ExecEventTodoProgress | ExecEventExecutionStats;

export function useExecutionEvents() {
  const { dispatch } = useApp();
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const reconnectAttemptRef = useRef(0);
  const removeTaskTimersRef = useRef<Set<ReturnType<typeof setTimeout>>>(new Set());

  /** Exponential backoff with jitter: min(2^n * 1000, 30000) + random(0, 1000) */
  const getReconnectDelay = () => {
    const n = reconnectAttemptRef.current;
    const base = Math.min(Math.pow(2, n) * 1000, 30000);
    const jitter = Math.floor(Math.random() * 1000);
    return base + jitter;
  };

  useEffect(() => {
    let shouldReconnect = true;
    // Reset attempt count on fresh mount
    reconnectAttemptRef.current = 0;

    function connect() {
      if (!shouldReconnect) return;

      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const ws = new WebSocket(`${protocol}//${window.location.host}/api/events`);
      wsRef.current = ws;

      ws.onopen = () => {
        // Reset backoff on successful connection
        reconnectAttemptRef.current = 0;
      };

      ws.onmessage = (event) => {
        if (event.data === 'Connected') return;

        try {
          const data: ExecEvent = JSON.parse(event.data);

          switch (data.type) {
            case 'Sync': {
              dispatch({ type: 'CLEAR_RUNNING_TASKS' });

              data.tasks.forEach(task => {
                let parsedLogs: LogEntry[] = [];
                try {
                  parsedLogs = JSON.parse(task.logs || '[]');
                } catch {
                  // ignore malformed logs
                }

                dispatch({
                  type: 'ADD_RUNNING_TASK',
                  payload: {
                    taskId: task.task_id,
                    todoId: task.todo_id,
                    todoTitle: task.todo_title,
                    executor: task.executor || 'claudecode',
                    logs: parsedLogs,
                    status: 'running',
                    startedAt: new Date().toISOString(),
                  },
                });

                dispatch({
                  type: 'UPDATE_TODO_STATUS',
                  payload: { id: task.todo_id, status: 'running' },
                });
              });
              break;
            }
            case 'Started': {
              dispatch({
                type: 'ADD_RUNNING_TASK',
                payload: {
                  taskId: data.task_id,
                  todoId: data.todo_id,
                  todoTitle: data.todo_title,
                  executor: data.executor || 'claudecode',
                  logs: [],
                  status: 'running',
                  startedAt: new Date().toISOString(),
                },
              });
              dispatch({
                type: 'UPDATE_TODO_STATUS',
                payload: { id: data.todo_id, status: 'running' },
              });
              break;
            }
            case 'Output': {
              dispatch({
                type: 'APPEND_TASK_LOG',
                payload: { taskId: data.task_id, log: data.entry },
              });
              break;
            }
            case 'TodoProgress': {
              dispatch({
                type: 'UPDATE_TASK_TODO_PROGRESS',
                payload: { taskId: data.task_id, progress: data.progress },
              });
              break;
            }
            case 'ExecutionStats': {
              dispatch({
                type: 'UPDATE_TASK_EXECUTION_STATS',
                payload: { taskId: data.task_id, stats: data.stats },
              });
              break;
            }
            case 'Finished': {
              dispatch({
                type: 'FINISH_TASK',
                payload: {
                  taskId: data.task_id,
                  todoId: data.todo_id,
                  success: data.success,
                  result: data.result,
                },
              });
              const newStatus = data.success ? 'completed' : 'failed';
              dispatch({
                type: 'UPDATE_TODO_STATUS',
                payload: { id: data.todo_id, status: newStatus },
              });
              const timer = setTimeout(() => {
                removeTaskTimersRef.current.delete(timer);
                dispatch({ type: 'REMOVE_RUNNING_TASK', payload: data.task_id });
              }, 3000);
              removeTaskTimersRef.current.add(timer);
              break;
            }
          }
        } catch {
          // ignore malformed messages
        }
      };

      ws.onclose = () => {
        wsRef.current = null;
        if (shouldReconnect) {
          const delay = getReconnectDelay();
          reconnectAttemptRef.current += 1;
          reconnectTimerRef.current = setTimeout(() => {
            reconnectTimerRef.current = null;
            connect();
          }, delay);
        }
      };

      ws.onerror = () => {};
    }

    connect();

    return () => {
      shouldReconnect = false;
      if (reconnectTimerRef.current) {
        clearTimeout(reconnectTimerRef.current);
        reconnectTimerRef.current = null;
      }
      removeTaskTimersRef.current.forEach(clearTimeout);
      removeTaskTimersRef.current.clear();
      if (wsRef.current) {
        wsRef.current.close();
        wsRef.current = null;
      }
    };
  }, [dispatch]);
}
