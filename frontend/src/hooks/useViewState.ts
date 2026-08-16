/**
 * useViewState — 统一的 URL-driven 视图导航状态管理。
 *
 * Hash 路由方案（028 重构后统一命名空间）：
 *   /#/todos                  事项列表（卡片/列表形态，默认 localStorage）
 *   /#/todos/:id              事项详情 #id（独立页）
 *   /#/todos/:id/posts/:rid   事项某次执行记录的帖子页
 *   /#/loops                  环路列表（table 形态）
 *   /#/loops/:id              环路详情 #id（独立页）
 *   /#/tasks/:id              任务详情 #id（独立页）
 *   /#/dashboard              仪表盘
 *   /#/settings?tab=system    设置-系统标签
 *   /#/runtime                运行管理
 *   /#/skills                 Skills
 *   /#/workspaces     工作空间
 *   /#/sessions               会话
 *   /#/executors              执行器
 *   /#/experts                专家管理面板
 *   /#/blackboard             黑板
 *   /#/wiki?workspace=1&slug=auth-module  Wiki 主题页面
 *
 * 设计要点：
 * - todos / loops 用 path 段区分列表/详情，刷新/分享/后退可恢复
 * - 其他视图（settings/wiki/blackboard/processes）仍用 query 参数
 * - 不做旧 `/#/items` URL 兼容重定向，全站统一到 `/#/todos`
 *
 * 只管理 URL + 派生的 React 状态，不持有 Todo/Loop 的 app 数据。
 */

import { useState, useEffect, useCallback } from 'react';

// 028 路由同步：history.pushState/replaceState 不会触发 popstate 事件，
// 因此当 BlackboardPage/ReferencingLoopsSection 等嵌套组件
// 调用 pushUrl/replaceUrl 时，App 根组件的 useViewState 实例不会更新。
// 解决方案：用模块级 EventTarget 广播 'nav-change' 事件，
// 所有 useViewState 实例都监听该事件，从当前 hash 重新同步状态。
// 这样任何一处发起导航，全站所有实例都会同步更新。
const navEventTarget = new EventTarget();
const NAV_CHANGE_EVENT = 'ntd-nav-change';

export type View =
  | 'todos'          // 事项命名空间（列表 /#/todos + 详情 /#/todos/:id + 帖子 /#/todos/:id/posts/:rid）
  | 'loops'          // 环路命名空间（列表 /#/loops + 详情 /#/loops/:id）
  | 'tasks'
  | 'processes'
  | 'dashboard'
  | 'settings'
  | 'runtime'
  | 'skills'
  | 'workspaces'
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

// 所有合法 View 字面量集合：parseViewFromSegments 用它做 includes 校验，
// 决定是否接受 URL 第一段为有效视图；新增 View 时必须同步追加，否则会被当成 fallback。
const ALL_VIEWS: View[] = [
  'todos', 'loops', 'tasks',
  'dashboard', 'settings',
  'runtime', 'skills', 'workspaces', 'sessions', 'executors', 'experts',
  'blackboard', 'wiki', 'messages', 'bots', 'processes', 'onboarding',
];

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

/**
 * 从 path 段指定位置取数字 id；越界或非数字返回 null。
 * 边界条件：
 *   - 索引越界（segments[index] === undefined）→ 返回 null，避免对 undefined 调 Number() 得 NaN
 *   - 非数字字符串（如 /#/todos/abc）→ Number('abc')=NaN，Number.isFinite 过滤后返回 null
 *   - 空字符串（连续斜杠 /#/todos//posts/1）→ Number('')=0，会返回 0；
 *     0 不是合法业务 id，调用方 getInitialTodoDetailId 等会自然走「无 id」分支
 */
function getPathIdAt(segments: string[], index: number): number | null {
  const raw = segments[index];
  if (!raw) return null; // 边界：索引越界或空串，统一返回 null 避免下游处理 0/NaN
  const n = Number(raw);
  return Number.isFinite(n) ? n : null;
}

