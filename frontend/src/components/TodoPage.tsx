import type { ReactNode } from 'react';
import { UnorderedListOutlined } from '@ant-design/icons';
import { ListDetailPage } from './ListDetailPage';
import { TodoList } from './todo-list';
import { TodoDetail } from './TodoDetail';

interface TodoPageProps {
  selectedTodoId: string | number | null;
  onOpenCreateModal: () => void;
  onSelectTodo: (todoId: string | number | null) => void;
  onSelectLoop: (loopId: number) => void;
  onCreateLoop: () => void;
  forcedListMode?: 'item' | 'loop';
  onListModeChange: () => void;
  onOpenPost?: (todoId: number, recordId: number) => void;
  /** 统一搜索词，来自 ItemsPage 顶层搜索框。 */
  searchKeyword?: string;
  /** ItemsPage 构建的完整 header extra（搜索框 + 刷新 + Segmented + 新建）。 */
  extra?: ReactNode;
  /** 刷新信号，来自 ItemsPage，点击刷新按钮时自增。 */
  refreshKey?: number;
}

/**
 * 桌面端事项页面组件（列表形态：双栏）
 * 使用 ListDetailPage 实现左侧列表 + 右侧详情的双栏布局，点左列表 → 右详情联动。
 * 移动端逻辑已独立到 TodoMobilePage 组件。
 *
 * 合并后它是「事项」页的列表形态；卡片形态由 ItemsPage 切到 TodoCenterCardView。
 */
export function TodoPage({
  selectedTodoId,
  onOpenCreateModal,
  onSelectTodo,
  onSelectLoop,
  onCreateLoop,
  forcedListMode,
  onListModeChange,
  onOpenPost,
  searchKeyword,
  extra,
  refreshKey,
}: TodoPageProps) {
  const listPanel = (
    <TodoList
      onOpenCreateModal={onOpenCreateModal}
      onSelectTodo={onSelectTodo}
      onSelectLoop={onSelectLoop}
      onCreateLoop={onCreateLoop}
      forcedListMode={forcedListMode}
      onListModeChange={onListModeChange}
      hideCreateButton={true}
      searchKeyword={searchKeyword}
      refreshKey={refreshKey}
    />
  );

  return (
    <ListDetailPage
      icon={<UnorderedListOutlined />}
      title="事项"
      storageKey="todo_page_sidebar_collapsed"
      extra={extra}
      listPanel={listPanel}
      detailPanel={selectedTodoId ? <TodoDetail onOpenPost={onOpenPost} /> : null}
    />
  );
}
