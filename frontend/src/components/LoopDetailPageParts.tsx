// LoopDetailPageParts — LoopDetailPage 的拆分子模块（响应 028 PR review 的函数体 ≤30 行规范）。
//
// 拆分原则：把环路详情的操作回调（触发/复制/删除/启停）拆到 useLoopDetailActions hook，
// 让 LoopDetailPage 主函数仅负责组合，函数体保持简短。

import { useCallback } from 'react';
import { message } from 'antd';
import * as dbLoops from '@/utils/database/loops';
import type { UpdateLoopStatusRequest } from '@/types/loop';

interface UseLoopDetailActionsArgs {
  loopId: number;
  workspaceId: number | null;
  onLoopChanged: () => void;
}

/**
 * 环路详情操作：触发 / 复制 / 删除 / 启停状态切换。
 * 拆成 hook 让 LoopDetailPage 主函数保持简短，便于测试与复用。
 */
export function useLoopDetailActions({
  loopId,
  workspaceId,
  onLoopChanged,
}: UseLoopDetailActionsArgs) {
  const handleTrigger = useCallback(async () => {
    if (workspaceId == null) return;
    try {
      const res = await dbLoops.triggerLoop(workspaceId, loopId);
      message.success(`已触发 (execution #${res.execution_id})`);
      onLoopChanged();
    } catch (e) {
      message.error(`触发失败: ${e instanceof Error ? e.message : '未知错误'}`);
    }
  }, [workspaceId, loopId, onLoopChanged]);

  const handleDuplicate = useCallback(async () => {
    if (workspaceId == null) return;
    try {
      await dbLoops.duplicateLoop(workspaceId, loopId);
      message.success('已复制');
      onLoopChanged();
    } catch (e) {
      message.error(`复制失败: ${e instanceof Error ? e.message : '未知错误'}`);
    }
  }, [workspaceId, loopId, onLoopChanged]);

  const handleDelete = useCallback(async () => {
    if (workspaceId == null) return;
    try {
      await dbLoops.deleteLoop(workspaceId, loopId);
      message.success('已删除');
      onLoopChanged();
    } catch {
      message.error('删除失败，环路可能正在被引用');
    }
  }, [workspaceId, loopId, onLoopChanged]);

  const handleToggleStatus = useCallback(async () => {
    if (workspaceId == null) return;
    try {
      const loop = await dbLoops.getLoop(workspaceId, loopId);
      const next = loop.status === 'enabled' ? 'paused' : 'enabled';
      await dbLoops.updateLoopStatus(workspaceId, loopId, { status: next } as UpdateLoopStatusRequest);
      message.success(`已${next === 'enabled' ? '启用' : '暂停'}`);
      onLoopChanged();
    } catch (e) {
      message.error(`状态切换失败: ${e instanceof Error ? e.message : '未知错误'}`);
    }
  }, [workspaceId, loopId, onLoopChanged]);

  return { handleTrigger, handleDuplicate, handleDelete, handleToggleStatus };
}
