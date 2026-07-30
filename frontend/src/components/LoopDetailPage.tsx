// LoopDetailPage - 028-列表详情独立路由：环路详情独立页。
//
// 设计要点（028-列表详情独立路由-设计 §4.6）：
// 1. URL `/#/loops/:id`，作为环路命名空间的详情态独立挂载。
// 2. 复用 `LoopDetailPanel`（已接收 loopId prop，与 state 解耦）。
// 3. 顶部 PageCard 提供「返回列表」按钮（紧贴标题），用 history.back() 保留列表状态。
// 4. 删除/启停等操作由 useLoopDetailActions hook 提供（044：触发/复制/导出/编辑已下线），
//    完成后通过 onLoopChanged 通知父组件刷新 LoopListPage（loopUpdateCount 递增触发其重拉）。
// 5. 删除按钮上提到 PageCard extra（右上角）--
//    内层 hideTitleRow=true 隐藏标题行时按钮不会连带消失。LoopDetailPanel 通过 onActionsReady
//    上报按钮所需上下文，本组件存 state 后渲染到 extra。
// 6. 单函数 ≤ 30 行：操作回调已拆到 useLoopDetailActions。

import { useState } from 'react';
import type { ReactNode } from 'react';
import { Button, Space } from 'antd';
import { ArrowLeftOutlined, RetweetOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { LoopDetailPanel } from './LoopStudioDetailPanel';
import { useLoopDetailActions } from './LoopDetailPageParts';
import { LoopDetailActions } from './LoopDetailActions';
import type { LoopDetailActionsProps } from './LoopDetailActions';

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
 * 1. PageCard 包裹 LoopDetailPanel，顶部标题 + 返回按钮（titleSuffix）+ 操作按钮（extra 右上角）。
 * 2. LoopDetailPanel 内部按 loopId 自己拉详情数据，本组件只负责容器与回调注入。
 * 3. 触发 / 复制 / 删除 / 启停等操作由 useLoopDetailActions hook 提供（已拆出），
 *    避免与 LoopListPage 形成循环依赖。
 * 4. 操作按钮上下文由 LoopDetailPanel 通过 onActionsReady 上报（含 detail + 导出/编辑内部
 *    handler），存 local state 后渲染到 extra，避免 hideTitleRow=true 时按钮连带消失。
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
  // 操作回调（已拆到 useLoopDetailActions；044 后只剩删除与启停）
  const { handleDelete, handleToggleStatus } = useLoopDetailActions({
    loopId,
    workspaceId: workspaceId ?? null,
    onLoopChanged,
  });

  // 删除后返回列表（覆盖 useLoopDetailActions 的 handleDelete，增加 onBack）
  const handleDeleteWithBack = async () => {
    await handleDelete();
    onBack();
  };

  // LoopDetailPanel 上报的按钮上下文；detail 加载完成前为 null，extra 不渲染操作按钮。
  const [actionsCtx, setActionsCtx] = useState<LoopDetailActionsProps | null>(null);

  // 右上角：删除按钮（044：触发/复制/导出/编辑已下线），仅 detail 加载后可见
  const extra: ReactNode = actionsCtx ? (
    <Space size={4}>
      <LoopDetailActions onDelete={handleDeleteWithBack} />
    </Space>
  ) : undefined;

  // 标题右侧：返回列表按钮
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
      extra={extra}
      style={{ flex: 1, height: '100%' }}
      contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(100% - 43px)' }}
    >
      <LoopDetailPanel
        loopId={loopId}
        workspaceId={workspaceId ?? null}
        tags={tags}
        // 外层 PageCard 已渲染标题行（标题 + 返回按钮 + 删除按钮），内层隐藏避免重复头部
        hideTitleRow
        onDelete={handleDeleteWithBack}
        onToggleStatus={handleToggleStatus}
        onChanged={onLoopChanged}
        onOpenProcess={onOpenProcess}
        onOpenTodo={onSelectTodo}
        onActionsReady={setActionsCtx}
      />
    </PageCard>
  );
}
