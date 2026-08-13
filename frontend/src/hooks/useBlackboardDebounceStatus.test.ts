import { describe, it, expect } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useBlackboardDebounceStatus } from './useBlackboardDebounceStatus';
import type { BlackboardDebounceStatus } from '@/hooks/useExecutionEvents';

/** 派发一个 blackboardDebounceStatus CustomEvent（模拟 useExecutionEvents 的桥接输出） */
function fireStatus(detail: Partial<BlackboardDebounceStatus> & { workspace_id: number }) {
  const full = {
    pending_count: 0,
    threshold: 10,
    debounce_secs: 600,
    remaining_secs: -1,
    refreshing: false,
    ...detail,
  } as BlackboardDebounceStatus;
  window.dispatchEvent(new CustomEvent('blackboardDebounceStatus', { detail: full }));
}

describe('useBlackboardDebounceStatus', () => {
  it('接收匹配 workspace 的事件并更新状态', () => {
    const { result } = renderHook(() => useBlackboardDebounceStatus(1));
    expect(result.current).toBeNull();

    act(() => {
      fireStatus({ workspace_id: 1, pending_count: 5, threshold: 10 });
    });
    expect(result.current?.pending_count).toBe(5);
  });

  it('过滤其他 workspace 的事件（多空间并存不串台）', () => {
    const { result } = renderHook(() => useBlackboardDebounceStatus(1));
    act(() => {
      fireStatus({ workspace_id: 2, pending_count: 9 });
    });
    expect(result.current).toBeNull();
  });

  it('workspaceId 变化时重新订阅（旧空间事件不再接收）', () => {
    const { result, rerender } = renderHook(({ id }) => useBlackboardDebounceStatus(id), {
      initialProps: { id: 1 },
    });
    act(() => {
      fireStatus({ workspace_id: 1, pending_count: 3 });
    });
    expect(result.current?.pending_count).toBe(3);

    rerender({ id: 2 });
    // 状态保留最后一次值（hook 不主动清空——与原实现行为一致）；
    // 但新事件只认 workspace 2
    act(() => {
      fireStatus({ workspace_id: 1, pending_count: 99 });
    });
    expect(result.current?.pending_count).toBe(3);
    act(() => {
      fireStatus({ workspace_id: 2, pending_count: 7 });
    });
    expect(result.current?.pending_count).toBe(7);
  });

  it('卸载时移除监听（不再接收事件）', () => {
    const { result, unmount } = renderHook(() => useBlackboardDebounceStatus(1));
    unmount();
    act(() => {
      fireStatus({ workspace_id: 1, pending_count: 42 });
    });
    // 卸载后 setStatus 不被调用：状态停留在卸载前的 null
    expect(result.current).toBeNull();
  });
});
