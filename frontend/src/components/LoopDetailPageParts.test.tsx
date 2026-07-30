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
  deleteLoop: vi.fn(),
  getLoop: vi.fn(),
  updateLoopStatus: vi.fn(),
}));

import { useLoopDetailActions } from './LoopDetailPageParts';

describe('useLoopDetailActions', () => {
  const mockOnLoopChanged = vi.fn();

  beforeEach(() => { vi.clearAllMocks(); });

  // 044：触发/复制已随手工环路能力下线，仅保留删除与启停的测试

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
