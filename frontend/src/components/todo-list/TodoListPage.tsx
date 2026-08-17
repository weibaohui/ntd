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
// 093：本组件只消费 todo 域状态，用细粒度 useTodos 替代合并版 useApp，
// 执行态（进度/统计推送）变化不再触发本组件重渲染。
import { useTodos } from '@/hooks/useTodoContext';
import { useIsMobile } from '@/hooks/useIsMobile';
// 109：列表形态直达路由——useViewState 提供 listView（URL ?view= 原文）与 replaceUrl。
import { useViewState, pickListView } from '@/hooks/useViewState';
import { TODO_LIST_REFRESH_EVENT } from '@/constants';
import { PageCard } from '@/components/common/PageCard';
import { TodoCenterCardView } from '@/components/TodoCenterCardView';
// RunningBoard：执行记录监控（6 列 + 实时 WS + 评审流水线 + 自带统计栏/刷新），running 视图复用。
import { RunningBoard } from '@/components/running-board';
import { TodoListView } from './TodoListView';
import {
  TodoListHeader,
  useTodoRowActions,
  ExecuteWithArgsModal,
} from './TodoListPageParts';
import type { TodoCenterItem } from '@/types';

/** localStorage 键：记住用户上次选的卡片/列表形态（URL 无 ?view= 参数时的兜底）。 */
const VIEW_STORAGE_KEY = 'ntd_items_view';

/** 读取持久化的视图模式，默认卡片（设计文档：默认卡片式事项中心）。 */
function readInitialView(): 'card' | 'list' | 'running' {
  try {
    const v = localStorage.getItem(VIEW_STORAGE_KEY);
    // 显式校验合法值：历史脏值（含已移除的 'kanban'）一律回退到默认卡片视图
    if (v === 'list' || v === 'running') return v;
    return 'card';
  } catch {
    return 'card';
  }
}

/** 056：搜索防抖毫秒数——输入停顿后再发请求，避免逐字符打服务端。 */
const SEARCH_DEBOUNCE_MS = 300;

