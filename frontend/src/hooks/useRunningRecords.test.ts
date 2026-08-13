import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import * as db from '@/utils/database';
import { useRunningRecords } from './useRunningRecords';
import type { ExecutionRecord, ExecutorConfig } from '@/types';

// 稳定 message mock（vi.hoisted）。
const mockMessage = vi.hoisted(() => ({
  error: vi.fn(),
  success: vi.fn(),
  warning: vi.fn(),
  info: vi.fn(),
}));
vi.mock('antd', async (importOriginal) => {
  const actual = await importOriginal<typeof import('antd')>();
  return {
    ...actual,
    App: { ...actual.App, useApp: () => ({ message: mockMessage }) },
  };
});

// useTodos：提供 selectedWorkspace（运行记录按 workspace 归属查询）。
vi.mock('@/hooks/useTodoContext', () => ({
  useTodos: () => ({ state: { selectedWorkspace: 1 } }),
}));

// useAutoRefreshRunningBoard：事件驱动刷新订阅，单测里桩成空实现。
vi.mock('@/hooks/useRunningBoard', () => ({
  useAutoRefreshRunningBoard: vi.fn(),
}));

vi.mock('@/utils/database', () => ({
  getRunningExecutionRecords: vi.fn(),
  getTodoBriefs: vi.fn(),
  forceFailExecution: vi.fn(),
}));

function makeRecord(over: Partial<ExecutionRecord> = {}): ExecutionRecord {
  return {
    id: 1,
    todo_id: 10,
    status: 'running' as never,
    command: '',
    stdout: '',
    stderr: '',
    result: null,
    started_at: '',
    finished_at: null,
    ...over,
  } as ExecutionRecord;
}

describe('useRunningRecords', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('loadRunningRecords：拉记录 + 按 todo_id 集反查 brief 标题', async () => {
    const records = [makeRecord({ id: 1, todo_id: 10 }), makeRecord({ id: 2, todo_id: 11 })];
    vi.mocked(db.getRunningExecutionRecords).mockResolvedValue(records);
    vi.mocked(db.getTodoBriefs).mockResolvedValue([
      { id: 10, title: '任务十' } as never,
      { id: 11, title: '任务十一' } as never,
    ]);
    const { result } = renderHook(() => useRunningRecords([]));

    await act(async () => {
      await result.current.loadRunningRecords();
    });

    expect(result.current.runningRecords).toHaveLength(2);
    // brief 按 todo_id 集一次拉取。
    expect(db.getTodoBriefs).toHaveBeenCalledWith(1, { ids: [10, 11] });
    expect(result.current.recordTodos).toHaveLength(2);
  });

  it('loadRunningRecords：无记录时清空 recordTodos（去重 todo_id 为空集）', async () => {
    vi.mocked(db.getRunningExecutionRecords).mockResolvedValue([]);
    const { result } = renderHook(() => useRunningRecords([]));

    await act(async () => {
      await result.current.loadRunningRecords();
    });
    expect(result.current.runningRecords).toHaveLength(0);
    expect(result.current.recordTodos).toHaveLength(0);
    expect(db.getTodoBriefs).not.toHaveBeenCalled();
  });

  it('handleBatchStop：对每个勾选 id 调 forceFail，清选 + 重拉 + 计数提示', async () => {
    vi.mocked(db.getRunningExecutionRecords).mockResolvedValue([]);
    vi.mocked(db.forceFailExecution).mockResolvedValue(undefined);
    const { result } = renderHook(() => useRunningRecords([]));

    // 预置勾选 3 条；让第 2 条失败（reject），验证部分失败计数。
    act(() => {
      result.current.setSelectedRecordIds([1, 2, 3]);
    });
    vi.mocked(db.forceFailExecution).mockImplementation(async (_ws: number, id: number) => {
      if (id === 2) throw new Error('boom');
    });

    await act(async () => {
      await result.current.handleBatchStop();
    });

    // 3 条都尝试停止（allSettled 不中断）。
    expect(db.forceFailExecution).toHaveBeenCalledTimes(3);
    expect(mockMessage.success).toHaveBeenCalledWith('已停止 2 个任务');
    expect(mockMessage.error).toHaveBeenCalledWith('1 个任务停止失败');
    // 清选 + 重拉（getRunningExecutionRecords 被调两次：handleBatchStop 末尾的 loadRunningRecords）。
    expect(result.current.selectedRecordIds).toHaveLength(0);
    expect(db.getRunningExecutionRecords).toHaveBeenCalled();
  });

  it('handleBatchStop：未勾选时直接返回，不调 forceFail', async () => {
    vi.mocked(db.forceFailExecution).mockResolvedValue(undefined);
    const { result } = renderHook(() => useRunningRecords([]));

    await act(async () => {
      await result.current.handleBatchStop();
    });
    expect(db.forceFailExecution).not.toHaveBeenCalled();
  });

  it('executorDisplayNames：由 executors 派生 name→display_name 映射', async () => {
    vi.mocked(db.getRunningExecutionRecords).mockResolvedValue([]);
    const { result, rerender } = renderHook(({ executors }) => useRunningRecords(executors), {
      initialProps: { executors: [] as ExecutorConfig[] },
    });
    expect(result.current.executorDisplayNames).toEqual({});

    rerender({
      executors: [
        { name: 'claude', display_name: 'Claude' } as ExecutorConfig,
        { name: 'codex', display_name: 'Codex' } as ExecutorConfig,
      ],
    });
    await waitFor(() =>
      expect(result.current.executorDisplayNames).toEqual({ claude: 'Claude', codex: 'Codex' }),
    );
  });
});
