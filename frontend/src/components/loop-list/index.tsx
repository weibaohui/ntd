// LoopListPage — 028-列表详情独立路由：环路列表独立页容器。
//
// 设计要点（028-列表详情独立路由-设计 §4.1）：
// 1. 替代原 LoopPage 的列表部分；环路详情已独立到 LoopDetailPage（URL: /#/loops/:id）。
// 2. 拉取环路列表 → 注入 LoopListView 渲染 table。
// 3. 顶部 header：搜索框 + 刷新 + 工作空间配置入口（原 LoopMobilePage 的
//    WorkspaceLoopConfigPage 入口迁到这里，保持列表页能直接管理评审模板）。
//    044：环路只由工艺 install/upgrade 产生，「新建」按钮已下线。
// 4. 监听 loopUpdateCount 触发重拉，让外部（如 LoopDetailPage 删了一个环路）能联动刷新。
// 5. 单函数 ≤ 30 行：数据拉取/过滤/回调已拆到子模块。

import { useCallback, useEffect, useMemo, useState } from 'react';
import { message } from 'antd';
import { RetweetOutlined } from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
// 093：本组件只消费 todo 域状态，用细粒度 useTodos 替代合并版 useApp，
// 执行态（进度/统计推送）变化不再触发本组件重渲染。
import { useTodos } from '@/hooks/useTodoContext';
import { PageCard } from '@/components/common/PageCard';
import { WorkspaceLoopConfigPage } from '@/components/settings/workspace/WorkspaceLoopConfigPage';
// LoopKanban：环路执行历史看板（8 列），kanban 视图复用。
import { LoopKanban } from '@/components/loop-kanban';
// 109：列表形态直达路由——useViewState 提供 listView（URL ?view= 原文）与 replaceUrl。
import { useViewState, pickListView } from '@/hooks/useViewState';
import { LoopListView } from './LoopListView';
import {
  LoopListHeader,
  useLoopRowActions,
  useLoopConfig,
} from './LoopListPageParts';
import type { LoopListItem } from '@/types/loop';

/** localStorage 键：记住用户上次选的列表/看板形态（URL 无 ?view= 参数时的兜底）。 */
const VIEW_STORAGE_KEY = 'ntd_loops_view';

/** 读取持久化的视图模式，默认列表（环路管理默认 table）。 */
function readInitialView(): 'list' | 'kanban' {
  try {
    const v = localStorage.getItem(VIEW_STORAGE_KEY);
    if (v === 'kanban') return 'kanban';
    return 'list';
  } catch {
    return 'list';
  }
}

/** 环路列表数据加载 hook：响应 workspace/loopUpdateCount 变化。 */
function useLoopListData(workspaceId: number | null, loopUpdateCount: number) {
  const [items, setItems] = useState<LoopListItem[]>([]);
  const [loading, setLoading] = useState(false);

  // reload 用 useCallback 保证引用稳定，避免 useEffect 依赖循环
  const reload = useCallback(async () => {
    if (workspaceId == null) { setItems([]); return; }
    setLoading(true);
    try {
      const data = await dbLoops.listLoops(workspaceId);
      setItems(data);
    } catch (e) {
      message.error(`加载环路列表失败：${e instanceof Error ? e.message : String(e)}`);
      setItems([]);
    } finally {
      setLoading(false);
    }
  }, [workspaceId]);

  // 工作空间变化或外部通知时重拉
  useEffect(() => { reload(); }, [reload, loopUpdateCount]);

  return { items, loading, reload };
}

interface LoopListPageProps {
  /** 点击行跳转：由父组件 pushUrl('loops', { id })。 */
  onSelectLoop: (id: number) => void;
  /** 触发外部计数变化，让父组件刷新 LoopDetailPage 等。 */
  onLoopChanged?: () => void;
  /** 父组件维护的刷新信号（如 LoopDetailPage 删除后递增），变化时重拉列表。 */
  loopUpdateCount?: number;
}

/**
 * 环路列表独立页：URL `/#/loops`。
 *
 * 整体处理思路：
 * 1. 挂载时拉取环路列表；工作空间变化 / loopUpdateCount 变化时重拉。
 * 2. 顶部 header 提供搜索 + 刷新 + 新建 + 配置按钮（LoopListHeader）。
 * 3. 行操作回调拆到 useLoopRowActions；配置页入口拆到 useLoopConfig。
 * 4. 把过滤后的列表传给 LoopListView 渲染 table。
 */
