// LoopDetailPage — 028-列表详情独立路由：环路详情独立页。
//
// 设计要点（028-列表详情独立路由-设计 §4.6）：
// 1. URL `/#/loops/:id`，作为环路命名空间的详情态独立挂载。
// 2. 复用 `LoopDetailPanel`（已接收 loopId prop，与 state 解耦）。
// 3. 顶部 PageCard 提供「返回列表」按钮，用 history.back() 保留列表状态。
// 4. 触发/复制/删除/启停等操作由 useLoopDetailActions hook 提供，
//    完成后通过 onLoopChanged 通知父组件刷新 LoopListPage（loopUpdateCount 递增触发其重拉）。
// 5. 单函数 ≤ 30 行：操作回调已拆到 useLoopDetailActions。

import type { ReactNode } from 'react';
import { Button } from 'antd';
import { ArrowLeftOutlined, RetweetOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { LoopDetailPanel } from './LoopStudioDetailPanel';
import { useLoopDetailActions } from './LoopDetailPageParts';

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
 * 3. 触发 / 复制 / 删除 / 启停等操作由 useLoopDetailActions hook 提供（已拆出），
 *    避免与 LoopListPage 形成循环依赖。
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
  // 操作回调（已拆到 useLoopDetailActions）
  const { handleTrigger, handleDuplicate, handleDelete, handleToggleStatus } = useLoopDetailActions({
    loopId,
    workspaceId: workspaceId ?? null,
    onLoopChanged,
  });

  // 删除后返回列表（覆盖 useLoopDetailActions 的 handleDelete，增加 onBack）
  const handleDeleteWithBack = async () => {
    await handleDelete();
    onBack();
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
        // 外层 PageCard 已渲染标题行（标题 + 返回按钮），内层隐藏避免重复头部
        hideTitleRow
        onTrigger={handleTrigger}
        onDuplicate={handleDuplicate}
        onDelete={handleDeleteWithBack}
        onToggleStatus={handleToggleStatus}
        onChanged={onLoopChanged}
        onOpenProcess={onOpenProcess}
        onOpenTodo={onSelectTodo}
      />
    </PageCard>
  );
}