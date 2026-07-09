import { useState } from 'react';
import { Button, Input, Segmented } from 'antd';
import { AppstoreOutlined, PlusOutlined, ReloadOutlined, SearchOutlined, UnorderedListOutlined } from '@ant-design/icons';
import { TodoCenterCardView } from './TodoCenterCardView';
import { TodoPage } from './TodoPage';
import { TodoMobilePage } from './mobile/TodoMobilePage';

/** localStorage 键：记住用户上次选的卡片/列表形态。 */
const VIEW_STORAGE_KEY = 'ntd_items_view';

/** 读取持久化的视图模式，默认卡片（设计文档：默认卡片式事项中心）。 */
function readInitialView(): 'card' | 'list' {
  try {
    const v = localStorage.getItem(VIEW_STORAGE_KEY);
    return v === 'list' ? 'list' : 'card';
  } catch {
    return 'card';
  }
}

interface ItemsPageProps {
  /** 选中事项（设置 selectedTodoId + URL detail）。列表与卡片两种形态共用。 */
  onSelectTodo: (todoId: string | number | null) => void;
  /** 点击所属 Loop 跳转 Loop 详情（卡片形态用）。 */
  onSelectLoop: (loopId: number) => void;
  /** 新建事项入口（复用全局 TodoDrawer）。 */
  onOpenCreateModal: () => void;
  // —— 以下为列表形态（TodoPage）所需 props ——
  selectedTodoId: string | number | null;
  loopUpdateCount: number;
  onCreateLoop: () => void;
  forcedListMode?: 'item' | 'loop';
  onListModeChange: () => void;
  effectiveMobilePanel: 'list' | 'detail';
  onOpenPost?: (todoId: number, recordId: number) => void;
  /** 移动端：列表形态切到 TodoMobilePage（双 PageCard），卡片形态走卡片墙单列。 */
  isMobile?: boolean;
  /** 刷新回调，由 App 层注入，用于 ItemsPage header 的刷新按钮。 */
  onReload?: () => void;
}

/**
 * 「事项」页合并宿主：把原「事项列表（双栏）」与「事项中心（卡片墙）」合成一个入口。
 *
 * - 卡片形态（默认）：五类驱动卡片墙（TodoCenterCardView）。
 * - 列表形态：原 TodoPage 双栏（左列表 + 右详情，点左 → 右联动）。
 *
 * 点卡片 → 选中该事项并切到列表形态，右栏打开其详情（含执行历史）。
 * 列表形态内部的左右联动由 TodoPage 自身保证，切换形态不影响联动。
 *
 * 搜索框统一放在 ItemsPage 顶层（卡片/列表两种形态共用一个搜索组件），
 * 通过 searchKeyword 传递给子组件，由子组件负责在自己的数据上做过滤。
 */
export function ItemsPage({
  onSelectTodo,
  onSelectLoop,
  onOpenCreateModal,
  selectedTodoId,
  loopUpdateCount,
  onCreateLoop,
  forcedListMode,
  onListModeChange,
  effectiveMobilePanel,
  onOpenPost,
  isMobile,
  onReload,
}: ItemsPageProps) {
  // 视图模式持久化：用户切到列表后下次仍记住，默认卡片
  const [viewMode, setViewMode] = useState<'card' | 'list'>(readInitialView);
  // 统一搜索词：卡片墙与列表双栏共用同一个搜索框，搜索逻辑由各自子组件在其数据上执行
  const [searchKeyword, setSearchKeyword] = useState('');

  const persistView = (m: 'card' | 'list') => {
    setViewMode(m);
    try {
      localStorage.setItem(VIEW_STORAGE_KEY, m);
    } catch {
      /* localStorage 不可用时静默降级，不影响切换 */
    }
  };

  // 点卡片：选中事项（右栏详情数据源）+ 切到列表形态让详情显示。
  // 列表形态点左列表其它事项仍照常联动右栏——卡片只是另一个「选事项」入口。
  const selectTodoAndSwitchToList = (id: number) => {
    onSelectTodo(id);
    persistView('list');
  };

  // 搜索框：桌面端统一放在 ItemsPage 顶层，移动端由子组件自行决定是否显示
  const searchInput = !isMobile ? (
    <Input
      allowClear
      size="small"
      placeholder="搜索标题或 Prompt"
      prefix={<SearchOutlined />}
      value={searchKeyword}
      onChange={(e) => setSearchKeyword(e.target.value)}
      style={{ width: 200 }}
      data-testid="items-page-search"
    />
  ) : null;

  // 刷新按钮：桌面端统一放在 ItemsPage 顶层，列表模式的子组件 TodoPage 没有自己的刷新按钮
  const reloadButton = !isMobile && onReload ? (
    <Button size="small" icon={<ReloadOutlined />} onClick={onReload} aria-label="刷新">
      刷新
    </Button>
  ) : null;

  // 完整的 header 元素：搜索框 + 刷新 + Segmented 切换 + 新建（桌面端，移动端由子组件自行处理）
  const extra = !isMobile ? (
    <>
      {searchInput}
      {reloadButton}
      <Segmented
        size="small"
        value={viewMode}
        onChange={(v) => persistView(v as 'card' | 'list')}
        options={[
          { value: 'card', icon: <AppstoreOutlined />, title: '卡片视图' },
          { value: 'list', icon: <UnorderedListOutlined />, title: '列表（双栏）' },
        ]}
        data-testid="todo-center-view-toggle"
      />
      <Button size="small" type="primary" icon={<PlusOutlined />} onClick={onOpenCreateModal}>
        新建
      </Button>
    </>
  ) : null;

  if (viewMode === 'card') {
    return (
      <TodoCenterCardView
        onSelectTodo={selectTodoAndSwitchToList}
        onSelectLoop={onSelectLoop}
        isMobile={isMobile}
        searchKeyword={searchKeyword}
        extra={extra}
        loopUpdateCount={loopUpdateCount}
      />
    );
  }

  // 列表形态：移动端走 TodoMobilePage（双 PageCard），桌面端走 TodoPage（双栏）
  if (isMobile) {
    return (
      <TodoMobilePage
        selectedTodoId={selectedTodoId}
        onOpenCreateModal={onOpenCreateModal}
        onSelectTodo={onSelectTodo}
        loopUpdateCount={loopUpdateCount}
        onSelectLoop={onSelectLoop}
        onCreateLoop={onCreateLoop}
        forcedListMode={forcedListMode}
        onListModeChange={onListModeChange}
        effectiveMobilePanel={effectiveMobilePanel}
        onOpenPost={onOpenPost}
        viewMode={viewMode}
        onViewModeChange={persistView}
        searchKeyword={searchKeyword}
        extra={extra}
      />
    );
  }

  return (
    <TodoPage
      selectedTodoId={selectedTodoId}
      onOpenCreateModal={onOpenCreateModal}
      onSelectTodo={onSelectTodo}
      loopUpdateCount={loopUpdateCount}
      onSelectLoop={onSelectLoop}
      onCreateLoop={onCreateLoop}
      forcedListMode={forcedListMode}
      onListModeChange={onListModeChange}
      onOpenPost={onOpenPost}
      searchKeyword={searchKeyword}
      extra={extra}
    />
  );
}
