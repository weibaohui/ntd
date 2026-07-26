// TodoDetailPage — 028-列表详情独立路由：事项详情独立页。
//
// 设计要点（028-列表详情独立路由-设计 §4.5）：
// 1. URL `/#/todos/:id`，作为事项命名空间的详情态独立挂载。
// 2. 内部仍复用 `TodoDetail` 组件（执行历史 / 所属环路 / 编辑等），不重写详情逻辑；
//    App.tsx 已在 useEffect 中根据 todoDetailId 派发 SELECT_TODO 同步 state.selectedTodoId，
//    TodoDetail 内部读 state.selectedTodoId 即可拿到当前 todoId。
// 3. 顶部 PageCard 提供「返回列表」按钮，使用 history.back() 让浏览器原生后退保留列表状态
//    （搜索词 / 分页 / 选中行）。
// 4. 不存在时显示 Empty 引导 + 返回按钮（TodoDetail 内部已有空态，这里不重复实现）。

import type { ReactNode } from 'react';
import { Button } from 'antd';
import { ArrowLeftOutlined, UnorderedListOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { TodoDetail } from './TodoDetail';

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
 * 1. PageCard 包裹 TodoDetail，顶部标题 + 返回按钮。
 * 2. TodoDetail 不改 prop 签名，仍读 state.selectedTodoId（App.tsx 已同步）。
 * 3. 顶部「返回列表」走 onBack（推荐 history.back()）保留列表状态。
 */
export function TodoDetailPage({ todoId, onBack, onOpenPost }: TodoDetailPageProps) {
  // 标题 + 返回按钮：放在 titleSuffix 让返回按钮紧贴标题，符合用户预期
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
      icon={<UnorderedListOutlined />}
      title={`事项 #${todoId}`}
      titleSuffix={titleSuffix}
      style={{ flex: 1, height: '100%' }}
      contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(100% - 43px)' }}
    >
      {/* 外层 PageCard 已渲染标题行（标题 + 返回按钮），内层隐藏避免重复头部 */}
      <TodoDetail hideTitleRow onOpenPost={onOpenPost} />
    </PageCard>
  );
}
