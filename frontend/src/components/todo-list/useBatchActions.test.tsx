// useBatchActions 单元测试。
// 验证 hook 的基本结构：batchActions 数量正确、modals 存在。
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import type { ReactNode } from 'react';

// 使用 vi.hoisted 创建模块级的 mock 对象，避免 vi.mock 工厂函数中的 hoisting 问题
const mockMessage = vi.hoisted(() => ({ success: vi.fn(), warning: vi.fn(), error: vi.fn() }));

vi.mock('antd', () => ({
  message: mockMessage,
  Modal: { confirm: vi.fn() },
  App: { useApp: () => ({ message: mockMessage, notification: { error: vi.fn() }, modal: { confirm: vi.fn() } }) },
  Button: ({ children, ...props }: { children?: ReactNode; [key: string]: unknown }) => <button {...props}>{children}</button>,
}));

vi.mock('@/utils/database', () => ({
  batchUpdateTodosExecutor: vi.fn(),
  batchCopyTodosWorkspace: vi.fn(),
  batchMoveTodosWorkspace: vi.fn(),
  batchPauseScheduler: vi.fn(),
  batchResumeScheduler: vi.fn(),
}));

vi.mock('@/utils/database/loops', () => ({
  forceStopLoops: vi.fn(),
  batchCopyLoopsWorkspace: vi.fn(),
  batchMoveLoopsWorkspace: vi.fn(),
}));

vi.mock('@/components/todo-drawer/ExecutorPicker', () => ({
  ExecutorPicker: () => null,
}));

vi.mock('@/components/shell/WorkspaceSwitcher', () => ({
  WorkspaceSwitcher: () => null,
}));

import { useBatchActions } from './useBatchActions';

describe('useBatchActions', () => {
  const mockRefreshItems = vi.fn();
  const mockRefreshLoops = vi.fn();
  const mockClearSelection = vi.fn();

  const defaultOpts = {
    mode: 'item' as const,
    selectedWorkspace: 1,
    onRefreshItems: mockRefreshItems,
    onRefreshLoops: mockRefreshLoops,
    onClearSelection: mockClearSelection,
  };

  beforeEach(() => { vi.clearAllMocks(); });

  it('返回 batchActions 数组和 modals JSX', () => {
    const { result } = renderHook(() => useBatchActions(defaultOpts));
    expect(Array.isArray(result.current.batchActions)).toBe(true);
    expect(result.current.modals).toBeDefined();
  });

  it('item 模式下返回 5 个批量操作', () => {
    const { result } = renderHook(() => useBatchActions(defaultOpts));
    expect(result.current.batchActions).toHaveLength(5);
    expect(result.current.batchActions[0].key).toBe('change-executor');
  });

  it('loop 模式下返回 3 个批量操作', () => {
    const { result } = renderHook(() => useBatchActions({ ...defaultOpts, mode: 'loop' }));
    expect(result.current.batchActions).toHaveLength(3);
    expect(result.current.batchActions[2].key).toBe('force-stop');
  });

  it('item 模式下 modals 非空', () => {
    const { result } = renderHook(() => useBatchActions(defaultOpts));
    expect(result.current.modals).toBeTruthy();
  });

  it('item 模式的 action keys 完整', () => {
    const { result } = renderHook(() => useBatchActions(defaultOpts));
    const keys = result.current.batchActions.map(a => a.key);
    expect(keys).toEqual(['change-executor', 'copy-workspace', 'move-workspace', 'pause-scheduler', 'resume-scheduler']);
  });

  it('loop 模式的 action keys 完整', () => {
    const { result } = renderHook(() => useBatchActions({ ...defaultOpts, mode: 'loop' }));
    const keys = result.current.batchActions.map(a => a.key);
    expect(keys).toEqual(['copy-workspace', 'move-workspace', 'force-stop']);
  });

  it('workspace 为 null 时仍能正常构造', () => {
    const { result } = renderHook(() => useBatchActions({ ...defaultOpts, selectedWorkspace: null }));
    expect(result.current.batchActions).toHaveLength(5);
  });
});