/** 应用首屏从 URL 解析当前 View；hash 缺失或非法时 fallback 到 'todos'。 */
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

/** 任务详情 id：来自 path 段 `tasks/:id` 中的 :id。 */
function getInitialTaskDetailId(): number | null {
  const segs = getHashPathSegments();
  if (segs[0] !== 'tasks') return null;
  return getPathIdAt(segs, 1);
}

/** 帖子记录 id：来自 path 段 `todos/:id/posts/:rid` 中的 :rid。 */
function getInitialPostRecordId(): number | null {
  const segs = getHashPathSegments();
  if (segs[0] !== 'todos') return null;
  if (segs[2] !== 'posts') return null;
  return getPathIdAt(segs, 3);
}

/**
 * 是否为帖子页 URL：path 段 `todos/:id/posts/:rid`。
 * 帖子页返回来源（?from=task&taskId=）只在该 URL 形态下有意义，
 * todos 列表/详情即使带 ?from= 也应忽略，避免无关 query 污染状态。
 */
function isTodosPostUrl(): boolean {
  const segs = getHashPathSegments();
  return segs[0] === 'todos' && segs[2] === 'posts';
}

/**
 * 从 query 解析帖子页返回来源。
 * `from=task&taskId=<id>` → 从任务-讨论 tab 跳入，返回时回到该任务讨论 tab；
 * 否则默认 'todo'（返回事项详情）。taskId 非法（非正数）时视为无效来源。
 */
export function parsePostBackFrom(params: URLSearchParams): { from: 'todo' | 'task'; taskId: number | null } {
  if (params.get('from') !== 'task') return { from: 'todo', taskId: null };
  const raw = params.get('taskId');
  const n = Number(raw);
  const taskId = raw && Number.isFinite(n) && n > 0 ? n : null;
  return { from: taskId != null ? 'task' : 'todo', taskId };
}

function getInitialTab(): string | null {
  const params = getHashSearchParams();
  const tab = params.get('tab');
  return tab || null;
}

function getInitialWikiSlug(): string | null {
  const params = getHashSearchParams();
  return params.get('slug');
}

function getInitialBlackboardFile(): string | null {
  const params = getHashSearchParams();
  return params.get('file');
}

function getInitialProcessGuid(): string | null {
  const params = getHashSearchParams();
  return params.get('guid');
}

/**
 * 109：从 hash query 解析列表形态参数（`?view=` 原文，不校验合法性）。
 * 四个列表视图（todos/loops/tasks/processes）共用；合法性由各页面用
 * `pickListView` 按视图值域校验，非法值一律回退 localStorage 记忆。
 */
function getInitialListView(): string | null {
  const params = getHashSearchParams();
  const view = params.get('view');
  return view && view.trim() ? view.trim() : null;
}

/**
 * 109：在「URL 形态参数」与「localStorage 记忆」之间选择实际形态。
 * - raw 合法（在 allowed 内）→ 用 raw（URL 直达优先）；
 * - raw 为 null/非法 → 回退 fallback（localStorage 记忆），保持旧行为。
 * 抽成纯函数供四个列表页面共用，避免各写各的 includes 校验漂移。
 */
export function pickListView(raw: string | null, allowed: string[], fallback: string): string {
  return raw && allowed.includes(raw) ? raw : fallback;
}

/**
 * 029：从 hash query 解析工艺编辑器模式。
 * - `'new'` / `'edit'` → 对应编辑器态
 * - 缺失或非法值 → `'list'`（默认列表页）
 *
 * 只在 `processes` view 下生效；其他 view 即使带 `?processMode=new` 也忽略，
 * 避免跨视图串台（syncFromHash 里会按 view 过滤）。
 */
function getInitialProcessMode(): 'list' | 'new' | 'edit' {
  const params = getHashSearchParams();
  const mode = params.get('processMode');
  if (mode === 'new' || mode === 'edit') return mode;
  return 'list';
}

