/**
 * useViewState — 统一的 URL-driven 视图导航状态管理。
 *
 * Hash 路由方案：
 *   /#/items?id=20             事项详情 #20
 *   /#/items                   事项列表（默认）
 *   /#/loops?id=5              环路详情 #5
 *   /#/loops                   环路列表
 *   /#/dashboard               仪表盘
 *   /#/settings?tab=system     设置-系统标签
 *   /#/memorial?mode=kanban    看板-看板视图
 *   /#/memorial?mode=running   看板-运行视图
 *   /#/memorial?mode=loop_kanban  看板-环路视图
 *   /#/memorial?mode=memorial  看板-结论视图
 *   /#/runtime                 运行管理
 *   /#/skills                  Skills
 *   /#/projectDirectories      工作空间
 *   /#/sessions                会话
 *   /#/executors               执行器
 *   /#/blackboard              黑板
 *
 * 移动端 panel 参数（用于 list/detail 独立页面）：
 *   /#/items?panel=list        事项列表（全屏）
 *   /#/items?panel=detail&id=123  事项详情（全屏）
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

export type BoardMode = 'memorial' | 'kanban' | 'running' | 'loop_kanban';

const ALL_VIEWS: View[] = [
  'items', 'loops',
  'dashboard', 'settings', 'memorial',
  'runtime', 'skills', 'projectDirectories', 'sessions', 'executors',
  'blackboard',
];

const ALL_BOARD_MODES: BoardMode[] = ['memorial', 'kanban', 'running', 'loop_kanban'];

function getHashPath(): string {
  const hash = window.location.hash || '';
  const hashWithoutHash = hash.startsWith('#') ? hash.slice(1) : hash;
  const [path] = hashWithoutHash.split('?', 2);
  return path || '';
}

function getHashSearchParams(): URLSearchParams {
  const hash = window.location.hash || '';
  const hashWithoutHash = hash.startsWith('#') ? hash.slice(1) : hash;
  const [, search] = hashWithoutHash.split('?', 2);
  return new URLSearchParams(search || '');
}

function parseViewFromHash(): View {
  const path = getHashPath();
  const viewPart = path.startsWith('/') ? path.slice(1) : path;
  if (viewPart && ALL_VIEWS.includes(viewPart as View)) {
    return viewPart as View;
  }
  return 'items';
}

function getInitialView(): View {
  return parseViewFromHash();
}

function getInitialId(): number | null {
  const params = getHashSearchParams();
  const id = params.get('id');
  if (!id) return null;
  const n = Number(id);
  return Number.isFinite(n) ? n : null;
}

function getInitialTab(): string | null {
  const params = getHashSearchParams();
  const tab = params.get('tab');
  return tab || null;
}

function getInitialPanel(): Panel {
  const params = getHashSearchParams();
  const panel = params.get('panel') as Panel | null;
  if (panel === 'detail' || panel === 'post') return panel;
  return 'list';
}

function getInitialRecordId(): number | null {
  const params = getHashSearchParams();
  const record = params.get('record');
  if (!record) return null;
  const n = Number(record);
  return Number.isFinite(n) ? n : null;
}

function getInitialBoardMode(): BoardMode {
  const params = getHashSearchParams();
  const mode = params.get('mode') as BoardMode | null;
  if (mode && ALL_BOARD_MODES.includes(mode)) return mode;
  return 'memorial';
}

function buildHashUrl(view: View, opts?: { id?: number | null; tab?: string | null; panel?: Panel; record?: number | null; mode?: BoardMode }): string {
  const path = `/${view}`;
  const params = new URLSearchParams();
  if (opts?.id != null) params.set('id', String(opts.id));
  if (typeof opts?.tab === 'string' && opts.tab.trim()) params.set('tab', opts.tab);
  if (opts?.panel === 'detail' || opts?.panel === 'post') params.set('panel', opts.panel);
  if (opts?.record != null) params.set('record', String(opts.record));
  if (opts?.mode) params.set('mode', opts.mode);
  const qs = params.toString();
  return qs ? `#${path}?${qs}` : `#${path}`;
}

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

export function useViewState() {
  const [activeView, setActiveView] = useState<View>(getInitialView);
  const [selectedId, setSelectedId] = useState<number | null>(getInitialId);
  const [activeTab, setActiveTab] = useState<string | null>(getInitialTab);
  const [activePanel, setActivePanel] = useState<Panel>(getInitialPanel);
  const [selectedRecordId, setSelectedRecordId] = useState<number | null>(getInitialRecordId);
  const [boardMode, setBoardMode] = useState<BoardMode>(getInitialBoardMode);

  const pushUrl = useCallback((view: View, opts?: { id?: number | null; tab?: string | null; panel?: Panel; record?: number | null; mode?: BoardMode }) => {
    const hashUrl = buildHashUrl(view, opts);
    window.history.pushState(null, '', hashUrl);
    setActiveView(view);
    setSelectedId(opts?.id ?? null);
    setActiveTab(opts?.tab ?? null);
    setActivePanel(opts?.panel ?? 'list');
    setSelectedRecordId(opts?.record ?? null);
    setBoardMode(opts?.mode ?? 'memorial');
  }, []);

  const replaceUrl = useCallback((view: View, opts?: { id?: number | null; tab?: string | null; panel?: Panel; record?: number | null; mode?: BoardMode }) => {
    const hashUrl = buildHashUrl(view, opts);
    window.history.replaceState(null, '', hashUrl);
    setActiveView(view);
    setSelectedId(opts?.id ?? null);
    setActiveTab(opts?.tab ?? null);
    setActivePanel(opts?.panel ?? 'list');
    setSelectedRecordId(opts?.record ?? null);
    setBoardMode(opts?.mode ?? 'memorial');
  }, []);

  useEffect(() => {
    const onPopState = () => {
      const view = parseViewFromHash();
      const params = getHashSearchParams();
      const idStr = params.get('id');
      const tab = params.get('tab');
      const panel = params.get('panel') as Panel | null;
      const recordStr = params.get('record');
      const mode = params.get('mode') as BoardMode | null;
      const resolvedId = idStr ? (Number.isFinite(Number(idStr)) ? Number(idStr) : null) : null;
      const resolvedRecord = recordStr ? (Number.isFinite(Number(recordStr)) ? Number(recordStr) : null) : null;
      const resolvedMode = mode && ALL_BOARD_MODES.includes(mode) ? mode : 'memorial';
      setActiveView(view);
      setSelectedId(resolvedId);
      setActiveTab(tab || null);
      setActivePanel(panel === 'detail' || panel === 'post' ? panel : 'list');
      setSelectedRecordId(resolvedRecord);
      setBoardMode(resolvedMode);
    };
    window.addEventListener('popstate', onPopState);
    return () => window.removeEventListener('popstate', onPopState);
  }, []);

  const showView = useCallback((view: View, opts?: { tab?: string | null }) => {
    pushUrl(view, { tab: opts?.tab ?? null });
  }, [pushUrl]);

  const selectTodo = useCallback((todoId: number) => {
    if (!Number.isFinite(todoId)) return;
    pushUrl('items', { id: todoId });
  }, [pushUrl]);

  const backToList = useCallback(() => {
    replaceUrl(activeView, { panel: 'list' });
  }, [activeView, replaceUrl]);

  const selectedPanel = useMemo<Panel>(() => (selectedId !== null ? 'detail' : 'list'), [selectedId]);

  return {
    activeView,
    selectedId,
    activeTab,
    activePanel,
    selectedPanel,
    selectedRecordId,
    boardMode,
    showView,
    selectTodo,
    backToList,
    pushUrl,
    replaceUrl,
  };
}
