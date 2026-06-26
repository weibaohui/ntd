import { UnorderedListOutlined, PlusOutlined } from '@ant-design/icons';
import { Button } from 'antd';
import { ListDetailPage } from './ListDetailPage';
import { TodoList } from './TodoList';
import { TodoDetail } from './TodoDetail';
import { EmptyDetailPlaceholder } from './EmptyDetailPlaceholder';
import { useIsMobile } from '@/hooks/useIsMobile';
import { SIDEBAR_WIDTH } from '@/constants';

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
}

export function TodoPage({
  selectedTodoId,
  onOpenCreateModal,
  onSelectTodo,
  loopUpdateCount,
  onSelectLoop,
  onCreateLoop,
  forcedListMode,
  onListModeChange,
  effectiveMobilePanel,
}: TodoPageProps) {
  const isMobile = useIsMobile();

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

  const detailPanel = selectedTodoId ? <TodoDetail /> : <EmptyDetailPlaceholder />;

  if (isMobile) {
    return (
      <>
        <div
          className={effectiveMobilePanel === 'list' ? 'animate-fade-in' : ''}
          style={{
            width: SIDEBAR_WIDTH.mobile,
            flexShrink: 0,
            height: '100%',
            display: effectiveMobilePanel === 'list' ? 'block' : 'none',
          }}
        >
          {listPanel}
        </div>
        <div
          className={effectiveMobilePanel === 'detail' ? 'animate-slide-in-right' : ''}
          style={{
            flex: 1,
            minWidth: 0,
            height: '100%',
            overflow: 'hidden',
            display: effectiveMobilePanel === 'detail' ? 'block' : 'none',
          }}
        >
          {detailPanel}
        </div>
      </>
    );
  }

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
      detailPanel={selectedTodoId ? <TodoDetail /> : null}
    />
  );
}