/** 导航参数：todos/loops 用 id/recordId 构造 path 段；其他视图用 query。 */
interface NavOpts {
  /** 详情 id（todos/loops 用，构造 path 段 /todos/:id）。 */
  id?: number | null;
  /** 帖子记录 id（todos 用，构造 /todos/:id/posts/:recordId）。 */
  recordId?: number | null;
  /**
   * 帖子页返回来源：`'task'` = 从任务-讨论 tab 跳入，帖子 URL 带 `?from=task&taskId=`，
   * 返回按钮据此回到该任务的讨论 tab；`'todo'`/缺省 = 返回事项详情。
   */
  postBack?: 'todo' | 'task' | null;
  /** postBack='task' 时返回的目标任务 id。 */
  postBackTaskId?: number | null;
  tab?: string | null;
  /**
   * 109：列表形态参数（仅 todos/loops/tasks/processes 四个列表视图生效）。
   * 值域随视图而异：todos=card|list|running、loops=list|kanban、
   * tasks=list|kanban|card、processes=mine|template。
   * 用独立 query key `view`，与 `tab`/`processMode` 互不冲突。
   */
  view?: string | null;
  workspace?: number | null;
  slug?: string | null;
  file?: string | null;
  /** 040：工艺模板 guid（processes 视图定位模板/编辑器目标；name 可重复后只能用 guid 寻址）。 */
  guid?: string | null;
  /**
   * 029：工艺编辑器模式。
   * - `'list'`（默认）：渲染工艺列表页
   * - `'new'`：渲染编辑器空白态，先弹元信息 Modal
   * - `'edit'`：渲染编辑器，加载 `guid` 对应 YAML
   *
   * 用独立 query key `processMode`，避免与其他 query 参数冲突。
   */
  processMode?: 'list' | 'new' | 'edit';
}

/**
 * 构造 hash URL。
 * - todos/loops 用 path 段（/todos、/todos/:id、/todos/:id/posts/:rid、/loops、/loops/:id）
 * - 其他视图用 query 参数（与 028 之前一致）
 *
 * 109：四个列表视图（todos/loops/tasks/processes）支持 `?view=` 列表形态参数，
 * 直达指定形态展示；详情/帖子页不携带该参数（形态只对列表页有意义）。
 */
