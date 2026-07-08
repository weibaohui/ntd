import { useState, useCallback, useEffect, useRef } from 'react';
import { getExecutionRecord } from '@/utils/database';
import { useExecution } from '@/hooks/useExecutionContext';
import type { LogEntry } from '@/types';
import type { ActionStatus } from './types';
import { api, unwrap } from '@/utils/database/client';

interface UseActionExecutionReturn {
  status: ActionStatus;
  result: string | null;
  error: string | null;
  recordId: number | null;
  /**
   * 当前任务的实时日志流（来自 WS Output 事件 push 进 runningTasks 的 logs）。
   * 执行中用于在 UI 上展示 AI 思考/工具调用过程，避免「黑盒转圈」；
   * 任务未启动或 WS 未推时为空数组。
   */
  logs: LogEntry[];
  execute: (prompt: string, executor?: string) => Promise<void>;
  retry: (prompt: string, executor?: string) => Promise<void>;
  reset: () => void;
}

interface ExecuteActionResult {
  task_id: string;
  record_id: number;
  todo_id: number;
  todo_created: boolean;
}

/**
 * 调用后端 POST /api/actions/execute 接口。
 */
async function callActionExecute(
  actionType: string,
  actionKey: string,
  prompt: string,
  params: Record<string, string>,
  workspaceId?: number,
  executor?: string,
): Promise<ExecuteActionResult> {
  return unwrap(
    await api.post('/api/actions/execute', {
      action_type: actionType,
      action_key: actionKey,
      prompt,
      params,
      workspace_id: workspaceId,
      executor,
    })
  );
}

/**
 * 管理 ActionButton 的执行状态。
 *
 * 流程：
 * 1. execute() → 调用 POST /api/actions/execute
 * 2. 通过 useExecutionContext 的 WebSocket 事件监听执行完成
 * 3. 收到 FINISH_TASK 事件后，查询 execution_record 获取 result
 */
export function useActionExecution(
  actionType: string,
  actionKey: string,
  _defaultPrompt: string,
  params: Record<string, string>,
  workspaceId?: number,
  _defaultExecutor?: string,
): UseActionExecutionReturn {
  const [status, setStatus] = useState<ActionStatus>('idle');
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [recordId, setRecordId] = useState<number | null>(null);
  const taskIdRef = useRef<string | null>(null);
  const { state } = useExecution();

  // 从全局 runningTasks 取当前任务的实时日志流。taskIdRef 在 execute() 拿到 task_id 后赋值；
  // state.runningTasks 由 WS 事件驱动更新，Output 事件会持续 append logs → 本组件随之重渲染。
  // 直接在渲染期读 ref + state 是安全的：ref 写入发生在 execute 闭包，state 变化由订阅触发。
  const runningTask = taskIdRef.current ? state.runningTasks[taskIdRef.current] : undefined;
  const logs: LogEntry[] = runningTask?.logs ?? [];

  // 监听 WebSocket 的 FINISH_TASK 事件
  useEffect(() => {
    if (status !== 'executing' || !taskIdRef.current) return;

    const task = state.runningTasks[taskIdRef.current];
    if (task?.status === 'finished') {
      if (task.success && task.result) {
        setResult(task.result);
        setStatus('completed');
      } else if (!task.success) {
        setError(task.result || '执行失败');
        setStatus('failed');
      } else {
        fetchResultFromRecord();
      }
    }
  }, [state.runningTasks, status]);

  // 从 execution_record 查询结果（WebSocket 事件没有 result 时的 fallback）
  const fetchResultFromRecord = useCallback(async () => {
    if (!recordId) return;
    try {
      const record = await getExecutionRecord(recordId);
      if (record.result) {
        setResult(record.result);
        setStatus('completed');
      } else if (record.status === 'failed') {
        setError(record.stderr || '执行失败');
        setStatus('failed');
      }
    } catch (err: any) {
      setError(err?.message || '查询执行结果失败');
      setStatus('failed');
    }
  }, [recordId]);

  const execute = useCallback(async (prompt: string, executor?: string) => {
    setStatus('executing');
    setResult(null);
    setError(null);

    try {
      const res = await callActionExecute(
        actionType,
        actionKey,
        prompt,
        params,
        workspaceId,
        executor,
      );
      setRecordId(res.record_id);
      taskIdRef.current = res.task_id;
    } catch (err: any) {
      setError(err?.message || '启动执行失败');
      setStatus('failed');
    }
  }, [actionType, actionKey, params, workspaceId]);

  const retry = useCallback(async (prompt: string, executor?: string) => {
    await execute(prompt, executor);
  }, [execute]);

  const reset = useCallback(() => {
    setStatus('idle');
    setResult(null);
    setError(null);
    setRecordId(null);
    taskIdRef.current = null;
  }, []);

  return { status, result, error, recordId, logs, execute, retry, reset };
}
