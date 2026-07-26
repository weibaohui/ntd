// LoopDetailPage — 028-列表详情独立路由：环路详情独立页。
//
// 设计要点（028-列表详情独立路由-设计 §4.6）：
// 1. URL `/#/loops/:id`，作为环路命名空间的详情态独立挂载。
// 2. 复用 `LoopDetailPanel`（已接收 loopId prop，与 state 解耦）。
// 3. 顶部 PageCard 提供「返回列表」按钮，用 history.back() 保留列表状态。
// 4. 触发/复制/删除/启停等操作直接调 dbLoops，完成后通过 onLoopChanged 通知父组件
//    刷新 LoopListPage（loopUpdateCount 递增触发其重拉）。

import type { ReactNode } from 'react';
import { Button, message } from 'antd';
import { ArrowLeftOutlined, RetweetOutlined } from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import type { UpdateLoopStatusRequest } from '@/types/loop';
import { PageCard } from '@/components/common/PageCard';
import { LoopDetailPanel } from './LoopStudioDetailPanel';

interface LoopDetailPageProps {
  /** 当前环路 id（来自 URL path 段 /#/loops/:id）。 */
  loopId: number;
  /** 当前工作空间 id（v1 路由必需）。 */
  workspaceId?: number | null;
  /** 全量标签集（详情面板渲染 Tag 用）。 */
  tags: Array<{ id: number; name: string; color: string }>;
  /** 返回列表：调用方用 history.back() 或 replaceUrl('loops')。 */
  onBack: () => void;
  /** 点击「来源工艺」跳转工艺详情；未注入时不可点击。 */
  onOpenProcess?: (templateName: string) => void;
  /** 点击流程图节点上的事项标题跳转事项详情。 */
  onSelectTodo?: (todoId: number) => void;
  /** 环路变更回调（删除/启停/触发后调），让父组件刷新 LoopListPage。 */
  onLoopChanged: () => void;
}

/**
 * 环路详情独立页：URL `/#/loops/:id`。
 *
 * 整体处理思路：
 * 1. PageCard 包裹 LoopDetailPanel，顶部标题 + 返回按钮。
 * 2. LoopDetailPanel 内部按 loopId 自己拉详情数据，本组件只负责容器与回调注入。
 * 3. 触发 / 复制 / 删除 / 启停等操作在 App.tsx 一层（原 LoopPage）已实现，
 *    这里改为本组件内部直接调 dbLoops，避免与 LoopListPage 形成循环依赖。
 */
export function LoopDetailPage({
  loopId,
  workspaceId,
  tags,
  onBack,
  onOpenProcess,
  onSelectTodo,
  onLoopChanged,
}: LoopDetailPageProps) {
  // 触发环路：调 API 提示成功，通知父组件刷新
  const handleTrigger = async () => {
    if (workspaceId == null) return;
    try {
      const res = await dbLoops.triggerLoop(workspaceId, loopId);
      message.success(`已触发 (execution #${res.execution_id})`);
      onLoopChanged();
    } catch (e) {
      message.error(`触发失败: ${e instanceof Error ? e.message : '未知错误'}`);
    }
  };

  // 复制环路：调 API 后提示，通知父组件刷新
  const handleDuplicate = async () => {
    if (workspaceId == null) return;
    try {
      await dbLoops.duplicateLoop(workspaceId, loopId);
      message.success('已复制');
      onLoopChanged();
    } catch (e) {
      message.error(`复制失败: ${e instanceof Error ? e.message : '未知错误'}`);
    }
  };

  // 删除环路：调 API 后返回列表
  const handleDelete = async () => {
    if (workspaceId == null) return;
    try {
      await dbLoops.deleteLoop(workspaceId, loopId);
      message.success('已删除');
      onLoopChanged();
      onBack();
    } catch {
      message.error('删除失败，环路可能正在被引用');
    }
  };

  // 切换启用/暂停状态
  const handleToggleStatus = async () => {
    if (workspaceId == null) return;
    try {
      const loops = await dbLoops.listLoops(workspaceId);
      const loop = loops.find(l => l.id === loopId);
      if (!loop) return;
      const next = loop.status === 'enabled' ? 'paused' : 'enabled';
      await dbLoops.updateLoopStatus(workspaceId, loopId, { status: next } as UpdateLoopStatusRequest);
      message.success(`已${next === 'enabled' ? '启用' : '暂停'}`);
      onLoopChanged();
    } catch (e) {
      message.error(`状态切换失败: ${e instanceof Error ? e.message : '未知错误'}`);
    }
  };

  // 标题 + 返回按钮
  const titleSuffix: ReactNode = (
    <Button
      size="small"
      type="text"
      icon={<ArrowLeftOutlined />}
      onClick={onBack}
    >
      返回列表
    </Button>
  );

  return (
    <PageCard
      icon={<RetweetOutlined />}
      title={`环路 #${loopId}`}
      titleSuffix={titleSuffix}
      style={{ flex: 1, height: '100%' }}
      contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(100% - 43px)' }}
    >
      <LoopDetailPanel
        loopId={loopId}
        workspaceId={workspaceId ?? null}
        tags={tags}
        onTrigger={handleTrigger}
        onDuplicate={handleDuplicate}
        onDelete={handleDelete}
        onToggleStatus={handleToggleStatus}
        onChanged={onLoopChanged}
        onOpenProcess={onOpenProcess}
        onOpenTodo={onSelectTodo}
      />
    </PageCard>
  );
}
