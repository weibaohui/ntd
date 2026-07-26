/**
 * useViewState — 统一的 URL-driven 视图导航状态管理。
 *
 * Hash 路由方案（028 重构后统一命名空间）：
 *   /#/todos                  事项列表（卡片/列表形态，默认 localStorage）
 *   /#/todos/:id              事项详情 #id（独立页）
 *   /#/todos/:id/posts/:rid   事项某次执行记录的帖子页
 *   /#/loops                  环路列表（table 形态）
 *   /#/loops/:id              环路详情 #id（独立页）
 *   /#/dashboard              仪表盘
 *   /#/settings?tab=system    设置-系统标签
 *   /#/memorial?mode=kanban   看板-看板视图
 *   /#/memorial?mode=running  看板-运行视图
 *   /#/memorial?mode=loop_kanban  看板-环路视图
 *   /#/memorial?mode=memorial 看板-结论视图
 *   /#/runtime                运行管理
 *   /#/skills                 Skills
 *   /#/projectDirectories     工作空间
 *   /#/sessions               会话
 *   /#/executors              执行器
 *   /#/experts                专家管理面板
 *   /#/blackboard             黑板
 *   /#/wiki?workspace=1&slug=auth-module  Wiki 主题页面
 *
 * 设计要点：
 * - todos / loops 用 path 段区分列表/详情，刷新/分享/后退可恢复
 * - 其他视图（settings/memorial/wiki/blackboard/processes）仍用 query 参数
 * - 不做旧 `/#/items` URL 兼容重定向，全站统一到 `/#/todos`
 *
 * 只管理 URL + 派生的 React 状态，不持有 Todo/Loop 的 app 数据。
 */

import { useState, useEffect, useCallback } from 'react';

export type View =
  | 'todos'          // 事项命名空间（列表 /#/todos + 详情 /#/todos/:id + 帖子 /#/todos/:id/posts/:rid）
  | 'loops'          // 环路命名空间（列表 /#/loops + 详情 /#/loops/:id）
  | 'tasks'
  | 'processes'
  | 'dashboard'
  | 'settings'
  | 'memorial'
  | 'runtime'
  | 'skills'
  | 'projectDirectories'
  | 'sessions'
  | 'executors'
  | 'experts'
  | 'blackboard'
  | 'wiki'
  | 'messages'
  | 'bots'
  | 'onboarding';

// 028 之前用 'items' + ?panel=detail|post 区分详情；现已统一到 'todos' + path 段，Panel 类型不再需要。
// 保留 'list' | 'detail' 字面量仅用于 MobileHeader 派生状态，避免大范围改动移动端组件签名。
export type Panel = 'list' | 'detail' | 'post';

export type BoardMode = 'memorial' | 'kanban' | 'running' | 'loop_kanban';

const ALL_VIEWS: View[] = [
  'todos', 'loops', 'tasks',
  'dashboard', 'settings', 'memorial',
  'runtime', 'skills', 'projectDirectories', 'sessions', 'executors', 'experts',
  'blackboard', 'wiki', 'messages', 'bots', 'processes', 'onboarding',
];

const ALL_BOARD_MODES: BoardMode[] = ['memorial', 'kanban', 'running', 'loop_kanban'];

/** 从 hash 中提取 path 部分（去 query）。如 `#/todos/123?tab=x` → `/todos/123`。 */
function getHashPath(): string {
  const hash = window.location.hash || '';
  const hashWithoutHash = hash.startsWith('#') ? hash.slice(1) : hash;
  const [path] = hashWithoutHash.split('?', 2);
  return path || '';
}

/** 从 hash 中提取 query 参数。如 `#/settings?tab=system` → URLSearchParams({tab:'system'})。 */
function getHashSearchParams(): URLSearchParams {
  const hash = window.location.hash || '';
  const hashWithoutHash = hash.startsWith('#') ? hash.slice(1) : hash;
  const [, search] = hashWithoutHash.split('?', 2);
  return new URLSearchParams(search || '');
}

/**
 * 把 hash path 切成段，便于按 segment 取 id。
 *   /#/todos/123/posts/456 → ['todos', '123', 'posts', '456']
 *   /#/loops/456          → ['loops', '456']
 *   /#/dashboard          → ['dashboard']
 */
