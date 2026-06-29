import { UnorderedListOutlined, PlusOutlined } from '@ant-design/icons';
import { Button } from 'antd';
import { PageCard } from '../common/PageCard';
import { TodoList } from '../TodoList';
import { TodoDetail } from '../TodoDetail';
import { EmptyDetailPlaceholder } from '../EmptyDetailPlaceholder';
import { SIDEBAR_WIDTH } from '@/constants';

interface TodoMobilePageProps {
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
 * 移动端事项页面组件 —— 与桌面端统一设计
 * 与 PC 端 ListDetailPage 一样，使用 PageCard 包裹详情页内容
 * 列表页和详情页各自有 PageCard 容器，详情页内 TodoDetail 渲染完整头部
 */
export function TodoMobilePage({
  selectedTodoId,
  onOpenCreateModal,
  onSelectTodo,
  loopUpdateCount,
  onSelectLoop,
  onCreateLoop,
  forcedListMode,
  onListModeChange,
  effectiveMobilePanel,
  onOpenPost,
}: TodoMobilePageProps) {
  const listPage = (
    <PageCard
      icon={<UnorderedListOutlined />}
      title="事项"
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
      style={{ height: '100%' }}
      contentStyle={{ padding: 0, height: 'calc(100% - 43px)' }}
    >
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
    </PageCard>
  );

  const detailPage = selectedTodoId ? (
    <PageCard
      icon={<UnorderedListOutlined />}
      title="事项"
      style={{ height: '100%', flex: 1, minWidth: 0 }}
      contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(100% - 43px)', overflow: 'hidden' }}
    >
      <TodoDetail onOpenPost={onOpenPost} />
    </PageCard>
  ) : (
    <PageCard
      icon={<UnorderedListOutlined />}
      title="事项"
      style={{ height: '100%', flex: 1, minWidth: 0 }}
      contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(100% - 43px)', overflow: 'hidden' }}
    >
      <EmptyDetailPlaceholder />
    </PageCard>
  );

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
        {listPage}
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
        {detailPage}
      </div>
    </>
  );
}
