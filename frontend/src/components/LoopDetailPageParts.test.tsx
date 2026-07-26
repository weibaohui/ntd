// useLoopDetailActions 单元测试。
// 验证 hook 返回的回调函数行为：调 API → 弹消息 → onLoopChanged。
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { message } from 'antd';
import * as dbLoops from '@/utils/database/loops';

vi.mock('antd', () => {
  const mockMessage = { success: vi.fn(), error: vi.fn() };
  return { message: mockMessage, Modal: { confirm: vi.fn() } };
});

vi.mock('@/utils/database/loops', () => ({
  triggerLoop: vi.fn(),
  duplicateLoop: vi.fn(),
  deleteLoop: vi.fn(),
  getLoop: vi.fn(),
  updateLoopStatus: vi.fn(),
}));

import { useLoopDetailActions } from './LoopDetailPageParts';

describe('useLoopDetailActions', () => {
  const mockOnLoopChanged = vi.fn();

  beforeEach(() => { vi.clearAllMocks(); });

  describe('handleTrigger', () => {
    it('workspace 为空时静默返回', async () => {
      const { result } = renderHook(() => useLoopDetailActions({ loopId: 5, workspaceId: null, onLoopChanged: mockOnLoopChanged }));
      await act(() => result.current.handleTrigger());
      expect(dbLoops.triggerLoop).not.toHaveBeenCalled();
    });

    it('触发成功后弹消息并回调', async () => {
      vi.mocked(dbLoops.triggerLoop).mockResolvedValueOnce({ execution_id: 88 } as never);
      const { result } = renderHook(() => useLoopDetailActions({ loopId: 5, workspaceId: 1, onLoopChanged: mockOnLoopChanged }));
      await act(() => result.current.handleTrigger());
      expect(dbLoops.triggerLoop).toHaveBeenCalledWith(1, 5);
      expect(message.success).toHaveBeenCalledWith(expect.stringContaining('#88'));
      expect(mockOnLoopChanged).toHaveBeenCalledOnce();
    });
  });

  describe('handleDuplicate', () => {
    it('workspace 为空时静默返回', async () => {
      const { result } = renderHook(() => useLoopDetailActions({ loopId: 5, workspaceId: null, onLoopChanged: mockOnLoopChanged }));
      await act(() => result.current.handleDuplicate());
      expect(dbLoops.duplicateLoop).not.toHaveBeenCalled();
    });

    it('复制成功后弹消息并回调', async () => {
      vi.mocked(dbLoops.duplicateLoop).mockResolvedValueOnce(undefined as never);
      const { result } = renderHook(() => useLoopDetailActions({ loopId: 5, workspaceId: 1, onLoopChanged: mockOnLoopChanged }));
      await act(() => result.current.handleDuplicate());
      expect(dbLoops.duplicateLoop).toHaveBeenCalledWith(1, 5);
      expect(message.success).toHaveBeenCalledWith(expect.stringContaining('复制'));
      expect(mockOnLoopChanged).toHaveBeenCalledOnce();
    });
  });

  describe('handleDelete', () => {
    it('删除成功后弹消息并回调', async () => {
      vi.mocked(dbLoops.deleteLoop).mockResolvedValueOnce(undefined as never);
      const { result } = renderHook(() => useLoopDetailActions({ loopId: 5, workspaceId: 1, onLoopChanged: mockOnLoopChanged }));
      await act(() => result.current.handleDelete());
      expect(dbLoops.deleteLoop).toHaveBeenCalledWith(1, 5);
      expect(message.success).toHaveBeenCalledWith(expect.stringContaining('删除'));
      expect(mockOnLoopChanged).toHaveBeenCalledOnce();
    });
  });

  describe('handleToggleStatus', () => {
    it('enabled → paused', async () => {
      vi.mocked(dbLoops.getLoop).mockResolvedValueOnce({ status: 'enabled' } as never);
      vi.mocked(dbLoops.updateLoopStatus).mockResolvedValueOnce(undefined as never);
      const { result } = renderHook(() => useLoopDetailActions({ loopId: 5, workspaceId: 1, onLoopChanged: mockOnLoopChanged }));
      await act(() => result.current.handleToggleStatus());
      expect(dbLoops.getLoop).toHaveBeenCalledWith(1, 5);
      expect(dbLoops.updateLoopStatus).toHaveBeenCalledWith(1, 5, { status: 'paused' });
      expect(message.success).toHaveBeenCalledWith(expect.stringContaining('暂停'));
      expect(mockOnLoopChanged).toHaveBeenCalledOnce();
    });

    it('paused → enabled', async () => {
      vi.mocked(dbLoops.getLoop).mockResolvedValueOnce({ status: 'paused' } as never);
      vi.mocked(dbLoops.updateLoopStatus).mockResolvedValueOnce(undefined as never);
      const { result } = renderHook(() => useLoopDetailActions({ loopId: 5, workspaceId: 1, onLoopChanged: mockOnLoopChanged }));
      await act(() => result.current.handleToggleStatus());
      expect(dbLoops.updateLoopStatus).toHaveBeenCalledWith(1, 5, { status: 'enabled' });
      expect(message.success).toHaveBeenCalledWith(expect.stringContaining('启用'));
    });
  });
});
