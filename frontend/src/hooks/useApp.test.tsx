// 093 批次2 评审补测：useAppDispatch / routeAppAction 路由表 / 各域 dispatch hook。
//
// 评审指出：四个新公开 hook 与路由函数零单测——「单一事实源」的 routeAppAction
// 若某个 action type 拼错会被静默丢弃（无 else 分支），必须逐 type 验证落域正确。
// 本文件经 useAppDispatch 走真实 Provider + 真实 reducer 做集成式断言：
// 每个 action type 分发后断言「目标域 state 变化 + 其它域纹丝不动」。
import { describe, it, expect } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import type { ReactNode } from 'react';
import { useAppDispatch } from './useApp';
import { useTodos, useTodosDispatch, TodoProvider } from './useTodoContext';
import { useExecution, useExecutionDispatch, ExecutionProvider } from './useExecutionContext';
import { useUI, useUIDispatch, UIProvider } from './useUIContext';
import { useTaskLogs, LogsProvider } from './useLogsContext';
import type { ExecutionRecord, RunningTask, ExecutionStats, LogEntry } from '@/types';

// ─── Provider 组合：与 AppProvider 同序但去掉 DataLoader ──────────
// DataLoader 会触发 db.getWorkspaces()（网络/存储依赖），与本组纯路由断言无关；
// 层序必须与 AppProvider 一致（dispatch Provider 都在 state Provider 外层）。
function wrapper({ children }: { children: ReactNode }) {
  return (
    <UIProvider>
      <LogsProvider>
        <ExecutionProvider>
          <TodoProvider>{children}</TodoProvider>
        </ExecutionProvider>
      </LogsProvider>
    </UIProvider>
  );
}

// ─── 测试夹具：只填必填字段，可选字段让 reducer 分支自己暴露问题 ──
const STATS: ExecutionStats = { tool_calls: 1, conversation_turns: 2, thinking_count: 3 };
const makeRecord = (id: number, status: ExecutionRecord['status'] = 'success'): ExecutionRecord => ({
  id, todo_id: 1, status, command: 'cmd', stdout: '', stderr: '', result: null,
  started_at: '2026-08-23T00:00:00Z', finished_at: null, usage: null,
  executor: 'claudecode', model: null, trigger_type: 'manual', pid: null,
});
const makeTask = (taskId: string): RunningTask => ({
  taskId, todoId: 1, todoTitle: 'fixture', executor: 'claudecode',
  status: 'running', startedAt: '2026-08-23T00:00:00Z',
});
const log = (content: string): LogEntry => ({ timestamp: '2026-08-23T00:00:00Z', type: 'info', content });

// 一次挂载同时拿到合并 dispatch 与四域读取口：路由断言要在同一棵树上
// 观察「目标域变了、其它域没变」，分散多棵树会让隔离断言失去对照基准。
function useHarness() {
  return {
    appDispatch: useAppDispatch(),
    todo: useTodos(),
    exec: useExecution(),
    ui: useUI(),
    logs: useTaskLogs('t1'),
  };
}

