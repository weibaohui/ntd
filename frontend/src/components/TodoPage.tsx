import { UnorderedListOutlined, PlusOutlined, AppstoreOutlined } from '@ant-design/icons';
import { Button, Segmented } from 'antd';
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
  /** 当前视图模式（卡片/列表），由宿主 ItemsPage 持有；列表页 header 展示切换器回到卡片。 */
  viewMode: 'card' | 'list';
  onViewModeChange: (m: 'card' | 'list') => void;
}

/**
 * 桌面端事项页面组件（列表形态：双栏）
 * 使用 ListDetailPage 实现左侧列表 + 右侧详情的双栏布局，点左列表 → 右详情联动。
 * 移动端逻辑已独立到 TodoMobilePage 组件。
 *
 * 合并后它是「事项」页的列表形态；卡片形态由 ItemsPage 切到 TodoCenterCardView。
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
  viewMode,
  onViewModeChange,
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
        <>
          {/* 卡片/列表切换：切回卡片墙，由宿主 ItemsPage 控制 */}
          <Segmented
            size="small"
            value={viewMode}
            onChange={(v) => onViewModeChange(v as 'card' | 'list')}
            options={[
              { value: 'card', icon: <AppstoreOutlined />, title: '卡片视图' },
              { value: 'list', icon: <UnorderedListOutlined />, title: '列表（双栏）' },
            ]}
            data-testid="todo-center-view-toggle"
          />
          <Button
            type="primary"
            size="small"
            icon={<PlusOutlined />}
            onClick={onOpenCreateModal}
          >
            新建
          </Button>
        </>
      }
      listPanel={listPanel}
      detailPanel={selectedTodoId ? <TodoDetail onOpenPost={onOpenPost} /> : null}
    />
  );
}
