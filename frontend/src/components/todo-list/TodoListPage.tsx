// TodoListPage — 028-列表详情独立路由：事项列表页容器（替代原 ItemsPage）。
//
// 设计要点（028-列表详情独立路由-设计 §4.1、§6.1）：
// 1. 替代原 ItemsPage，承担卡片/列表形态切换；事项详情已独立到 TodoDetailPage（URL: /#/todos/:id）。
// 2. viewMode='card'（默认）→ 渲染 TodoCenterCardView（五类驱动卡片墙）。
// 3. viewMode='list' → 渲染 PageCard + TodoListView（Ant Design Table 单栏宽屏）。
// 4. 点击卡片或 table 行 → 调用 onSelectTodo 由父组件 pushUrl('todos', { id }) 跳转独立详情页。
// 5. 顶部 header：搜索框 + 刷新 + Segmented + 新建按钮。
// 6. 单行操作由 useTodoRowActions hook 提供，避免主函数膨胀。
// 7. 监听 TODO_LIST_REFRESH_EVENT 跨组件刷新（TodoDrawer 保存后触发）。
// 8. 单函数 ≤ 30 行：数据获取/行操作/内容渲染已拆到子模块。

import { useEffect, useState, useCallback } from 'react';
import { message } from 'antd';
import { UnorderedListOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';
import { useApp } from '@/hooks/useApp';
import { useIsMobile } from '@/hooks/useIsMobile';
import { TODO_LIST_REFRESH_EVENT } from '@/constants';
import { PageCard } from '@/components/common/PageCard';
import { TodoCenterCardView } from '@/components/TodoCenterCardView';
import { TodoListView } from './TodoListView';
import {
  TodoListHeader,
  useTodoRowActions,
  ExecuteWithArgsModal,
} from './TodoListPageParts';
import type { TodoCenterItem } from '@/types';

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

/** 按搜索词过滤：标题或 prompt 命中关键字（不区分大小写）。 */
function filterBySearchKeyword(items: TodoCenterItem[], keyword: string): TodoCenterItem[] {
  const kw = keyword.trim().toLowerCase();
  if (!kw) return items;
  return items.filter(todo => {
    const title = (todo.title || '').toLowerCase();
    const prompt = (todo.prompt || '').toLowerCase();
    return title.includes(kw) || prompt.includes(kw);
  });
}

/** 列表数据加载 hook：响应 workspace/视图切换 + 跨组件刷新事件。 */
function useTodoListData(workspaceId: number | null, viewMode: 'card' | 'list') {
  const [items, setItems] = useState<TodoCenterItem[]>([]);
  const [loading, setLoading] = useState(false);

  // reload 用 useCallback 包裹，使 effect 依赖稳定
  const reload = useCallback(async () => {
    if (workspaceId == null) { setItems([]); return; }
    setLoading(true);
    try {
      const data = await db.getTodoCenter(workspaceId);
      setItems(data);
    } catch (e) {
      message.error(`加载事项列表失败：${e instanceof Error ? e.message : String(e)}`);
      setItems([]);
    } finally {
      setLoading(false);
    }
  }, [workspaceId]);

  // 列表形态挂载/工作空间变化时拉数据；卡片形态不触发（其内部自管）
  useEffect(() => {
    if (viewMode === 'list') reload();
  }, [workspaceId, viewMode, reload]);

  // 跨组件刷新：TodoDrawer 保存、QuickCapture 创建后通过 custom event 通知
  useEffect(() => {
    const handler = () => { if (viewMode === 'list') reload(); };
    window.addEventListener(TODO_LIST_REFRESH_EVENT, handler);
    return () => window.removeEventListener(TODO_LIST_REFRESH_EVENT, handler);
  }, [viewMode, reload]);

  return { items, loading, reload };
}

interface TodoListPageProps {
  /** 选中事项：跳转到 /#/todos/:id。 */
  onSelectTodo: (id: number) => void;
  /** 点击所属 Loop 跳转 Loop 详情（卡片形态用）。 */
  onSelectLoop: (loopId: number) => void;
  /** 新建事项入口（复用全局 TodoDrawer）。 */
  onOpenCreateModal: () => void;
  /** 编辑事项入口（单行菜单「编辑」触发，由 App.tsx 打开 TodoDrawer 编辑模式）。 */
  onEditTodo: (todo: TodoCenterItem) => void;
}

/**
 * 事项列表页：URL `/#/todos`。
 * 主函数仅负责组合（视图切换 + 数据拉取 + 透传拆分子模块），保持函数体简短。
 */
export function TodoListPage({
  onSelectTodo,
  onSelectLoop,
  onOpenCreateModal,
  onEditTodo,
}: TodoListPageProps) {
  const { state } = useApp();
  const workspaceId = state.selectedWorkspace;
  const isMobile = useIsMobile();

  // 视图模式：默认卡片，用户切到列表后下次仍记住
  const [viewMode, setViewMode] = useState<'card' | 'list'>(readInitialView);
  // 统一搜索词：卡片/列表两种形态共用一个搜索框
  const [searchKeyword, setSearchKeyword] = useState('');
  // 刷新信号：每次点击刷新按钮自增，传递给 TodoCenterCardView 触发重新加载
  const [refreshKey, setRefreshKey] = useState(0);
  // 列表数据：抽到独立 hook 管理加载/刷新/事件监听
  const { items, loading, reload } = useTodoListData(workspaceId, viewMode);

  // 行操作 + 带参执行 Modal（已拆到 TodoListPageParts）
  const rowActions = useTodoRowActions({ workspaceId, onReload: reload });

  // 持久化视图模式
  const handleViewChange = useCallback((m: 'card' | 'list') => {
    setViewMode(m);
    try { localStorage.setItem(VIEW_STORAGE_KEY, m); } catch { /* 静默降级 */ }
  }, []);

  // 顶部刷新按钮：列表形态触发 reload，卡片形态刷新 key 驱动 TodoCenterCardView
  const handleReload = useCallback(() => {
    if (viewMode === 'list') reload();
    setRefreshKey(k => k + 1);
  }, [viewMode, reload]);

  // 根据 viewMode 渲染卡片/列表内容
  const listItems = filterBySearchKeyword(items, searchKeyword);
  const headerExtra = (
    <TodoListHeader
      isMobile={isMobile}
      viewMode={viewMode}
      searchKeyword={searchKeyword}
      loading={loading}
      onSearchChange={setSearchKeyword}
      onViewChange={handleViewChange}
      onReload={handleReload}
      onCreate={onOpenCreateModal}
    />
  );

  return (
    <>
      {viewMode === 'card' ? (
        <TodoCenterCardView
          onSelectTodo={onSelectTodo}
          onSelectLoop={onSelectLoop}
          isMobile={isMobile}
          searchKeyword={searchKeyword}
          extra={headerExtra}
          refreshKey={refreshKey}
        />
      ) : (
        <PageCard
          icon={<UnorderedListOutlined />}
          title="事项"
          extra={headerExtra}
          style={{ flex: 1, height: '100%' }}
          contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(100% - 43px)' }}
        >
          <TodoListView
            items={listItems}
            loading={loading}
            tags={state.tags}
            onSelectTodo={onSelectTodo}
            onEditTodo={onEditTodo}
            onDeleteTodo={rowActions.handleDeleteTodo}
            onExecuteTodo={rowActions.handleExecuteTodo}
            onExecuteWithArgs={rowActions.handleExecuteWithArgs}
            onRefresh={reload}
          />
        </PageCard>
      )}
      <ExecuteWithArgsModal
        open={rowActions.executeWithArgsModalOpen}
        args={rowActions.executeArgs}
        onArgsChange={rowActions.setExecuteArgs}
        onOk={rowActions.confirmExecuteWithArgs}
        onCancel={rowActions.cancelExecuteWithArgs}
      />
    </>
  );
}
