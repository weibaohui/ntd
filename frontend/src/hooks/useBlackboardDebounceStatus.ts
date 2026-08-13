/**
 * useBlackboardDebounceStatus — 黑板防抖状态订阅 hook（096-W4-4 产物）。
 *
 * 原 BlackboardPage 中 MobileDebounceIndicator 与 BlackboardDebounceBar 各持有一份
 * 逐字相同的 `blackboardDebounceStatus` 事件订阅（addEventListener + workspace_id
 * 过滤 + setStatus + 卸载清理，约 9 行 ×2）。收敛为单份：任何 UI 想消费防抖状态，
 * 直接调用本 hook，不再复制订阅样板。
 *
 * 事件源：`useExecutionEvents` 把后端 WS 事件桥接为 window CustomEvent。
 */

import { useEffect, useState } from 'react';
import type { BlackboardDebounceStatus } from '@/hooks/useExecutionEvents';

/**
 * 订阅指定工作空间的黑板防抖状态。
 * 仅接收 `workspace_id === workspaceId` 的事件；无事件时为 null（调用方通常据此不渲染）。
 */
export function useBlackboardDebounceStatus(workspaceId: number): BlackboardDebounceStatus | null {
  const [status, setStatus] = useState<BlackboardDebounceStatus | null>(null);

  useEffect(() => {
    const handler = (e: Event) => {
      const s = (e as CustomEvent<BlackboardDebounceStatus>).detail;
      // 只关心当前工作空间的事件——多工作空间并存时避免串台
      if (s.workspace_id !== workspaceId) return;
      setStatus(s);
    };
    window.addEventListener('blackboardDebounceStatus', handler);
    return () => window.removeEventListener('blackboardDebounceStatus', handler);
  }, [workspaceId]);

  return status;
}
