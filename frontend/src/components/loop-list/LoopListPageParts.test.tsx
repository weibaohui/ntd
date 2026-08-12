// useLoopRowActions / useLoopConfig 单元测试。
// 验证 hook 返回的回调函数行为：调 API → 弹消息 → onReload。
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { message, Modal } from 'antd';
import * as dbLoops from '@/utils/database/loops';
import * as todos from '@/utils/database/todos';

vi.mock('antd', () => {
  const mockMessage = { success: vi.fn(), error: vi.fn(), warning: vi.fn() };
  return { message: mockMessage, Modal: { confirm: vi.fn() } };
});

vi.mock('@/utils/database/loops', () => ({
  deleteLoop: vi.fn(),
  updateLoopStatus: vi.fn(),
}));

vi.mock('@/utils/database/todos', () => ({
  getProjectDirectories: vi.fn(),
}));

import { useLoopRowActions, useLoopConfig } from './LoopListPageParts';
import type { LoopListItem } from '@/types/loop';

describe('useLoopRowActions', () => {
  const mockReload = vi.fn();
  const mockOnLoopChanged = vi.fn();
  const mockLoop: LoopListItem = { id: 10, name: '测试环路', status: 'enabled' } as LoopListItem;

  beforeEach(() => { vi.clearAllMocks(); });

  describe('handleDelete', () => {
    it('workspace 为空时静默返回', async () => {
      const { result } = renderHook(() => useLoopRowActions({ workspaceId: null, onReload: mockReload }));
      await act(() => result.current.handleDelete(mockLoop));
      expect(dbLoops.deleteLoop).not.toHaveBeenCalled();
    });

    it('删除成功后弹消息、刷新、通知变化', async () => {
      vi.mocked(dbLoops.deleteLoop).mockResolvedValueOnce(undefined as never);
      // NTD-014-C：删除走 Modal.confirm 二次确认，测试中自动点「确定」放行 onOk；
      // 返回 antd 要求的 { destroy, update } 形状，避免 TS 报错。
      vi.mocked(Modal.confirm).mockImplementationOnce((cfg) => {
        void cfg.onOk?.();
        return { destroy: vi.fn(), update: vi.fn() };
      });
      const { result } = renderHook(() => useLoopRowActions({ workspaceId: 1, onReload: mockReload, onLoopChanged: mockOnLoopChanged }));
      await act(() => result.current.handleDelete(mockLoop));
      expect(dbLoops.deleteLoop).toHaveBeenCalledWith(1, 10);
      expect(message.success).toHaveBeenCalledWith(expect.stringContaining('删除'));
      expect(mockReload).toHaveBeenCalledOnce();
      expect(mockOnLoopChanged).toHaveBeenCalledOnce();
    });

    it('删除失败时弹错误消息', async () => {
      vi.mocked(dbLoops.deleteLoop).mockRejectedValueOnce(new Error('引用冲突'));
      // 确认框放行后 onOk 内的 deleteLoop 抛错，应弹错误提示。
      // 注意：onOk 失败会 re-throw（保持对话框打开），测试 mock 里需接住，避免未处理 rejection。
      vi.mocked(Modal.confirm).mockImplementationOnce((cfg) => {
        void cfg.onOk?.().catch(() => {});
        return { destroy: vi.fn(), update: vi.fn() };
      });
      const { result } = renderHook(() => useLoopRowActions({ workspaceId: 1, onReload: mockReload }));
      await act(() => result.current.handleDelete(mockLoop));
      expect(message.error).toHaveBeenCalled();
    });
  });

  describe('handleToggleStatus', () => {
    it('workspace 为空时静默返回', async () => {
      const { result } = renderHook(() => useLoopRowActions({ workspaceId: null, onReload: mockReload }));
      await act(() => result.current.handleToggleStatus(mockLoop));
      expect(dbLoops.updateLoopStatus).not.toHaveBeenCalled();
    });

    it('enabled → paused 切换', async () => {
      vi.mocked(dbLoops.updateLoopStatus).mockResolvedValueOnce(undefined as never);
      const { result } = renderHook(() => useLoopRowActions({ workspaceId: 1, onReload: mockReload }));
      await act(() => result.current.handleToggleStatus(mockLoop));
      expect(dbLoops.updateLoopStatus).toHaveBeenCalledWith(1, 10, { status: 'paused' });
      expect(message.success).toHaveBeenCalledWith(expect.stringContaining('暂停'));
      expect(mockReload).toHaveBeenCalledOnce();
    });

    it('paused → enabled 切换', async () => {
      vi.mocked(dbLoops.updateLoopStatus).mockResolvedValueOnce(undefined as never);
      const pausedLoop = { ...mockLoop, status: 'paused' };
      const { result } = renderHook(() => useLoopRowActions({ workspaceId: 1, onReload: mockReload }));
      await act(() => result.current.handleToggleStatus(pausedLoop));
      expect(dbLoops.updateLoopStatus).toHaveBeenCalledWith(1, 10, { status: 'enabled' });
      expect(message.success).toHaveBeenCalledWith(expect.stringContaining('启用'));
    });
  });
});

describe('useLoopConfig', () => {
  beforeEach(() => { vi.clearAllMocks(); });

  it('初始状态：未打开、无 currentWorkspace', () => {
    const { result } = renderHook(() => useLoopConfig({ workspaceId: 1 }));
    expect(result.current.loopConfigOpen).toBe(false);
    expect(result.current.currentWorkspace).toBeNull();
  });

  it('workspace 为空时 open 不执行', async () => {
    const { result } = renderHook(() => useLoopConfig({ workspaceId: null }));
    await act(() => result.current.handleOpenLoopConfig());
    expect(todos.getProjectDirectories).not.toHaveBeenCalled();
  });

  it('打开配置页：拉取目录并找到对应工作空间', async () => {
    vi.mocked(todos.getProjectDirectories).mockResolvedValueOnce([
      { id: 1, name: '空间A' },
      { id: 2, name: '空间B' },
    ] as never);
    const { result } = renderHook(() => useLoopConfig({ workspaceId: 2 }));
    await act(() => result.current.handleOpenLoopConfig());
    expect(result.current.loopConfigOpen).toBe(true);
    expect(result.current.currentWorkspace).toEqual({ id: 2, name: '空间B' });
  });

  it('未找到工作空间时弹警告', async () => {
    vi.mocked(todos.getProjectDirectories).mockResolvedValueOnce([
      { id: 1, name: '空间A' },
    ] as never);
    const { result } = renderHook(() => useLoopConfig({ workspaceId: 99 }));
    await act(() => result.current.handleOpenLoopConfig());
    expect(result.current.loopConfigOpen).toBe(false);
    expect(message.warning).toHaveBeenCalledWith('未找到当前工作空间');
  });

  it('close 关闭配置页并清空 currentWorkspace', () => {
    const { result } = renderHook(() => useLoopConfig({ workspaceId: 1 }));
    act(() => result.current.handleCloseLoopConfig());
    expect(result.current.loopConfigOpen).toBe(false);
    expect(result.current.currentWorkspace).toBeNull();
  });
});