function getHashPathSegments(): string[] {
  const path = getHashPath();
  const viewPart = path.startsWith('/') ? path.slice(1) : path;
  return viewPart.split('/').filter(Boolean);
}

/** 从 path 段第一段解析 View 类型；未匹配时 fallback 到 'todos'（友好引导，避免 404）。 */
function parseViewFromSegments(segments: string[]): View {
  const first = segments[0];
  if (first && ALL_VIEWS.includes(first as View)) {
    return first as View;
  }
  // 旧 `/#/items` 不做重定向：用户手动输入旧 URL 时统一回到 `/#/todos` 列表
  return 'todos';
}

/** 从 path 段指定位置取数字 id；越界或非数字返回 null。 */
function getPathIdAt(segments: string[], index: number): number | null {
  const raw = segments[index];
  if (!raw) return null;
  const n = Number(raw);
  return Number.isFinite(n) ? n : null;
}

function getInitialView(): View {
  return parseViewFromSegments(getHashPathSegments());
}

/** 事项详情 id：来自 path 段 `todos/:id` 中的 :id。 */
function getInitialTodoDetailId(): number | null {
  const segs = getHashPathSegments();
  if (segs[0] !== 'todos') return null;
  return getPathIdAt(segs, 1);
}

/** 环路详情 id：来自 path 段 `loops/:id` 中的 :id。 */
function getInitialLoopDetailId(): number | null {
  const segs = getHashPathSegments();
  if (segs[0] !== 'loops') return null;
  return getPathIdAt(segs, 1);
}

/** 帖子记录 id：来自 path 段 `todos/:id/posts/:rid` 中的 :rid。 */
function getInitialPostRecordId(): number | null {
  const segs = getHashPathSegments();
  if (segs[0] !== 'todos') return null;
  if (segs[2] !== 'posts') return null;
  return getPathIdAt(segs, 3);
}

function getInitialTab(): string | null {
  const params = getHashSearchParams();
  const tab = params.get('tab');
  return tab || null;
}

function getInitialBoardMode(): BoardMode {
  const params = getHashSearchParams();
  const mode = params.get('mode') as BoardMode | null;
  if (mode && ALL_BOARD_MODES.includes(mode)) return mode;
  return 'memorial';
}

function getInitialWikiSlug(): string | null {
  const params = getHashSearchParams();
  return params.get('slug');
}

function getInitialBlackboardFile(): string | null {
  const params = getHashSearchParams();
  return params.get('file');
}

function getInitialProcessName(): string | null {
  const params = getHashSearchParams();
  return params.get('name');
}

/** 导航参数：todos/loops 用 id/recordId 构造 path 段；其他视图用 query。 */
interface NavOpts {
  /** 详情 id（todos/loops 用，构造 path 段 /todos/:id）。 */
  id?: number | null;
  /** 帖子记录 id（todos 用，构造 /todos/:id/posts/:recordId）。 */
  recordId?: number | null;
  tab?: string | null;
  mode?: BoardMode;
  workspace?: number | null;
  slug?: string | null;
  file?: string | null;
  name?: string | null;
}

/**
 * 构造 hash URL。
 * - todos/loops 用 path 段（/todos、/todos/:id、/todos/:id/posts/:rid、/loops、/loops/:id）
 * - 其他视图用 query 参数（与 028 之前一致）
 */
