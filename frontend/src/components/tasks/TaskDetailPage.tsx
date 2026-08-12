// TaskDetailPage — 任务详情独立路由（合并环路详情版）。
//
// 设计要点：
// 1. URL `/#/tasks/:id`，作为任务命名空间的详情态独立挂载。
// 2. 内部复用 `TaskDetailPanel` 组件，已合并环路详情全部内容（DAG/执行历史）。
// 3. 顶部 PageCard 提供「返回列表」按钮（062 起统一在 extra 最右端）+ 动态标题。
// 4. workspaceId 从当前选中的 workspace 获取，与任务列表页一致。

import { useState } from 'react';
import { OrderedListOutlined } from '@ant-design/icons';
// 093：本组件只消费 todo 域状态，用细粒度 useTodos 替代合并版 useApp，
// 执行态（进度/统计推送）变化不再触发本组件重渲染。
import { useTodos } from '@/hooks/useTodoContext';
import { PageCard } from '@/components/common/PageCard';
import { TaskDetailPanel } from './TaskDetailPanel';

interface TaskDetailPageProps {
  taskId: number;
  onBack: () => void;
  /** 点击 DAG 节点上的事项标题跳转事项详情（legacy todo 系统）。 */
  onSelectTodo?: (todoId: number) => void;
  /** 任务删除成功后由宿主跳回任务列表（NTD-014-A）。 */
  onDeleted?: () => void;
}

/**
 * 任务详情独立页：URL `/#/tasks/:id`。
 *
 * 整体处理思路：
 * 1. PageCard 包裹 TaskDetailPanel，标题动态显示任务名。
 * 2. 传递 onSelectTodo / onDeleted 给内部面板。
 * 3. 返回列表走 onBack（推荐 history.back()）保留列表状态。
 */
export function TaskDetailPage({
  taskId, onBack, onSelectTodo, onDeleted,
}: TaskDetailPageProps) {
  const { state } = useTodos();
  const wsId = state.selectedWorkspace ?? 0;
  const [detailTitle, setDetailTitle] = useState<string>(`任务 #${taskId}`);

  return (
    <PageCard
      icon={<OrderedListOutlined />}
      title={detailTitle}
      // 062：返回按钮移交 PageCard 统一渲染（extra 最右端）
      onBack={onBack}
      style={{ flex: 1, height: '100%' }}
      contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(100% - 43px)' }}
    >
      <TaskDetailPanel
        taskId={taskId}
        workspaceId={wsId}
        onTitleReady={(title) => setDetailTitle(`任务 #${taskId}: ${title}`)}
        onOpenTodo={onSelectTodo}
        onDeleted={onDeleted}
      />
    </PageCard>
  );
}
