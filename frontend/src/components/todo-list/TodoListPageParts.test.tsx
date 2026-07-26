// useTodoRowActions 单元测试。
// 验证 hook 返回的回调函数行为：调 API → 弹消息 → onReload。
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { message } from 'antd';
import * as db from '@/utils/database';

// mock antd message（用法与项目中一致：message.success / message.error）
vi.mock('antd', () => {
  const mockMessage = { success: vi.fn(), error: vi.fn(), warning: vi.fn() };
  return {
    message: mockMessage,
    Modal: { confirm: vi.fn() },
  };
});

// mock 数据库调用
vi.mock('@/utils/database', () => ({
  executeTodo: vi.fn(),
}));

import { useTodoRowActions } from './TodoListPageParts';
import type { TodoCenterItem } from '@/types';

describe('useTodoRowActions', () => {
  const mockReload = vi.fn();
  const mockTodo: TodoCenterItem = {
    id: 42,
    title: '测试事项',
    status: 'pending',
    executor: 'claude',
  } as TodoCenterItem;

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('handleExecuteTodo', () => {
    it('workspace 为空时静默返回', async () => {
      const { result } = renderHook(() => useTodoRowActions({ workspaceId: null, onReload: mockReload }));
      await act(() => result.current.handleExecuteTodo(mockTodo));
      expect(db.executeTodo).not.toHaveBeenCalled();
    });

    it('调用 executeTodo 成功后弹成功消息并刷新', async () => {
      vi.mocked(db.executeTodo).mockResolvedValueOnce({ id: 1 } as never);
      const { result } = renderHook(() => useTodoRowActions({ workspaceId: 1, onReload: mockReload }));
      await act(() => result.current.handleExecuteTodo(mockTodo));
      expect(db.executeTodo).toHaveBeenCalledWith(1, 42, 'claude');
      expect(message.success).toHaveBeenCalledWith(expect.stringContaining('执行'));
      expect(mockReload).toHaveBeenCalledOnce();
    });

    it('executeTodo 失败时弹错误消息', async () => {
      vi.mocked(db.executeTodo).mockRejectedValueOnce(new Error('网络错误'));
      const { result } = renderHook(() => useTodoRowActions({ workspaceId: 1, onReload: mockReload }));
      await act(() => result.current.handleExecuteTodo(mockTodo));
      expect(message.error).toHaveBeenCalledWith(expect.stringContaining('网络错误'));
    });

    it('未配置 executor 时不传 executor 参数', async () => {
      vi.mocked(db.executeTodo).mockResolvedValueOnce({ id: 2 } as never);
      const noExecTodo = { ...mockTodo, executor: undefined };
      const { result } = renderHook(() => useTodoRowActions({ workspaceId: 1, onReload: mockReload }));
      await act(() => result.current.handleExecuteTodo(noExecTodo));
      expect(db.executeTodo).toHaveBeenCalledWith(1, 42, undefined);
    });
  });
});
