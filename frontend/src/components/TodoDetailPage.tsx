// TodoDetailPage - 028-列表详情独立路由：事项详情独立页。
//
// 设计要点（028-列表详情独立路由-设计 §4.5）：
// 1. URL `/#/todos/:id`，作为事项命名空间的详情态独立挂载。
// 2. 内部仍复用 `TodoDetail` 组件（执行历史 / 所属环路 / 编辑等），不重写详情逻辑；
//    App.tsx 已在 useEffect 中根据 todoDetailId 派发 SELECT_TODO 同步 state.selectedTodoId，
//    TodoDetail 内部读 state.selectedTodoId 即可拿到当前 todoId。
// 3. 顶部 PageCard 提供「返回列表」按钮（062 起统一在 extra 最右端）；当前调用方（App.tsx）
//    传入 backToList()，内部用 replaceUrl 回列表路由——详情页不产生历史条目，
//    浏览器后退键不会退回到已离开的详情页。
// 4. 操作按钮（优化标题/编辑/删除）上提到 PageCard extra（右上角）--
//    内层 hideTitleRow=true 隐藏标题行时按钮不会连带消失。TodoDetail 通过 onActionsReady
//    上报按钮所需上下文（todo + 回调），本组件存 state 后渲染到 extra。
// 5. 不存在时显示 Empty 引导 + 返回按钮（TodoDetail 内部已有空态，这里不重复实现）。

import { useState } from 'react';
import type { ReactNode } from 'react';
import { Space } from 'antd';
import { UnorderedListOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { TodoDetail } from './TodoDetail';
import { TodoDetailActions } from './todo-detail/TodoDetailActions';
import type { TodoDetailActionsProps } from './todo-detail/TodoDetailActions';

interface TodoDetailPageProps {
  /** 当前事项 id（来自 URL path 段 /#/todos/:id）。 */
  todoId: number;
  /** 返回列表：调用方用 history.back() 或 replaceUrl('todos')。 */
  onBack: () => void;
  /** 打开帖子页（点击执行记录时触发），跳转到 /#/todos/:id/posts/:recordId。 */
  onOpenPost: (todoId: number, recordId: number) => void;
}

/**
 * 事项详情独立页：URL `/#/todos/:id`。
 *
 * 整体处理思路：
 * 1. PageCard 包裹 TodoDetail，顶部标题 + 操作按钮 + 返回按钮（extra 右上角，返回在最右，062）。
 * 2. TodoDetail 不改 prop 签名，仍读 state.selectedTodoId（App.tsx 已同步）。
 * 3. 操作按钮上下文由 TodoDetail 通过 onActionsReady 上报，存 local state 后渲染到 extra，
 *    避免 TodoDetail 内部 hideTitleRow=true 时按钮连带标题一起消失。
 */
export function TodoDetailPage({ todoId, onBack, onOpenPost }: TodoDetailPageProps) {
  // TodoDetail 上报的按钮上下文；selectedTodo 加载完成前为 null，extra 不渲染操作按钮。
  // 同时从 actionsCtx.todo.title 获取实际标题，动态更新 PageCard 标题（详情标题功能）。
  const [actionsCtx, setActionsCtx] = useState<TodoDetailActionsProps | null>(null);

  // 详情标题：数据加载后显示实际事项标题；未加载时显示 "事项 #X"。
  const detailTitle = actionsCtx?.todo.title
    ? `事项 #${todoId}: ${actionsCtx.todo.title}`
    : `事项 #${todoId}`;

  // 右上角：操作按钮组（优化标题/编辑/删除），仅 selectedTodo 加载后可见
  const extra: ReactNode = actionsCtx ? (
    <Space size={4}>
      <TodoDetailActions
        todo={actionsCtx.todo}
        onDelete={actionsCtx.onDelete}
        onEdit={actionsCtx.onEdit}
        onTitleUpdate={actionsCtx.onTitleUpdate}
      />
    </Space>
  ) : undefined;

  return (
    <PageCard
      icon={<UnorderedListOutlined />}
      title={detailTitle}
      extra={extra}
      // 062：返回按钮移交 PageCard 统一渲染（extra 最右端，操作按钮之后）
      onBack={onBack}
      style={{ flex: 1, height: '100%' }}
      contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(100% - 43px)' }}
    >
      {/* 外层 PageCard 已渲染标题行（标题 + 返回按钮 + 操作按钮），内层隐藏避免重复头部 */}
      <TodoDetail hideTitleRow onOpenPost={onOpenPost} onActionsReady={setActionsCtx} />
    </PageCard>
  );
}