export function buildHashUrl(view: View, opts?: NavOpts): string {
  // 事项命名空间：path 段驱动
  if (view === 'todos') {
    if (opts?.id != null && opts?.recordId != null) {
      // 帖子页 URL。从任务-讨论 tab 跳入时带返回来源 query（?from=task&taskId=），
      // 帖子页返回按钮据此回到对应任务的讨论 tab；否则默认返回事项详情。
      let url = `#/todos/${opts.id}/posts/${opts.recordId}`;
      if (opts?.postBack === 'task' && opts?.postBackTaskId != null) {
        url += `?from=task&taskId=${opts.postBackTaskId}`;
      }
      return url;
    }
    if (opts?.id != null) {
      return `#/todos/${opts.id}`;
    }
    // 列表页：带形态参数 → `#/todos?view=list` 直达指定形态（109）
    return appendListView(`#/todos`, opts?.view);
  }
  // 环路命名空间：path 段驱动
  if (view === 'loops') {
    if (opts?.id != null) {
      return `#/loops/${opts.id}`;
    }
    return appendListView(`#/loops`, opts?.view);
  }
  // 任务命名空间：path 段驱动，与 todos/loops 一致。
  // 支持 ?tab= query：帖子页返回任务-讨论 tab 时据此恢复 Tabs 选中态（对齐 Settings 页模式）。
  // 109：列表页额外支持 ?view= 形态参数；详情页只带 tab，不带 view。
  if (view === 'tasks') {
    if (opts?.id != null) {
      const qs = typeof opts.tab === 'string' && opts.tab.trim() ? `?tab=${opts.tab}` : '';
      return `#/tasks/${opts.id}${qs}`;
    }
    const params = new URLSearchParams();
    if (typeof opts?.tab === 'string' && opts.tab.trim()) params.set('tab', opts.tab);
    // trim 后写入：与 todos/loops 的 appendListView 口径一致，避免「未 trim 值进 URL、
    // syncFromHash 又 trim 复原」导致不同 useViewState 实例的 listView 短暂不一致（review 修复）。
    if (typeof opts?.view === 'string' && opts.view.trim()) params.set('view', opts.view.trim());
    const qs = params.toString();
    return qs ? `#/tasks?${qs}` : `#/tasks`;
  }
  // 其他视图保持 query 参数风格
  const path = `/${view}`;
  const params = new URLSearchParams();
  if (typeof opts?.tab === 'string' && opts.tab.trim()) params.set('tab', opts.tab);
  if (view === 'wiki') {
    // wiki 视图需要 workspace 和 slug 来定位文件
    if (opts?.workspace != null) params.set('workspace', String(opts.workspace));
    if (opts?.slug) params.set('slug', opts.slug);
  }
  if (view === 'blackboard' && opts?.file) {
    // blackboard 视图的 file 参数标识当前查看的文件
    params.set('file', opts.file);
  }
  if (view === 'processes' && opts?.guid) {
    // processes 视图的 guid 参数定位工艺模板，用于「环路 → 来源工艺」回跳自动开详情
    params.set('guid', opts.guid);
  }
  // 029：工艺编辑器模式。list（默认）渲染列表页，new/edit 渲染编辑器。
  // 用独立 query key `processMode` 避免与看板 `mode` 冲突。
  if (view === 'processes' && opts?.processMode && opts.processMode !== 'list') {
    params.set('processMode', opts.processMode);
  }
  // 109：工艺列表形态参数（mine|template）。编辑器态（new/edit）不携带，
  // 避免编辑器 URL 上挂着与编辑无关的形态参数；list/缺省（列表态）才追加。
  if (view === 'processes' && opts?.processMode !== 'new' && opts?.processMode !== 'edit'
    && typeof opts?.view === 'string' && opts.view.trim()) {
    // trim 后写入：与 appendListView/tasks 分支口径一致（review 修复）。
    params.set('view', opts.view.trim());
  }
  const qs = params.toString();
  return qs ? `#${path}?${qs}` : `#${path}`;
}

/**
 * 109：给列表页基础 URL 追加 `?view=` 形态参数（todos/loops 用）。
 * 无参数/空白参数返回原 URL，避免生成 `#/todos?` 这类带空 query 的 URL。
 */
function appendListView(base: string, view?: string | null): string {
  if (typeof view !== 'string' || !view.trim()) return base;
  return `${base}?view=${encodeURIComponent(view.trim())}`;
}

