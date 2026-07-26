// TodoListPage — 028-列表详情独立路由：事项列表页容器（替代原 ItemsPage）。
//
// 设计要点（028-列表详情独立路由-设计 §4.1、§6.1）：
// 1. 替代原 ItemsPage，承担卡片/列表形态切换；事项详情已独立到 TodoDetailPage（URL: /#/todos/:id）。
// 2. viewMode='card'（默认）→ 渲染 TodoCenterCardView（五类驱动卡片墙）。
// 3. viewMode='list' → 渲染 PageCard + TodoListView（Ant Design Table 单栏宽屏）。
// 4. 点击卡片或 table 行 → 调用 onSelectTodo 由父组件 pushUrl('todos', { id }) 跳转独立详情页。
// 5. 顶部 header 由本组件构建：搜索框 + 刷新 + Segmented + 新建（桌面/移动端共用）。
// 6. 单行操作（执行/带参执行/编辑/删除）由本组件内部实现，避免父组件 App.tsx 持有过多业务逻辑。
// 7. 监听 TODO_LIST_REFRESH_EVENT 跨组件刷新（TodoDrawer 保存后触发）。
// 8. 单函数 ≤ 30 行：数据拉取、过滤、行操作、header 构建拆为独立函数。

import { useCallback, useEffect, useMemo, useState } from 'react';
import { Button, Input, Modal, Segmented, message } from 'antd';
import {
  AppstoreOutlined,
  PlusOutlined,
  ReloadOutlined,
  SearchOutlined,
  ThunderboltOutlined,
  UnorderedListOutlined,
} from '@ant-design/icons';
import type { ReactNode } from 'react';
import * as db from '@/utils/database';
import { useApp } from '@/hooks/useApp';
import { useIsMobile } from '@/hooks/useIsMobile';
import { TODO_LIST_REFRESH_EVENT } from '@/constants';
import { PageCard } from '@/components/common/PageCard';
import { TodoCenterCardView } from '@/components/TodoCenterCardView';
import { TodoListView } from './TodoListView';
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

interface TodoListPageProps {
  /** 选中事项：跳转到 /#/todos/:id（由父组件 pushUrl('todos', { id })）。 */
  onSelectTodo: (id: number) => void;
  /** 点击所属 Loop 跳转 Loop 详情（卡片形态用）。 */
  onSelectLoop: (loopId: number) => void;
  /** 新建事项入口（顶部「新建」按钮，复用全局 TodoDrawer）。 */
  onOpenCreateModal: () => void;
  /** 编辑事项入口（单行菜单「编辑」触发，由 App.tsx 打开 TodoDrawer 编辑模式）。 */
  onEditTodo: (todo: TodoCenterItem) => void;
}

