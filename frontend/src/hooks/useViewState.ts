/**
 * useViewState — 统一的 URL-driven 视图导航状态管理。
 *
 * URL 方案：
 *   /?view=items&id=20        事项详情 #20
 *   /?view=items               事项列表（默认，可省略 "?view=items" 以 / 表示）
 *   /?view=loops&id=5         环路详情 #5
 *   /?view=loops               环路列表
 *   /?view=dashboard           仪表盘
 *   /?view=settings&tab=system 设置-系统标签
 *   /?view=memorial            看板
 *   /?view=runtime             运行管理
 *   /?view=skills              Skills
 *   /?view=projectDirectories  工作空间
 *   /?view=sessions            会话
 *   /?view=executors           执行器
 *
 * 移动端 panel 参数（用于 list/detail 独立页面）：
 *   /?view=items&panel=list        事项列表（全屏）
 *   /?view=items&panel=detail&id=123  事项详情（全屏）
 *
 * 只管理 URL + 派生的 React 状态，不持有 Todo/Loop 的 app 数据。
 */

import { useState, useEffect, useCallback, useMemo } from 'react';

export type View =
  | 'items'
  | 'loops'
  | 'dashboard'
  | 'settings'
  | 'memorial'
  | 'runtime'
  | 'skills'
  | 'projectDirectories'
  | 'sessions'
  | 'executors';

export type Panel = 'list' | 'detail';

const ALL_VIEWS: View[] = [
  'items', 'loops',
  'dashboard', 'settings', 'memorial',
  'runtime', 'skills', 'projectDirectories', 'sessions', 'executors',
];

// ─── URL 解析/构建 ─────────────────────────────────────────

function getInitialView(): View {
  const view = new URLSearchParams(window.location.search).get('view') as View | null;
  if (view && ALL_VIEWS.includes(view)) return view;
  return 'items';
}

function getInitialId(): number | null {
  const id = new URLSearchParams(window.location.search).get('id');
  if (!id) return null;
  const n = Number(id);
  return Number.isFinite(n) ? n : null;
}

function getInitialTab(): string | null {
  const tab = new URLSearchParams(window.location.search).get('tab');
  return tab || null;
}

function getInitialPanel(): Panel {
  // 移动端使用 panel 参数区分列表/详情页面
  // 桌面端忽略此参数，始终显示 list+detail 双栏布局
  const panel = new URLSearchParams(window.location.search).get('panel') as Panel | null;
  return panel === 'detail' ? 'detail' : 'list';
}

function buildUrl(view: View, opts?: { id?: number | null; tab?: string | null; panel?: Panel }): string {
  const params = new URLSearchParams();
  params.set('view', view);
  if (opts?.id != null) params.set('id', String(opts.id));
  if (typeof opts?.tab === 'string' && opts.tab.trim()) params.set('tab', opts.tab);
  // 只有移动端需要 panel 参数，桌面端不需要（双栏布局始终显示）
  if (opts?.panel === 'detail') params.set('panel', 'detail');
  const qs = params.toString();
  return qs ? `/?${qs}` : '/';
}

// ─── 左铁路键 ←→ View 映射 ───────────────────────────────
// useViewState 不依赖 LeftRailKey，但提供映射方便 App.tsx 使用。

const VIEW_TO_NAV_KEY: Record<View, string> = {
  items: 'items',
  loops: 'loops',
  dashboard: 'dashboard',
  memorial: 'memorial',
  settings: 'settings',
  runtime: 'settings_runtime',
  skills: 'settings_skills',
  projectDirectories: 'settings_projectDirectories',
  sessions: 'settings_sessions',
  executors: 'settings_executors',
};

export function viewToNavKey(view: View): string {
  return VIEW_TO_NAV_KEY[view];
}

// ─── Hook ────────────────────────────────────────────────

