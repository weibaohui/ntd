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
  | 'executors'
  | 'blackboard';

export type Panel = 'list' | 'detail' | 'post';

const ALL_VIEWS: View[] = [
  'items', 'loops',
  'dashboard', 'settings', 'memorial',
  'runtime', 'skills', 'projectDirectories', 'sessions', 'executors',
  'blackboard',
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
  // 桌面端使用 panel 参数区分列表/帖子详情页面
  const panel = new URLSearchParams(window.location.search).get('panel') as Panel | null;
  if (panel === 'detail' || panel === 'post') return panel;
  return 'list';
}

function getInitialRecordId(): number | null {
  const record = new URLSearchParams(window.location.search).get('record');
  if (!record) return null;
  const n = Number(record);
  return Number.isFinite(n) ? n : null;
}

function buildUrl(view: View, opts?: { id?: number | null; tab?: string | null; panel?: Panel; record?: number | null }): string {
  const params = new URLSearchParams();
  params.set('view', view);
  if (opts?.id != null) params.set('id', String(opts.id));
  if (typeof opts?.tab === 'string' && opts.tab.trim()) params.set('tab', opts.tab);
  // panel 参数：detail 用于移动端，post 用于帖子详情页
  if (opts?.panel === 'detail' || opts?.panel === 'post') params.set('panel', opts.panel);
  if (opts?.record != null) params.set('record', String(opts.record));
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
  blackboard: 'blackboard',
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
  // panel：list=列表，detail=移动端详情，post=帖子详情（桌面端全屏）
  const [activePanel, setActivePanel] = useState<Panel>(getInitialPanel);
  // record：帖子详情页中选中的执行记录 ID
  const [selectedRecordId, setSelectedRecordId] = useState<number | null>(getInitialRecordId);

  // 统一 push URL + React state — 用于视图间导航（首页 → 各视图）
  const pushUrl = useCallback((view: View, opts?: { id?: number | null; tab?: string | null; panel?: Panel; record?: number | null }) => {
    const url = buildUrl(view, opts);
    window.history.pushState(null, '', url);
    setActiveView(view);
    setSelectedId(opts?.id ?? null);
    setActiveTab(opts?.tab ?? null);
    setActivePanel(opts?.panel ?? 'list');
    setSelectedRecordId(opts?.record ?? null);
  }, []);

  // replaceUrl — 用于切换不污染浏览器历史
  const replaceUrl = useCallback((view: View, opts?: { id?: number | null; tab?: string | null; panel?: Panel; record?: number | null }) => {
    const url = buildUrl(view, opts);
    window.history.replaceState(null, '', url);
    setActiveView(view);
    setSelectedId(opts?.id ?? null);
    setActiveTab(opts?.tab ?? null);
    setActivePanel(opts?.panel ?? 'list');
    setSelectedRecordId(opts?.record ?? null);
  }, []);

  // 统一 popstate 处理
  useEffect(() => {
    const onPopState = () => {
      const params = new URLSearchParams(window.location.search);
      const view = params.get('view') as View | null;
      const idStr = params.get('id');
      const tab = params.get('tab');
      const panel = params.get('panel') as Panel | null;
      const recordStr = params.get('record');
      const resolvedView = view && ALL_VIEWS.includes(view) ? view : 'items';
      const resolvedId = idStr ? (Number.isFinite(Number(idStr)) ? Number(idStr) : null) : null;
      const resolvedRecord = recordStr ? (Number.isFinite(Number(recordStr)) ? Number(recordStr) : null) : null;
      setActiveView(resolvedView);
      setSelectedId(resolvedId);
      setActiveTab(tab || null);
      setActivePanel(panel === 'detail' || panel === 'post' ? panel : 'list');
      setSelectedRecordId(resolvedRecord);
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
    selectedRecordId,
    showView,
    selectTodo,
    backToList,
    pushUrl,
    replaceUrl,
  };
}