function buildHashUrl(view: View, opts?: NavOpts): string {
  // 事项命名空间：path 段驱动
  if (view === 'todos') {
    if (opts?.id != null && opts?.recordId != null) {
      return `#/todos/${opts.id}/posts/${opts.recordId}`;
    }
    if (opts?.id != null) {
      return `#/todos/${opts.id}`;
    }
    return `#/todos`;
  }
  // 环路命名空间：path 段驱动
  if (view === 'loops') {
    if (opts?.id != null) {
      return `#/loops/${opts.id}`;
    }
    return `#/loops`;
  }
  // 其他视图保持 query 参数风格
  const path = `/${view}`;
  const params = new URLSearchParams();
  if (typeof opts?.tab === 'string' && opts.tab.trim()) params.set('tab', opts.tab);
  if (opts?.mode) params.set('mode', opts.mode);
  if (view === 'wiki') {
    // wiki 视图需要 workspace 和 slug 来定位文件
    if (opts?.workspace != null) params.set('workspace', String(opts.workspace));
    if (opts?.slug) params.set('slug', opts.slug);
  }
  if (view === 'blackboard' && opts?.file) {
    // blackboard 视图的 file 参数标识当前查看的文件
    params.set('file', opts.file);
  }
  if (view === 'processes' && opts?.name) {
    // processes 视图的 name 参数定位工艺模板，用于「环路 → 来源工艺」回跳自动开详情
    params.set('name', opts.name);
  }
  const qs = params.toString();
  return qs ? `#${path}?${qs}` : `#${path}`;
}

const VIEW_TO_NAV_KEY: Record<View, string> = {
  todos: 'todos',      // 事项命名空间统一高亮「事项」导航项
  loops: 'loops',
  tasks: 'tasks',
  processes: 'processes',
  dashboard: 'dashboard',
  memorial: 'memorial',
  blackboard: 'blackboard',
  settings: 'settings',
  runtime: 'settings_runtime',
  skills: 'settings_skills',
  projectDirectories: 'settings_projectDirectories',
  sessions: 'settings_sessions',
  executors: 'settings_executors',
  experts: 'settings_experts',

  wiki: 'blackboard',
  messages: 'messages',
  bots: 'settings_bots',
  onboarding: 'onboarding',
};

export function viewToNavKey(view: View): string {
  return VIEW_TO_NAV_KEY[view];
}

export function useViewState() {
  const [activeView, setActiveView] = useState<View>(getInitialView);
  // 028：todoDetailId / loopDetailId / postRecordId 来自 path 段，刷新可恢复
  const [todoDetailId, setTodoDetailId] = useState<number | null>(getInitialTodoDetailId);
  const [loopDetailId, setLoopDetailId] = useState<number | null>(getInitialLoopDetailId);
  const [postRecordId, setPostRecordId] = useState<number | null>(getInitialPostRecordId);
  const [activeTab, setActiveTab] = useState<string | null>(getInitialTab);
  const [boardMode, setBoardMode] = useState<BoardMode>(getInitialBoardMode);
  const [wikiSlug, setWikiSlug] = useState<string | null>(getInitialWikiSlug);
  const [blackboardFile, setBlackboardFile] = useState<string | null>(getInitialBlackboardFile);
  const [processName, setProcessName] = useState<string | null>(getInitialProcessName);

  const pushUrl = useCallback((view: View, opts?: NavOpts) => {
    const hashUrl = buildHashUrl(view, opts);
    window.history.pushState(null, '', hashUrl);
    syncStateFromOptions(view, opts, {
      setActiveView, setTodoDetailId, setLoopDetailId, setPostRecordId,
      setActiveTab, setBoardMode, setWikiSlug, setBlackboardFile, setProcessName,
    });
  }, []);

  const replaceUrl = useCallback((view: View, opts?: NavOpts) => {
    const hashUrl = buildHashUrl(view, opts);
    window.history.replaceState(null, '', hashUrl);
    syncStateFromOptions(view, opts, {
      setActiveView, setTodoDetailId, setLoopDetailId, setPostRecordId,
      setActiveTab, setBoardMode, setWikiSlug, setBlackboardFile, setProcessName,
    });
  }, []);

  useEffect(() => {
    const onPopState = () => {
      // 后退/前进时从 hash 重新解析所有状态
      const segments = getHashPathSegments();
      const view = parseViewFromSegments(segments);
      const params = getHashSearchParams();
      const tab = params.get('tab');
      const mode = params.get('mode') as BoardMode | null;
      const slug = params.get('slug');
      const file = params.get('file');
      const name = params.get('name');
      const resolvedMode = mode && ALL_BOARD_MODES.includes(mode) ? mode : 'memorial';

      setActiveView(view);
      // todos/loops 详情 id 仅在对应 view 下提取，避免跨视图串台
      setTodoDetailId(view === 'todos' ? getPathIdAt(segments, 1) : null);
      setPostRecordId(view === 'todos' && segments[2] === 'posts' ? getPathIdAt(segments, 3) : null);
      setLoopDetailId(view === 'loops' ? getPathIdAt(segments, 1) : null);
      setActiveTab(tab || null);
      setBoardMode(resolvedMode);
      setWikiSlug(view === 'wiki' ? (slug || null) : null);
      setBlackboardFile(view === 'blackboard' ? (file || null) : null);
      setProcessName(view === 'processes' ? (name || null) : null);
    };
    window.addEventListener('popstate', onPopState);
    return () => window.removeEventListener('popstate', onPopState);
  }, []);

  const showView = useCallback((view: View, opts?: { tab?: string | null }) => {
    pushUrl(view, { tab: opts?.tab ?? null });
  }, [pushUrl]);

  /** 选中事项 → 跳转到事项详情独立页 `/#/todos/:id`。 */
  const selectTodo = useCallback((todoId: number) => {
    if (!Number.isFinite(todoId)) return;
    pushUrl('todos', { id: todoId });
  }, [pushUrl]);

  const selectWiki = useCallback((workspaceId: number, slug: string) => {
    pushUrl('wiki', { workspace: workspaceId, slug });
  }, [pushUrl]);

  /** 返回列表：todos/loops 用 path 段清空 detail；其他视图无 path 段，等同 showView。 */
  const backToList = useCallback(() => {
    if (activeView === 'todos') { replaceUrl('todos'); return; }
    if (activeView === 'loops') { replaceUrl('loops'); return; }
    pushUrl(activeView);
  }, [activeView, pushUrl, replaceUrl]);

  // MobileHeader 需要知道当前是否处于「详情态」以决定返回按钮显隐；
  // 由 todoDetailId/loopDetailId 派生，保持兼容旧 activePanel: 'detail' | 'list' 接口。
  const activePanel: Panel = (activeView === 'todos' && todoDetailId != null)
    || (activeView === 'loops' && loopDetailId != null)
    ? 'detail'
    : 'list';

  return {
    activeView,
    // 028：详情 id（path 段驱动）
    todoDetailId,
    loopDetailId,
    postRecordId,
    // 派生：仅用于 MobileHeader 返回按钮显隐
    activePanel,
    activeTab,
    boardMode,
    wikiSlug,
    blackboardFile,
    processName,
    showView,
    selectTodo,
    selectWiki,
    backToList,
    pushUrl,
    replaceUrl,
  };
}