/**
 * 事项列表页：URL `/#/todos`。
 *
 * 整体处理思路：
 * 1. 内部维护 viewMode（card/list），持久化到 localStorage。
 * 2. viewMode='card' → 渲染 TodoCenterCardView（已自带 PageCard）。
 * 3. viewMode='list' → 渲染 PageCard + TodoListView（table 形态）。
 * 4. 顶部 header 由本组件构建，下发给子组件（卡片/列表共用）。
 * 5. 列表形态的行操作（执行/带参/删除）在本组件内实现，避免 App.tsx 持有过多业务逻辑。
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

  // 视图模式：默认卡片（设计文档），用户切到列表后下次仍记住
  const [viewMode, setViewMode] = useState<'card' | 'list'>(readInitialView);
  // 列表形态的数据源（卡片形态由 TodoCenterCardView 内部自管）
  const [items, setItems] = useState<TodoCenterItem[]>([]);
  const [loading, setLoading] = useState(false);
  // 统一搜索词：卡片/列表两种形态共用一个搜索框
  const [searchKeyword, setSearchKeyword] = useState('');
  // 刷新信号：每次点击刷新按钮自增，传递给 TodoCenterCardView 触发重新加载
  const [refreshKey, setRefreshKey] = useState(0);

  // 持久化视图模式到 localStorage，下次进入仍记住
  const persistView = useCallback((m: 'card' | 'list') => {
    setViewMode(m);
    try {
      localStorage.setItem(VIEW_STORAGE_KEY, m);
    } catch {
      /* localStorage 不可用时静默降级，不影响切换 */
    }
  }, []);

  // 拉取列表数据：仅 list 形态需要（card 形态由 TodoCenterCardView 自管）
  // 用 getTodoCenter（而非 getAllTodos）是因为它已聚合 last_execution_status 等列字段
  const reload = useCallback(async () => {
    if (workspaceId == null) {
      setItems([]);
      return;
    }
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
    if (viewMode === 'list') {
      reload();
    }
  }, [reload, viewMode]);

  // 跨组件刷新：TodoDrawer 保存、QuickCapture 创建后通过 custom event 通知本组件重拉
  useEffect(() => {
    const handler = () => {
      if (viewMode === 'list') reload();
    };
    window.addEventListener(TODO_LIST_REFRESH_EVENT, handler);
    return () => window.removeEventListener(TODO_LIST_REFRESH_EVENT, handler);
  }, [reload, viewMode]);

  // 按搜索词过滤：标题或 prompt 命中关键字（不区分大小写）
  const filteredItems = useMemo(() => {
    const kw = searchKeyword.trim().toLowerCase();
    if (!kw) return items;
    return items.filter(todo => {
      const title = (todo.title || '').toLowerCase();
      const prompt = (todo.prompt || '').toLowerCase();
      return title.includes(kw) || prompt.includes(kw);
    });
  }, [items, searchKeyword]);

  // 单行执行：调 executeTodo 后提示 + 刷新
  // 不携带 params（即「立即执行」），带参执行走 handleExecuteWithArgs
  const handleExecuteTodo = useCallback(async (todo: TodoCenterItem) => {
    if (workspaceId == null) return;
    try {
      await db.executeTodo(workspaceId, todo.id, todo.executor || undefined);
      message.success('任务已开始执行');
      reload();
    } catch (e) {
      message.error(`执行失败: ${e instanceof Error ? e.message : '未知错误'}`);
    }
  }, [workspaceId, reload]);

  // 单行删除：调 deleteTodo 后提示 + 刷新
  const handleDeleteTodo = useCallback(async (todo: TodoCenterItem) => {
    if (workspaceId == null) return;
    try {
      await db.deleteTodo(workspaceId, todo.id);
      message.success('已删除');
      reload();
    } catch (e) {
      message.error(`删除失败: ${e instanceof Error ? e.message : '未知错误'}`);
    }
  }, [workspaceId, reload]);

  // 带参执行：弹 Modal 让用户输入补充信息，确认后再调 executeTodo
  // 与 TodoDetail 的带参执行语义一致，但列表页用 Modal 节省横向空间
  const [executeWithArgsModalOpen, setExecuteWithArgsModalOpen] = useState(false);
  const [executeArgs, setExecuteArgs] = useState('');
  const [pendingExecuteTodo, setPendingExecuteTodo] = useState<TodoCenterItem | null>(null);

  const handleExecuteWithArgs = useCallback((todo: TodoCenterItem) => {
    setPendingExecuteTodo(todo);
    setExecuteArgs('');
    setExecuteWithArgsModalOpen(true);
  }, []);

  const confirmExecuteWithArgs = useCallback(async () => {
    if (!pendingExecuteTodo || workspaceId == null) return;
    // params.message 字段与后端 ExecuteRequest 对齐，传 undefined 时后端沿用 todo 原 prompt
    const params = executeArgs.trim() ? { message: executeArgs.trim() } : undefined;
    try {
      await db.executeTodo(workspaceId, pendingExecuteTodo.id, pendingExecuteTodo.executor || undefined, params);
      message.success('任务已开始执行');
      setExecuteWithArgsModalOpen(false);
      setPendingExecuteTodo(null);
      reload();
    } catch (e) {
      message.error(`执行失败: ${e instanceof Error ? e.message : '未知错误'}`);
    }
  }, [pendingExecuteTodo, workspaceId, executeArgs, reload]);

  // 顶部 header：搜索框 + 刷新 + Segmented + 新建
  // 桌面端展开全部；移动端精简（去掉搜索/刷新，保留 Segmented + 新建）
  const headerExtra: ReactNode = isMobile ? (
    <>
      <Segmented
        size="small"
        value={viewMode}
        onChange={(v) => persistView(v as 'card' | 'list')}
        options={[
          { value: 'card', icon: <AppstoreOutlined />, title: '卡片' },
          { value: 'list', icon: <UnorderedListOutlined />, title: '列表' },
        ]}
        data-testid="todo-center-view-toggle"
      />
      <Button size="small" type="primary" icon={<PlusOutlined />} onClick={onOpenCreateModal}>
        新建
      </Button>
    </>
  ) : (
    <>
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
      <Button
        size="small"
        icon={<ReloadOutlined />}
        onClick={() => {
          // 卡片形态触发 TodoCenterCardView 内部 reload；列表形态触发本组件 reload
          if (viewMode === 'list') reload();
          setRefreshKey(k => k + 1);
        }}
        loading={loading}
        aria-label="刷新"
      >
        刷新
      </Button>
      <Segmented
        size="small"
        value={viewMode}
        onChange={(v) => persistView(v as 'card' | 'list')}
        options={[
          { value: 'card', icon: <AppstoreOutlined />, title: '卡片视图' },
          { value: 'list', icon: <UnorderedListOutlined />, title: '列表' },
        ]}
        data-testid="todo-center-view-toggle"
      />
      <Button size="small" type="primary" icon={<PlusOutlined />} onClick={onOpenCreateModal}>
        新建
      </Button>
    </>
  );

  // 卡片形态：直接渲染 TodoCenterCardView，它自带 PageCard + 五类 Tab + 卡片墙
  // 把 headerExtra 透传给卡片视图，让其顶部 header 与列表形态保持一致
  if (viewMode === 'card') {
    return (
      <TodoCenterCardView
        onSelectTodo={onSelectTodo}
        onSelectLoop={onSelectLoop}
        isMobile={isMobile}
        searchKeyword={searchKeyword}
        extra={headerExtra}
        refreshKey={refreshKey}
      />
    );
  }

  // 列表形态：PageCard 包裹 TodoListView（Ant Design Table）
  return (
    <PageCard
      icon={<UnorderedListOutlined />}
      title="事项"
      extra={headerExtra}
      style={{ flex: 1, height: '100%' }}
      contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(100% - 43px)' }}
    >
      <TodoListView
        items={filteredItems}
        loading={loading}
        tags={state.tags}
        onSelectTodo={onSelectTodo}
        onEditTodo={onEditTodo}
        onDeleteTodo={handleDeleteTodo}
        onExecuteTodo={handleExecuteTodo}
        onExecuteWithArgs={handleExecuteWithArgs}
        onRefresh={reload}
      />
      {/* 带参执行 Modal：与 TodoDetail 内的带参执行语义一致，列表页用 Modal 节省横向空间 */}
      <Modal
        title={<><ThunderboltOutlined style={{ marginRight: 8 }} />带参执行</>}
        open={executeWithArgsModalOpen}
        onOk={confirmExecuteWithArgs}
        onCancel={() => {
          setExecuteWithArgsModalOpen(false);
          setPendingExecuteTodo(null);
        }}
        okText="开始执行"
        cancelText="取消"
        destroyOnClose
      >
        <p style={{ marginBottom: 12, color: 'var(--color-text-secondary)' }}>
          输入补充信息，将与任务原有内容一起执行：
        </p>
        <Input.TextArea
          value={executeArgs}
          onChange={(e) => setExecuteArgs(e.target.value)}
          rows={4}
          placeholder="输入补充信息..."
        />
      </Modal>
    </PageCard>
  );
}