describe('093 批次2 useAppDispatch / routeAppAction', () => {
  it('test_useAppDispatch_routes_todo_domain_actions', () => {
    const { result } = renderHook(() => useHarness(), { wrapper });
    const d = result.current.appDispatch;
    // 两个 todo action 逐一落域，且不牵动执行/ui 域（隔离断言）
    act(() => d({ type: 'SELECT_TODO', payload: 7 }));
    expect(result.current.todo.state.selectedTodoId).toBe(7);
    act(() => d({ type: 'SELECT_WORKSPACE', payload: 3 }));
    expect(result.current.todo.state.selectedWorkspace).toBe(3);
    expect(result.current.exec.state.executionRecords).toEqual({});
    expect(result.current.ui.state.loading).toBe(true);
  });

  it('test_useAppDispatch_routes_execution_domain_actions', () => {
    const { result } = renderHook(() => useHarness(), { wrapper });
    const d = result.current.appDispatch;
    // 10 个执行 action 全量走一遍：任一 type 在路由表拼错即静默丢弃，此处会失败
    act(() => d({ type: 'SET_EXECUTION_RECORDS', payload: { todoId: 1, records: [makeRecord(1)] } }));
    expect(result.current.exec.state.executionRecords[1]).toHaveLength(1);
    act(() => d({ type: 'ADD_EXECUTION_RECORD', payload: { todoId: 1, record: makeRecord(2, 'running') } }));
    expect(result.current.exec.state.executionRecords[1][0]?.id).toBe(2);
    act(() => d({ type: 'UPDATE_EXECUTION_RECORD', payload: { todoId: 1, record: makeRecord(2, 'failed') } }));
    expect(result.current.exec.state.executionRecords[1][0]?.status).toBe('failed');
    // 先 ADD 才有 running task，后续 FINISH/UPDATE_* 依赖其存在
    act(() => d({ type: 'ADD_RUNNING_TASK', payload: makeTask('t1') }));
    expect(result.current.exec.state.runningTasks.t1).toBeDefined();
    expect(result.current.exec.state.activeTaskId).toBe('t1');
    act(() => d({ type: 'UPDATE_TASK_TODO_PROGRESS', payload: { taskId: 't1', progress: [] } }));
    expect(result.current.exec.state.runningTasks.t1?.todoProgress).toEqual([]);
    act(() => d({ type: 'UPDATE_TASK_EXECUTION_STATS', payload: { taskId: 't1', stats: STATS } }));
    expect(result.current.exec.state.runningTasks.t1?.executionStats).toEqual(STATS);
    act(() => d({ type: 'FINISH_TASK', payload: { taskId: 't1', todoId: 1, success: true, result: 'ok' } }));
    expect(result.current.exec.state.runningTasks.t1?.status).toBe('finished');
    act(() => d({ type: 'SET_ACTIVE_TASK', payload: null }));
    expect(result.current.exec.state.activeTaskId).toBeNull();
    act(() => d({ type: 'REMOVE_RUNNING_TASK', payload: 't1' }));
    expect(result.current.exec.state.runningTasks.t1).toBeUndefined();
    act(() => d({ type: 'ADD_RUNNING_TASK', payload: makeTask('t2') }));
    act(() => d({ type: 'CLEAR_RUNNING_TASKS' }));
    expect(result.current.exec.state.runningTasks).toEqual({});
    // 隔离：上述全部为执行域动作，todo/ui 域不得被卷入
    expect(result.current.todo.state.selectedTodoId).toBeNull();
    expect(result.current.ui.state.loading).toBe(true);
  });

  it('test_useAppDispatch_routes_logs_and_ui_actions', () => {
    const { result } = renderHook(() => useHarness(), { wrapper });
    const d = result.current.appDispatch;
    // 4 个日志 action + 唯一 ui action，经 useTaskLogs/useUI 观测落域
    act(() => d({ type: 'SET_TASK_LOGS', payload: { taskId: 't1', logs: [log('a')] } }));
    expect(result.current.logs).toHaveLength(1);
    act(() => d({ type: 'APPEND_TASK_LOGS', payload: { taskId: 't1', logs: [log('b')] } }));
    expect(result.current.logs).toHaveLength(2);
    act(() => d({ type: 'REMOVE_TASK_LOGS', payload: 't1' }));
    expect(result.current.logs).toHaveLength(0);
    act(() => d({ type: 'SET_TASK_LOGS', payload: { taskId: 't1', logs: [log('c'), log('d')] } }));
    act(() => d({ type: 'CLEAR_LOGS' }));
    expect(result.current.logs).toHaveLength(0);
    act(() => d({ type: 'SET_LOADING', payload: false }));
    expect(result.current.ui.state.loading).toBe(false);
    // 隔离：日志/ui 动作不得牵动 todo/执行域
    expect(result.current.todo.state.selectedTodoId).toBeNull();
    expect(result.current.exec.state.runningTasks).toEqual({});
  });

  it('test_useAppDispatch_returns_stable_reference_across_rerender', () => {
    // 「零订阅」承诺的一半是 dispatch 引用恒定：rerender 后必须是同一个函数引用，
    // 否则依赖它的 effect 会反复重建（WS 重连风暴的隐患）。
    const { result, rerender } = renderHook(() => useAppDispatch(), { wrapper });
    const first = result.current;
    rerender();
    expect(result.current).toBe(first);
  });

  // ─── 错误分支：缺 Provider 必须显式抛错而非静默返回 undefined ──
  it('test_useAppDispatch_outside_providers_throws', () => {
    expect(() => renderHook(() => useAppDispatch())).toThrow(/must be used within/);
  });

  it('test_useTodosDispatch_outside_provider_throws', () => {
    expect(() => renderHook(() => useTodosDispatch())).toThrow(/within TodoProvider/);
  });

  it('test_useExecutionDispatch_outside_provider_throws', () => {
    expect(() => renderHook(() => useExecutionDispatch())).toThrow(/within ExecutionProvider/);
  });

  it('test_useUIDispatch_outside_provider_throws', () => {
    expect(() => renderHook(() => useUIDispatch())).toThrow(/within UIProvider/);
  });
});