export function LoopListPage({
  onSelectLoop,
  onLoopChanged,
  loopUpdateCount = 0,
}: LoopListPageProps) {
  const { state } = useTodos();
  const workspaceId = state.selectedWorkspace;
  // 109：pushUrl 用于看板点事项跳详情；listView 是 URL ?view= 原文；replaceUrl 用于形态写回。
  const { pushUrl, listView, replaceUrl } = useViewState();
  const [searchKeyword, setSearchKeyword] = useState('');
  // 视图模式：URL ?view= 优先（直达指定形态），无参数/非法值回退 localStorage 记忆。
  // storedView 只在挂载时读一次：URL 变化走 listView 同步，localStorage 只作无参数兜底。
  const [storedView] = useState<'list' | 'kanban'>(readInitialView);
  // 泛型版 pickListView 由 allowed/fallback 推导联合类型，无需 as 断言
  const viewMode = pickListView(listView, ['list', 'kanban'], storedView);
  // 111：list/kanban 各自独立时间窗——两个形态的数据维度不同（环路定义 vs 执行历史），
  // 共享一个 state 会让默认值互相污染。默认值：list=null（全部，管理视角不默认收窄）；
  // kanban=24（保持历史默认，执行历史维度不突变）。两者均不持久化，与任务页口径一致。
  const [listHours, setListHours] = useState<number | null>(null);
  const [kanbanHours, setKanbanHours] = useState<number | null>(24);

  // 列表数据加载 hook（仅 list 态消费；kanban 态 LoopKanban 自拉执行历史）
  const { items, loading, reload } = useLoopListData(workspaceId, loopUpdateCount);

  const handleViewChange = useCallback((m: 'list' | 'kanban') => {
    // 写 localStorage 兜底 + replaceUrl 同步 URL（?view=），使形态可直达/分享。
    try { localStorage.setItem(VIEW_STORAGE_KEY, m); } catch { /* 静默降级 */ }
    replaceUrl('loops', { view: m });
  }, [replaceUrl]);

  // kanban 态执行轨迹流程图点事项标题 → 跳事项详情。
  const handleOpenTodo = useCallback((todoId: number) => {
    pushUrl('todos', { id: todoId });
  }, [pushUrl]);

  // 行操作回调（已拆到 useLoopRowActions）
  // 044：触发/复制已随手工环路能力下线，只剩删除与启停
  const { handleDelete, handleToggleStatus } = useLoopRowActions({
    workspaceId, onReload: reload, onLoopChanged,
  });

  // 配置页入口（已拆到 useLoopConfig）
  const { loopConfigOpen, currentWorkspace, handleOpenLoopConfig, handleCloseLoopConfig } = useLoopConfig({
    workspaceId,
  });

  // 工作空间切换时关闭配置页
  useEffect(() => { handleCloseLoopConfig(); }, [workspaceId, handleCloseLoopConfig]);

  // 按搜索词 + 时间窗过滤：useMemo 避免每次渲染都重新计算。
  // 111：时间窗按环路 created_at 过滤（与任务页口径一致）；created_at 缺失/非法
  // 视为不在窗口内（与任务页对非法时间的 NaN-drop 处理对齐）。
  const filteredItems = useMemo(() => {
    let result = items;
    if (listHours != null) {
      const cutoff = Date.now() - listHours * 3600 * 1000;
      result = result.filter(l => {
        const ts = l.created_at ? new Date(l.created_at).getTime() : NaN;
        return !Number.isNaN(ts) && ts >= cutoff;
      });
    }
    const kw = searchKeyword.trim().toLowerCase();
    if (!kw) return result;
    return result.filter(l => (l.name || '').toLowerCase().includes(kw));
  }, [items, searchKeyword, listHours]);

  // 配置态：渲染 WorkspaceLoopConfigPage 替代列表
  if (loopConfigOpen && currentWorkspace) {
    return (
      <WorkspaceLoopConfigPage workspace={currentWorkspace} onBack={handleCloseLoopConfig} />
    );
  }

  // header 共用：list/kanban 态共享 searchKeyword/Segmented；
  // 111：时间窗按形态路由到各自 state，切换形态互不污染。
  const headerExtra = (
    <LoopListHeader
      viewMode={viewMode}
      onViewChange={handleViewChange}
      searchKeyword={searchKeyword}
      hours={viewMode === 'kanban' ? kanbanHours : listHours}
      onHoursChange={viewMode === 'kanban' ? setKanbanHours : setListHours}
      loading={loading}
      workspaceId={workspaceId}
      onSearchChange={setSearchKeyword}
      onReload={reload}
      onOpenConfig={handleOpenLoopConfig}
    />
  );

  // kanban 态：PageCard + LoopKanban（执行历史，受控 searchText/hours/onOpenTodo）。
  // 维度提示：list 是 loop 定义管理，kanban 是 loop 执行历史——切换 Segmented 会换数据对象。
  const renderKanbanView = () => (
    <PageCard
      icon={<RetweetOutlined />}
      title="环路"
      extra={headerExtra}
      style={{ flex: 1, height: '100%' }}
      contentStyle={{ height: 'calc(100% - 43px)', overflow: 'hidden' }}
    >
      <LoopKanban searchText={searchKeyword} hours={kanbanHours} onOpenTodo={handleOpenTodo} />
    </PageCard>
  );

  // 列表态：PageCard + LoopListView
  const renderListView = () => (
    <PageCard
      icon={<RetweetOutlined />}
      title="环路"
      extra={headerExtra}
      style={{ flex: 1, height: '100%' }}
      contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(100% - 43px)' }}
    >
      <LoopListView
        items={filteredItems}
        loading={loading}
        tags={state.tags}
        onSelectLoop={onSelectLoop}
        onDelete={handleDelete}
        onToggleStatus={handleToggleStatus}
        onRefresh={reload}
      />
    </PageCard>
  );

  return viewMode === 'kanban' ? renderKanbanView() : renderListView();
}