export function useViewState() {
  const [activeView, setActiveView] = useState<View>(getInitialView);
  const [selectedId, setSelectedId] = useState<number | null>(getInitialId);
  // tab 只用于 settings view，暴露给 SettingsPage 使用
  const [activeTab, setActiveTab] = useState<string | null>(getInitialTab);
  // panel 只用于移动端 list/detail 独立页面，桌面端忽略（始终显示双栏）
  const [activePanel, setActivePanel] = useState<Panel>(getInitialPanel);

  // 统一 push URL + React state — 用于视图间导航（首页 → 各视图）
  const pushUrl = useCallback((view: View, opts?: { id?: number | null; tab?: string | null; panel?: Panel }) => {
    const url = buildUrl(view, opts);
    window.history.pushState(null, '', url);
    setActiveView(view);
    setSelectedId(opts?.id ?? null);
    setActiveTab(opts?.tab ?? null);
    // panel 仅在移动端有意义，桌面端始终为 list
    setActivePanel(opts?.panel ?? 'list');
  }, []);

  // replaceUrl — 用于移动端 list/detail 内部切换，不污染浏览器历史
  // 桌面端也可以用，保持行为一致
  const replaceUrl = useCallback((view: View, opts?: { id?: number | null; tab?: string | null; panel?: Panel }) => {
    const url = buildUrl(view, opts);
    window.history.replaceState(null, '', url);
    setActiveView(view);
    setSelectedId(opts?.id ?? null);
    setActiveTab(opts?.tab ?? null);
    setActivePanel(opts?.panel ?? 'list');
  }, []);

  // 统一 popstate 处理 — 替代了之前分散在 App.tsx / SettingsPage 的三个监听器
  useEffect(() => {
    const onPopState = () => {
      const params = new URLSearchParams(window.location.search);
      const view = params.get('view') as View | null;
      const idStr = params.get('id');
      const tab = params.get('tab');
      const panel = params.get('panel') as Panel | null;
      const resolvedView = view && ALL_VIEWS.includes(view) ? view : 'items';
      const resolvedId = idStr ? (Number.isFinite(Number(idStr)) ? Number(idStr) : null) : null;
      setActiveView(resolvedView);
      setSelectedId(resolvedId);
      setActiveTab(tab || null);
      // panel 只在移动端使用，桌面端始终为 list
      setActivePanel(panel === 'detail' ? 'detail' : 'list');
    };
    window.addEventListener('popstate', onPopState);
    return () => window.removeEventListener('popstate', onPopState);
  }, []);

  // showView: 导航到页面型视图（dashboard/settings/memorial），清空 id
  const showView = useCallback((view: View, opts?: { tab?: string | null }) => {
    pushUrl(view, { tab: opts?.tab ?? null });
  }, [pushUrl]);

  // selectTodo: 向后兼容 RunningBoard / RunningRecordDrawer
  // 导航到 items 视图 + 选中 todo
  const selectTodo = useCallback((todoId: number) => {
    if (!Number.isFinite(todoId)) return;
    pushUrl('items', { id: todoId });
  }, [pushUrl]);

  // backToList: 回到当前视图的概览（清空 id，切到 list panel），用于移动端返回按钮
  // 使用 replaceUrl 避免污染浏览器历史（list/detail 切换不应产生历史记录）
  const backToList = useCallback(() => {
    replaceUrl(activeView, { panel: 'list' });
  }, [activeView, replaceUrl]);

  // selectedPanel 从 URL 派生：移动端使用 activePanel，桌面端始终为 list+detail 双栏
  // 桌面端不需要区分 list/detail panel（双栏同时显示），移动端才需要
  const selectedPanel = useMemo<Panel>(() => (selectedId !== null ? 'detail' : 'list'), [selectedId]);

  return {
    activeView,
    selectedId,
    activeTab,
    activePanel,
    selectedPanel,
    showView,
    selectTodo,
    backToList,
    pushUrl,
    replaceUrl,
  };
}