/** 列表数据加载 hook（056 服务端分页版）：翻页/排序/搜索/时间窗变化时重新拉取对应页。 */
function useTodoListData(
  workspaceId: number | null,
  viewMode: 'card' | 'list' | 'running',
  searchKeyword: string,
  hours: number | null,
) {
  const [items, setItems] = useState<TodoCenterItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [total, setTotal] = useState(0);
  const [sortBy, setSortBy] = useState<string | undefined>(undefined);
  const [sortOrder, setSortOrder] = useState<'asc' | 'desc' | undefined>(undefined);
  // 搜索词防抖：rawSearchKeyword 即时更新，debouncedSearch 停顿后才变
  const [debouncedSearch, setDebouncedSearch] = useState('');

  // 搜索输入停顿 SEARCH_DEBOUNCE_MS 后才落盘到 debouncedSearch，并重回第 1 页
  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedSearch(searchKeyword.trim());
      setPage(1);
    }, SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [searchKeyword]);

  // 111：时间窗变化时回第 1 页——窗口收窄后旧页码可能超出有效页数，
  // 后端虽有截断兜底，但主动重置可避免用户看到「第 3 页但只有 1 条」的困惑。
  useEffect(() => {
    setPage(1);
  }, [hours]);

  // reload 用 useCallback 包裹，使 effect 依赖稳定
  const reload = useCallback(async () => {
    if (workspaceId == null) { setItems([]); setTotal(0); return; }
    setLoading(true);
    try {
      const data = await db.getTodoCenter(workspaceId, {
        page,
        pageSize,
        search: debouncedSearch || undefined,
        sortBy,
        sortOrder,
        // 111：时间窗下推服务端过滤——分页场景下前端过滤只作用于当前页会漏数据
        hours,
      });
      setItems(data.items);
      setTotal(data.total);
    } catch (e) {
      message.error(`加载事项列表失败：${e instanceof Error ? e.message : String(e)}`);
      setItems([]);
    } finally {
      setLoading(false);
    }
  }, [workspaceId, page, pageSize, debouncedSearch, sortBy, sortOrder, hours]);

  // 列表形态挂载/工作空间变化/分页参数变化时拉数据；卡片形态不触发（其内部自管）
  useEffect(() => {
    if (viewMode === 'list') reload();
  }, [viewMode, reload]);

  // 跨组件刷新：TodoDrawer 保存、QuickCapture 创建、WS 执行事件后重拉当前页
  useEffect(() => {
    const handler = () => { if (viewMode === 'list') reload(); };
    window.addEventListener(TODO_LIST_REFRESH_EVENT, handler);
    return () => window.removeEventListener(TODO_LIST_REFRESH_EVENT, handler);
  }, [viewMode, reload]);

  // 翻页/排序变化处理器（由 TodoListView 的 Table onChange 驱动）
  const handleServerChange = useCallback(
    (nextPage: number, nextPageSize: number, nextSortBy?: string, nextSortOrder?: 'asc' | 'desc') => {
      setPage(nextPageSize !== pageSize ? 1 : nextPage); // 改页大小回第 1 页
      setPageSize(nextPageSize);
      setSortBy(nextSortBy);
      setSortOrder(nextSortOrder);
    },
    [pageSize],
  );

  return {
    items, loading, reload,
    pagination: { current: page, pageSize, total },
    onServerChange: handleServerChange,
  };
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
  const { state } = useTodos();
  const workspaceId = state.selectedWorkspace;
  const isMobile = useIsMobile();
  // 109：listView 是 URL ?view= 原文（四个列表视图共用）；切换形态时用 replaceUrl 写回 URL。
  const { listView, replaceUrl } = useViewState();

  // 视图模式：URL ?view= 优先（直达指定形态），无参数/非法值回退 localStorage 记忆。
  // storedView 只在挂载时读一次：URL 变化走 listView 同步，localStorage 只作无参数兜底。
  // 注意 allowed 必须包含全部三种形态（含默认 card）：否则 localStorage 记忆为 list/running 时，
  // ?view=card 会被误判非法而无法强制卡片形态（review 修复）。
  const [storedView] = useState<'card' | 'list' | 'running'>(readInitialView);
  // 泛型版 pickListView 由 allowed/fallback 推导联合类型，无需 as 断言
  const viewMode = pickListView(listView, ['card', 'list', 'running'], storedView);
  // 统一搜索词：卡片/列表两种形态共用一个搜索框
  const [searchKeyword, setSearchKeyword] = useState('');
  // 111：时间窗（card/list 共享）：null=全部不过滤，与任务页口径一致；
  // 不持久化——离开页面回到默认「全部」，避免用户忘记过滤态导致老数据「消失」。
  const [hours, setHours] = useState<number | null>(null);
  // 刷新信号：每次点击刷新按钮自增，传递给 TodoCenterCardView 触发重新加载
  const [refreshKey, setRefreshKey] = useState(0);
  // 列表数据：056 服务端分页 hook（搜索词传入后内部防抖；时间窗变化自动重拉）
  const { items, loading, reload, pagination, onServerChange } =
    useTodoListData(workspaceId, viewMode, searchKeyword, hours);

  // 行操作 + 带参执行 Modal（已拆到 TodoListPageParts）
  const rowActions = useTodoRowActions({ workspaceId, onReload: reload });

  // 持久化视图模式：写 localStorage 兜底 + replaceUrl 同步 URL（?view=），
  // 使当前形态可分享/直达；replaceUrl 不膨胀浏览器历史栈（后退不逐次回退形态切换）。
  const handleViewChange = useCallback((m: 'card' | 'list' | 'running') => {
    try { localStorage.setItem(VIEW_STORAGE_KEY, m); } catch { /* 静默降级 */ }
    replaceUrl('todos', { view: m });
  }, [replaceUrl]);

  // 顶部刷新按钮：列表形态触发 reload，卡片形态刷新 key 驱动 TodoCenterCardView
  const handleReload = useCallback(() => {
    if (viewMode === 'list') reload();
    setRefreshKey(k => k + 1);
  }, [viewMode, reload]);

  // 根据 viewMode 渲染卡片/列表内容（056：搜索已下推服务端，不再页内过滤）
  const headerExtra = (
    <TodoListHeader
      isMobile={isMobile}
      viewMode={viewMode}
      searchKeyword={searchKeyword}
      loading={loading}
      hours={hours}
      onHoursChange={setHours}
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
          hours={hours}
          extra={headerExtra}
          refreshKey={refreshKey}
        />
      ) : viewMode === 'running' ? (
        // 执行监控态：复用 RunningBoard（执行记录 6 列 + 实时 WS + 评审流水线 + 自带统计栏/刷新）。
        // 不传 searchText/hours：RunningBoard 自带统计栏+刷新+实时，全量执行记录（与 card 的 todo 定义维度区分）。
        <PageCard
          icon={<UnorderedListOutlined />}
          title="事项"
          extra={headerExtra}
          style={{ flex: 1, height: '100%' }}
          contentStyle={{ height: 'calc(100% - 43px)', overflow: 'hidden' }}
        >
          <RunningBoard />
        </PageCard>
      ) : (
        <PageCard
          icon={<UnorderedListOutlined />}
          title="事项"
          extra={headerExtra}
          style={{ flex: 1, height: '100%' }}
          contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(100% - 43px)' }}
        >
          <TodoListView
            items={items}
            loading={loading}
            pagination={pagination}
            onServerChange={onServerChange}
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