/**
 * 把 NavOpts 同步到 React state。
 * 抽成纯函数避免 pushUrl/replaceUrl 重复实现，并让函数体保持简短。
 */
function syncStateFromOptions(
  view: View,
  opts: NavOpts | undefined,
  setters: {
    setActiveView: (v: View) => void;
    setTodoDetailId: (id: number | null) => void;
    setLoopDetailId: (id: number | null) => void;
    setPostRecordId: (id: number | null) => void;
    setActiveTab: (t: string | null) => void;
    setBoardMode: (m: BoardMode) => void;
    setWikiSlug: (s: string | null) => void;
    setBlackboardFile: (f: string | null) => void;
    setProcessName: (n: string | null) => void;
  },
): void {
  const {
    setActiveView, setTodoDetailId, setLoopDetailId, setPostRecordId,
    setActiveTab, setBoardMode, setWikiSlug, setBlackboardFile, setProcessName,
  } = setters;
  setActiveView(view);
  // todos: id+recordId 表示帖子页；仅 id 表示详情；都没有表示列表
  setTodoDetailId(view === 'todos' ? (opts?.id ?? null) : null);
  setPostRecordId(view === 'todos' ? (opts?.recordId ?? null) : null);
  setLoopDetailId(view === 'loops' ? (opts?.id ?? null) : null);
  setActiveTab(opts?.tab ?? null);
  setBoardMode(opts?.mode ?? 'memorial');
  setWikiSlug(view === 'wiki' ? (opts?.slug ?? null) : null);
  setBlackboardFile(view === 'blackboard' ? (opts?.file ?? null) : null);
  setProcessName(view === 'processes' ? (opts?.name ?? null) : null);
}
