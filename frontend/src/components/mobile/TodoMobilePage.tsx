import type { ReactNode } from 'react';
import { UnorderedListOutlined, PlusOutlined, AppstoreOutlined } from '@ant-design/icons';
import { Button, Segmented } from 'antd';
import { PageCard } from '../common/PageCard';
import { TodoList } from '../todo-list';
import { TodoDetail } from '../TodoDetail';
import { EmptyDetailPlaceholder } from '../EmptyDetailPlaceholder';
import { SIDEBAR_WIDTH } from '@/constants';

interface TodoMobilePageProps {
  selectedTodoId: string | number | null;
  onOpenCreateModal: () => void;
  onSelectTodo: (todoId: string | number | null) => void;
  /** 刷新信号，点击刷新按钮时自增。 */
  refreshKey?: number;
  onSelectLoop: (loopId: number) => void;
  onCreateLoop: () => void;
  forcedListMode?: 'item' | 'loop';
  onListModeChange: () => void;
  effectiveMobilePanel: 'list' | 'detail';
  onOpenPost?: (todoId: number, recordId: number) => void;
  /** 当前视图模式（卡片/列表），由宿主 ItemsPage 持有；列表页 header 展示切换器回到卡片。 */
  viewMode: 'card' | 'list';
  onViewModeChange: (m: 'card' | 'list') => void;
  /** 统一搜索词，来自 ItemsPage 顶层搜索框。 */
  searchKeyword?: string;
  /** ItemsPage 构建的完整 header extra（桌面端使用，移动端传 null 由本组件自行构建）。 */
  extra?: ReactNode;
}

/**
 * 移动端事项页面组件（列表形态：双 PageCard 列表/详情切换）
 * 与 PC 端 ListDetailPage 一样，使用 PageCard 包裹详情页内容
 * 列表页和详情页各自有 PageCard 容器，详情页内 TodoDetail 渲染完整头部
 *
 * 合并后它是「事项」页移动端的列表形态；卡片形态由 ItemsPage 切到 TodoCenterCardView。
 */
export function TodoMobilePage({
  selectedTodoId,
  onOpenCreateModal,
  onSelectTodo,
  refreshKey,
  onSelectLoop,
  onCreateLoop,
  forcedListMode,
  onListModeChange,
  effectiveMobilePanel,
  onOpenPost,
  viewMode,
  onViewModeChange,
  searchKeyword,
  extra,
}: TodoMobilePageProps) {
  // 桌面端由 ItemsPage 传入完整 extra；移动端使用本组件自建的 header
  const listPageExtra = extra ?? (
    <>
      <Segmented
        size="small"
        value={viewMode}
        onChange={(v) => onViewModeChange(v as 'card' | 'list')}
        options={[
          { value: 'card', icon: <AppstoreOutlined />, title: '卡片视图' },
          { value: 'list', icon: <UnorderedListOutlined />, title: '列表' },
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
  );

  const listPage = (
    <PageCard
      icon={<UnorderedListOutlined />}
      title="事项"
      extra={listPageExtra}
      style={{ height: '100%' }}
      contentStyle={{ padding: 0, height: 'calc(100% - 43px)' }}
    >
      <TodoList
        onOpenCreateModal={onOpenCreateModal}
        onSelectTodo={onSelectTodo}
        refreshKey={refreshKey}
        onSelectLoop={onSelectLoop}
        onCreateLoop={onCreateLoop}
        forcedListMode={forcedListMode}
        onListModeChange={onListModeChange}
        hideCreateButton={true}
        searchKeyword={searchKeyword}
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
