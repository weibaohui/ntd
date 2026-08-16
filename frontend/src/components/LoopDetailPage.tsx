// LoopDetailPage - 028-列表详情独立路由：环路详情独立页。
//
// 设计要点（028-列表详情独立路由-设计 §4.6）：
// 1. URL `/#/loops/:id`，作为环路命名空间的详情态独立挂载。
// 2. 复用 `LoopDetailPanel`（已接收 loopId prop，与 state 解耦）。
// 3. 顶部 PageCard 提供「返回列表」按钮（062 起统一在 extra 最右端）；当前调用方（App.tsx）
//    传入 backToList()，内部用 replaceUrl 回列表路由——详情页不产生历史条目，
//    浏览器后退键不会退回到已离开的详情页。
// 4. 删除/启停等操作由 useLoopDetailActions hook 提供（044：触发/复制/导出/编辑已下线），
//    完成后通过 onLoopChanged 通知父组件刷新 LoopListPage（loopUpdateCount 递增触发其重拉）。
// 5. 删除按钮上提到 PageCard extra（右上角）--
//    内层 hideTitleRow=true 隐藏标题行时按钮不会连带消失。LoopDetailPanel 通过 onActionsReady
//    上报按钮所需上下文，本组件存 state 后渲染到 extra。
// 6. 单函数 ≤ 30 行：操作回调已拆到 useLoopDetailActions。

import { useState, useCallback, useEffect } from 'react';
import type { ReactNode } from 'react';
import { Space } from 'antd';
import { RetweetOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { LoopDetailPanel } from './LoopStudioDetailPanel';
import { useLoopDetailActions } from './LoopDetailPageParts';
import { LoopDetailActions } from './LoopDetailActions';

interface LoopDetailPageProps {
  /** 当前环路 id（来自 URL path 段 /#/loops/:id）。 */
  loopId: number;
  /** 当前工作空间 id（v1 路由必需）。 */
  workspaceId?: number | null;
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
 * 1. PageCard 包裹 LoopDetailPanel，顶部标题（含环路名，062）+ 操作按钮 + 返回按钮（extra 右上角）。
 * 2. LoopDetailPanel 内部按 loopId 自己拉详情数据，本组件只负责容器与回调注入。
 * 3. 触发 / 复制 / 删除 / 启停等操作由 useLoopDetailActions hook 提供（已拆出），
 *    避免与 LoopListPage 形成循环依赖。
 * 4. 操作按钮上下文由 LoopDetailPanel 通过 onActionsReady 上报（含 detail + 导出/编辑内部
 *    handler），存 local state 后渲染到 extra，避免 hideTitleRow=true 时按钮连带消失。
 */
export function LoopDetailPage({
  loopId,
  workspaceId,
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

  // 删除后返回列表（覆盖 useLoopDetailActions 的 handleDelete，增加 onBack）。
  // useCallback 保证引用稳定，避免每次重渲染都创建新函数传给 LoopStudioDetailPanel 导致
  // onActionsReady 的 effect 循环触发（NTD-007：回调对象传递链中的引用稳定性）。
  const handleDeleteWithBack = useCallback(async () => {
    await handleDelete();
    onBack();
  }, [handleDelete, onBack]);

  // LoopDetailPanel 上报的就绪标志位：detail 加载完成前为 false，extra 不渲染操作按钮。
  // 用 boolean 而非对象状态，避免回传新对象引用触发父组件重渲染产生新 handleDeleteWithBack 再触发子
  // effect 的死循环（NTD-007）。
  const [actionsReady, setActionsReady] = useState(false);

  // 062：环路名称由 LoopDetailPanel 加载后上报，标题从「环路 #id」升级为「环路 #id: 名称」。
  // 同名时 setState 不触发重渲染（React 状态相同 bailout），无死循环风险。
  const [loopName, setLoopName] = useState<string | null>(null);
  const handleTitleReady = useCallback((name: string) => {
    setLoopName((prev) => (prev === name ? prev : name));
  }, []);

  // 切换环路或工作空间（组件不重挂载、仅 prop 变化）时重置名称，
  // 避免新环路 detail 未加载完成前标题短暂显示旧名称（与 TasksPage detailTitle 同款防御）。
  useEffect(() => {
    setLoopName(null);
  }, [loopId, workspaceId]);

  // 右上角：删除按钮（044：触发/复制/导出/编辑已下线），仅 detail 加载后可见
  const extra: ReactNode = actionsReady ? (
    <Space size={4}>
      <LoopDetailActions onDelete={handleDeleteWithBack} />
    </Space>
  ) : undefined;

  return (
    <PageCard
      icon={<RetweetOutlined />}
      title={loopName ? `环路 #${loopId}: ${loopName}` : `环路 #${loopId}`}
      extra={extra}
      // 062：返回按钮移交 PageCard 统一渲染（extra 最右端，删除按钮之后）
      onBack={onBack}
      style={{ flex: 1, height: '100%' }}
      contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(100% - 43px)' }}
    >
      <LoopDetailPanel
        loopId={loopId}
        workspaceId={workspaceId ?? null}
        // 外层 PageCard 已渲染标题行（标题 + 返回按钮 + 删除按钮），内层隐藏避免重复头部
        hideTitleRow
        onDelete={handleDeleteWithBack}
        onToggleStatus={handleToggleStatus}
        onChanged={onLoopChanged}
        onOpenProcess={onOpenProcess}
        onOpenTodo={onSelectTodo}
        onActionsReady={setActionsReady}
        onTitleReady={handleTitleReady}
      />
    </PageCard>
  );
}
