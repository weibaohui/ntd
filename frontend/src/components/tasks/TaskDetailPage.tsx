// TaskDetailPage — 031-任务详情独立路由：任务详情独立页。
//
// 设计要点：
// 1. URL `/#/tasks/:id`，作为任务命名空间的详情态独立挂载。
// 2. 内部复用 `TaskDetailPanel` 组件（步骤/执行历史等），不重写详情逻辑。
// 3. 顶部 PageCard 提供「返回列表」按钮，使用 history.back() 让浏览器原生后退保留列表状态。
// 4. workspaceId 从当前选中的 workspace 获取，与任务列表页一致。

import { useState } from 'react';
import { Button } from 'antd';
import { ArrowLeftOutlined, OrderedListOutlined } from '@ant-design/icons';
import { useApp } from '@/hooks/useApp';
import { PageCard } from '@/components/common/PageCard';
import { TaskDetailPanel } from './TaskDetailPanel';

interface TaskDetailPageProps {
  /** 当前任务 id（来自 URL path 段 /#/tasks/:id）。 */
  taskId: number;
  /** 返回列表：调用方用 history.back() 或 replaceUrl('tasks')。 */
  onBack: () => void;
}

/**
 * 任务详情独立页：URL `/#/tasks/:id`。
 *
 * 整体处理思路：
 * 1. PageCard 包裹 TaskDetailPanel，顶部标题 + 返回按钮。
 * 2. workspaceId 从 App state 的 selectedWorkspace 获取。
 * 3. 返回列表走 onBack（推荐 history.back()）保留列表状态。
 */
export function TaskDetailPage({ taskId, onBack }: TaskDetailPageProps) {
  const { state } = useApp();
  const wsId = state.selectedWorkspace ?? 0;
  // 详情标题：数据加载后显示实际任务标题；未加载时显示 "任务 #X"。
  const [detailTitle, setDetailTitle] = useState<string>(`任务 #${taskId}`);

  return (
    <PageCard
      icon={<OrderedListOutlined />}
      title={detailTitle}
      titleSuffix={
        <Button
          size="small"
          type="text"
          icon={<ArrowLeftOutlined />}
          onClick={onBack}
        >
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
      />
    </PageCard>
  );
}