const VIEW_TO_NAV_KEY: Record<View, string> = {
  todos: 'todos',      // 事项命名空间统一高亮「事项」导航项
  loops: 'loops',
  tasks: 'tasks',
  processes: 'processes',
  dashboard: 'dashboard',
  blackboard: 'blackboard',
  settings: 'settings',
  runtime: 'settings_runtime',
  skills: 'settings_skills',
  workspaces: 'settings_workspaces',
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
  // 028：todoDetailId / loopDetailId / taskDetailId / postRecordId 来自 path 段，刷新可恢复
  const [todoDetailId, setTodoDetailId] = useState<number | null>(getInitialTodoDetailId);
  const [loopDetailId, setLoopDetailId] = useState<number | null>(getInitialLoopDetailId);
  const [taskDetailId, setTaskDetailId] = useState<number | null>(getInitialTaskDetailId);
  const [postRecordId, setPostRecordId] = useState<number | null>(getInitialPostRecordId);
  // 帖子页返回来源：仅帖子页 URL 解析 ?from=task，其他视图一律 'todo'（回事项详情）
  const [postBackFrom, setPostBackFrom] = useState<'todo' | 'task'>(
    () => (isTodosPostUrl() ? parsePostBackFrom(getHashSearchParams()).from : 'todo'),
  );
  const [postBackTaskId, setPostBackTaskId] = useState<number | null>(
    () => (isTodosPostUrl() ? parsePostBackFrom(getHashSearchParams()).taskId : null),
  );
  const [activeTab, setActiveTab] = useState<string | null>(getInitialTab);
  const [wikiSlug, setWikiSlug] = useState<string | null>(getInitialWikiSlug);
  const [blackboardFile, setBlackboardFile] = useState<string | null>(getInitialBlackboardFile);
  const [processGuid, setProcessGuid] = useState<string | null>(getInitialProcessGuid);
  // 109：列表形态参数（?view= 原文）。四个列表视图的页面据此直达指定形态；
  // 与 localStorage 的关系：URL 优先，无参数/非法值回退 localStorage（页面侧 pickListView）。
  const [listView, setListView] = useState<string | null>(getInitialListView);
  // 029：工艺编辑器模式。list（默认）= 列表页，new/edit = 编辑器态。
  // 与 processGuid 配套：edit 模式下 processGuid 指向被编辑的工艺 guid（040 起按 guid 寻址）。
  const [processMode, setProcessMode] = useState<'list' | 'new' | 'edit'>(getInitialProcessMode);

  // setters 集中传入 syncFromHash，避免每个回调都重复一遍
  const setters = {
    setActiveView, setTodoDetailId, setLoopDetailId, setTaskDetailId, setPostRecordId,
    setPostBackFrom, setPostBackTaskId,
    setActiveTab, setWikiSlug, setBlackboardFile, setProcessGuid, setProcessMode, setListView,
  };

  const pushUrl = useCallback((view: View, opts?: NavOpts) => {
    const hashUrl = buildHashUrl(view, opts);
    window.history.pushState(null, '', hashUrl);
    // 写入 history 后立即同步本实例 state，避免等事件往返一帧
    syncStateFromOptions(view, opts, setters);
    // 广播给其他 useViewState 实例（如 App 根、其他嵌套组件）
    navEventTarget.dispatchEvent(new Event(NAV_CHANGE_EVENT));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const replaceUrl = useCallback((view: View, opts?: NavOpts) => {
    const hashUrl = buildHashUrl(view, opts);
    window.history.replaceState(null, '', hashUrl);
    syncStateFromOptions(view, opts, setters);
    navEventTarget.dispatchEvent(new Event(NAV_CHANGE_EVENT));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    // 统一处理 popstate（浏览器后退/前进）与 nav-change（其他实例 pushUrl/replaceUrl）
    const onNavChange = () => {
      syncFromHash(setters);
    };
    window.addEventListener('popstate', onNavChange);
    navEventTarget.addEventListener(NAV_CHANGE_EVENT, onNavChange);
    return () => {
      window.removeEventListener('popstate', onNavChange);
      navEventTarget.removeEventListener(NAV_CHANGE_EVENT, onNavChange);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
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

  /**
   * 返回上一级：优先用 history.back() 恢复浏览器历史，保持移动端返回语义；
   * 无历史可退时（直接刷新进入详情）按层级 fallback：
   *   - 帖子页 /#/todos/:id/posts/:rid → /#/todos/:id（父事项详情）
   *   - 事项详情 /#/todos/:id          → /#/todos（列表）
   *   - 环路详情 /#/loops/:id          → /#/loops（列表）
   *   - 其他视图                       → 该视图根路径
   */
  const backToList = useCallback(() => {
    // 帖子页返回：区分来源——从任务-讨论 tab 跳入则回到该任务的讨论 tab，
    // 否则回父事项详情（保留列表状态恢复策略）。
    // 统一用 replaceUrl：与桌面端 TodoPostPage onBack 一致，避免 pushUrl 在
    // 「帖子页 → 来源页」之间留下多余 history 条目（浏览器后退不会回到帖子页）。
    if (activeView === 'todos' && todoDetailId != null && postRecordId != null) {
      if (postBackFrom === 'task' && postBackTaskId != null) {
        replaceUrl('tasks', { id: postBackTaskId, tab: 'discussion' });
        return;
      }
      replaceUrl('todos', { id: todoDetailId });
      return;
    }
    // 事项/环路/任务详情返回列表
    if (activeView === 'todos') { replaceUrl('todos'); return; }
    if (activeView === 'loops') { replaceUrl('loops'); return; }
    if (activeView === 'tasks') { replaceUrl('tasks'); return; }
    pushUrl(activeView);
  }, [activeView, todoDetailId, postRecordId, postBackFrom, postBackTaskId, pushUrl, replaceUrl]);

  // MobileHeader 需要知道当前是否处于「详情态」以决定返回按钮显隐；
  // 由 todoDetailId/loopDetailId/taskDetailId 派生，保持兼容旧 activePanel: 'detail' | 'list' 接口。
  // 帖子页同样视为 detail 态，MobileHeader 返回按钮可触发 backToList 回到父事项详情。
  const activePanel: Panel = (activeView === 'todos' && todoDetailId != null)
    || (activeView === 'loops' && loopDetailId != null)
    || (activeView === 'tasks' && taskDetailId != null)
    ? 'detail'
    : 'list';

  return {
    activeView,
    // 028：详情 id（path 段驱动）
    todoDetailId,
    loopDetailId,
    taskDetailId,
    postRecordId,
    // 帖子页返回来源（from=task 时返回对应任务讨论 tab）
    postBackFrom,
    postBackTaskId,
    // 派生：仅用于 MobileHeader 返回按钮显隐
    activePanel,
    activeTab,
    wikiSlug,
    blackboardFile,
    processGuid,
    // 109：列表形态参数（?view= 原文）。仅 todos/loops/tasks/processes 四个列表视图消费，
    // 其他视图恒为 null；页面用 pickListView(raw, allowed, localStorageFallback) 得出实际形态。
    listView,
    // 029：工艺编辑器模式。list（默认）= 列表页，new/edit = 编辑器态。
    // ProcessPage 根据 mode 分流：list 渲染列表，new/edit 渲染 ProcessEditor。
    processMode,
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
    setTaskDetailId: (id: number | null) => void;
    setPostRecordId: (id: number | null) => void;
    setPostBackFrom: (f: 'todo' | 'task') => void;
    setPostBackTaskId: (id: number | null) => void;
    setActiveTab: (t: string | null) => void;
    setWikiSlug: (s: string | null) => void;
    setBlackboardFile: (f: string | null) => void;
    setProcessGuid: (g: string | null) => void;
    setProcessMode: (m: 'list' | 'new' | 'edit') => void;
    setListView: (v: string | null) => void;
  },
): void {
  const {
    setActiveView, setTodoDetailId, setLoopDetailId, setTaskDetailId, setPostRecordId,
    setPostBackFrom, setPostBackTaskId,
    setActiveTab, setWikiSlug, setBlackboardFile, setProcessGuid, setProcessMode, setListView,
  } = setters;
  setActiveView(view);
  // todos: id+recordId 表示帖子页；仅 id 表示详情；都没有表示列表
  setTodoDetailId(view === 'todos' ? (opts?.id ?? null) : null);
  setPostRecordId(view === 'todos' ? (opts?.recordId ?? null) : null);
  // 帖子页返回来源只在 todos 帖子 URL 上有意义；其他视图一律清空（回事项详情默认分支）
  setPostBackFrom(view === 'todos' && opts?.postBack === 'task' ? 'task' : 'todo');
  setPostBackTaskId(view === 'todos' && opts?.postBack === 'task' ? (opts?.postBackTaskId ?? null) : null);
  setLoopDetailId(view === 'loops' ? (opts?.id ?? null) : null);
  setTaskDetailId(view === 'tasks' ? (opts?.id ?? null) : null);
  setActiveTab(opts?.tab ?? null);
  setWikiSlug(view === 'wiki' ? (opts?.slug ?? null) : null);
  setBlackboardFile(view === 'blackboard' ? (opts?.file ?? null) : null);
  setProcessGuid(view === 'processes' ? (opts?.guid ?? null) : null);
  // 029：工艺编辑器模式仅在 processes view 下生效，其他 view 一律 fallback 到 'list'。
  // 避免跨视图串台：如 /#/dashboard?processMode=new 不应让 dashboard 渲染编辑器。
  setProcessMode(view === 'processes' ? (opts?.processMode ?? 'list') : 'list');
  // 109：列表形态参数仅在四个列表视图下同步，其他视图一律清空，
  // 避免跨视图串台（如 /#/dashboard?view=list 不应影响 dashboard）。
  setListView(view === 'todos' || view === 'loops' || view === 'tasks' || view === 'processes'
    ? (opts?.view ?? null)
    : null);
}

/**
 * 从当前 hash 重新解析所有状态并同步到 setters。
 * 用于 popstate（浏览器后退/前进）和 nav-change 事件（其他实例 pushUrl/replaceUrl）。
 * 抽成纯函数避免 useViewState 中 useEffect 重复实现。
 */
function syncFromHash(setters: {
  setActiveView: (v: View) => void;
  setTodoDetailId: (id: number | null) => void;
  setLoopDetailId: (id: number | null) => void;
  setTaskDetailId: (id: number | null) => void;
  setPostRecordId: (id: number | null) => void;
  setPostBackFrom: (f: 'todo' | 'task') => void;
  setPostBackTaskId: (id: number | null) => void;
  setActiveTab: (t: string | null) => void;
  setWikiSlug: (s: string | null) => void;
  setBlackboardFile: (f: string | null) => void;
  setProcessGuid: (g: string | null) => void;
  setProcessMode: (m: 'list' | 'new' | 'edit') => void;
  setListView: (v: string | null) => void;
}): void {
  const segments = getHashPathSegments();
  const view = parseViewFromSegments(segments);
  const params = getHashSearchParams();
  const tab = params.get('tab');
  const slug = params.get('slug');
  const file = params.get('file');
  const guid = params.get('guid');
  // 029：工艺编辑器模式。非法值（如 ?processMode=foo）一律 fallback 到 'list'。
  const rawProcessMode = params.get('processMode');
  const processMode: 'list' | 'new' | 'edit' =
    rawProcessMode === 'new' || rawProcessMode === 'edit' ? rawProcessMode : 'list';
  // 109：列表形态参数原文（合法性由页面侧 pickListView 校验）。
  const rawView = params.get('view');
  const listView = rawView && rawView.trim() ? rawView.trim() : null;

  setters.setActiveView(view);
  // todos/loops/tasks 详情 id 仅在对应 view 下提取，避免跨视图串台
  setters.setTodoDetailId(view === 'todos' ? getPathIdAt(segments, 1) : null);
  setters.setPostRecordId(view === 'todos' && segments[2] === 'posts' ? getPathIdAt(segments, 3) : null);
  // 帖子页返回来源：仅 todos 帖子 URL 解析 ?from=task&taskId=，
  // 列表/详情/其他视图一律清空（回事项详情默认分支）
  const isPost = view === 'todos' && segments[2] === 'posts';
  const postBack = parsePostBackFrom(params);
  setters.setPostBackFrom(isPost ? postBack.from : 'todo');
  setters.setPostBackTaskId(isPost ? postBack.taskId : null);
  setters.setLoopDetailId(view === 'loops' ? getPathIdAt(segments, 1) : null);
  setters.setTaskDetailId(view === 'tasks' ? getPathIdAt(segments, 1) : null);
  setters.setActiveTab(tab || null);
  setters.setWikiSlug(view === 'wiki' ? (slug || null) : null);
  setters.setBlackboardFile(view === 'blackboard' ? (file || null) : null);
  setters.setProcessGuid(view === 'processes' ? (guid || null) : null);
  // 029：工艺编辑器模式仅在 processes view 下生效，其他 view 即使带 ?processMode=new 也忽略。
  setters.setProcessMode(view === 'processes' ? processMode : 'list');
  // 109：列表形态参数仅在四个列表视图下生效，其他 view 即使带 ?view= 也忽略（避免跨视图串台）。
  setters.setListView(view === 'todos' || view === 'loops' || view === 'tasks' || view === 'processes'
    ? listView
    : null);
}
