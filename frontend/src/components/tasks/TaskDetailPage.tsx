// TaskDetailPage — 任务详情独立路由（合并环路详情版）。
//
// 设计要点：
// 1. URL `/#/tasks/:id`，作为任务命名空间的详情态独立挂载。
// 2. 内部复用 `TaskDetailPanel` 组件，已合并环路详情全部内容（DAG/执行历史/执行看板）。
// 3. 顶部 PageCard 提供「返回列表」按钮 + 动态标题。
// 4. workspaceId 从当前选中的 workspace 获取，与任务列表页一致。

import { useState } from 'react';
import { Button } from 'antd';
import { ArrowLeftOutlined, OrderedListOutlined } from '@ant-design/icons';
import { useApp } from '@/hooks/useApp';
import { PageCard } from '@/components/common/PageCard';
import { TaskDetailPanel } from './TaskDetailPanel';

interface TaskDetailPageProps {
  taskId: number;
  onBack: () => void;
  /** 点击「来源工艺」面包屑跳转工艺详情。 */
  onOpenProcess?: (templateName: string) => void;
  /** 点击 DAG 节点上的事项标题跳转事项详情（legacy todo 系统）。 */
  onSelectTodo?: (todoId: number) => void;
  /** 环路状态变更后通知宿主。 */
  onLoopChanged?: () => void;
}

/**
 * 任务详情独立页：URL `/#/tasks/:id`。
 *
 * 整体处理思路：
 * 1. PageCard 包裹 TaskDetailPanel，标题动态显示任务名。
 * 2. 传递 onOpenProcess / onSelectTodo / onLoopChanged 给内部面板。
 * 3. 返回列表走 onBack（推荐 history.back()）保留列表状态。
 */
export function TaskDetailPage({
  taskId, onBack, onOpenProcess, onSelectTodo, onLoopChanged,
}: TaskDetailPageProps) {
  const { state } = useApp();
  const wsId = state.selectedWorkspace ?? 0;
  const [detailTitle, setDetailTitle] = useState<string>(`任务 #${taskId}`);

  return (
    <PageCard
      icon={<OrderedListOutlined />}
      title={detailTitle}
      titleSuffix={
        <Button size="small" type="text" icon={<ArrowLeftOutlined />} onClick={onBack}>
          返回列表
        </Button>
      }
      style={{ flex: 1, height: '100%' }}
      contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(100% - 43px)' }}
    >
      <TaskDetailPanel
        taskId={taskId}
        workspaceId={wsId}
        onTitleReady={(title) => setDetailTitle(`任务 #${taskId}: ${title}`)}
        onOpenProcess={onOpenProcess}
        onOpenTodo={onSelectTodo}
        onLoopChanged={onLoopChanged}
      />
    </PageCard>
  );
}
