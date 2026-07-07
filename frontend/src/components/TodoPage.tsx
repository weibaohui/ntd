import { UnorderedListOutlined, PlusOutlined } from '@ant-design/icons';
import { Button } from 'antd';
import { ListDetailPage } from './ListDetailPage';
import { TodoList } from './todo-list';
import { TodoDetail } from './TodoDetail';

interface TodoPageProps {
  selectedTodoId: string | number | null;
  onOpenCreateModal: () => void;
  onSelectTodo: (todoId: string | number | null) => void;
  loopUpdateCount: number;
  onSelectLoop: (loopId: number) => void;
  onCreateLoop: () => void;
  forcedListMode?: 'item' | 'loop';
  onListModeChange: () => void;
  effectiveMobilePanel: 'list' | 'detail';
  onOpenPost?: (todoId: number, recordId: number) => void;
}

/**
 * 桌面端事项页面组件
 * 使用 ListDetailPage 实现左侧列表 + 右侧详情的双栏布局
 * 移动端逻辑已独立到 TodoMobilePage 组件
 */
export function TodoPage({
  onOpenCreateModal,
  onSelectTodo,
  loopUpdateCount,
  onSelectLoop,
  onCreateLoop,
  forcedListMode,
  onListModeChange,
  selectedTodoId,
  onOpenPost,
}: TodoPageProps) {
  const listPanel = (
    <TodoList
      onOpenCreateModal={onOpenCreateModal}
      onSelectTodo={onSelectTodo}
      loopUpdateCount={loopUpdateCount}
      onSelectLoop={onSelectLoop}
      onCreateLoop={onCreateLoop}
      forcedListMode={forcedListMode}
      onListModeChange={onListModeChange}
      hideCreateButton={true}
    />
  );

  return (
    <ListDetailPage
      icon={<UnorderedListOutlined />}
      title="事项"
      storageKey="todo_page_sidebar_collapsed"
      extra={
        <Button
          type="primary"
          size="small"
          icon={<PlusOutlined />}
          onClick={onOpenCreateModal}
        >
          新建
        </Button>
      }
      listPanel={listPanel}
      detailPanel={selectedTodoId ? <TodoDetail onOpenPost={onOpenPost} /> : null}
    />
  );
}
