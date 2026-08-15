// 主应用入口组件。

import { useState, useEffect, useCallback, useMemo, useRef, lazy, Suspense } from 'react';
import { ConfigProvider, Layout, App as AntApp, Drawer, Spin } from 'antd';
import { TODO_LIST_REFRESH_EVENT } from './constants';
import { AppProvider, useApp } from './hooks/useApp';
import { useIsMobile } from './hooks/useIsMobile';
import { useExecutionEvents } from './hooks/useExecutionEvents';
import { useViewState, viewToNavKey, type View } from './hooks/useViewState';
import { ThemeProvider, useTheme } from '@/hooks/useTheme';
import { ConsolePanelProvider, useConsolePanel } from '@/hooks/useConsolePanel';
// 028-列表详情独立路由：todos/loops 用 path 段区分列表/详情，旧 ItemsPage/TodoPage/LoopPage 已删除
// 091：首屏代码分割——事项主路径（TodoListPage/TodoDetailPage/Dashboard）保持静态加载
// 保首屏速度，其余页面级组件改为 React.lazy，各自拆成独立 chunk，按视图按需加载。
// 这样 monaco / @xyflow 等重型依赖随对应页面一起移出主 bundle（index.js 显著缩小）。
import { TodoListPage } from '@/components/todo-list/TodoListPage';
import { TodoDetailPage } from '@/components/TodoDetailPage';
import { Dashboard } from '@/components/Dashboard';
// 命名导出组件需包一层 .then 取 default，React.lazy 只认 default 导出。
const LoopListPage = lazy(() => import('@/components/loop-list').then(m => ({ default: m.LoopListPage })));
const LoopDetailPage = lazy(() => import('@/components/LoopDetailPage').then(m => ({ default: m.LoopDetailPage })));
const TodoPostPage = lazy(() => import('@/components/todo-post').then(m => ({ default: m.TodoPostPage })));
const ProcessPage = lazy(() => import('@/components/ProcessPage').then(m => ({ default: m.ProcessPage })));
const TasksPage = lazy(() => import('@/components/tasks/TasksPage').then(m => ({ default: m.TasksPage })));
const TaskDetailPage = lazy(() => import('@/components/tasks/TaskDetailPage').then(m => ({ default: m.TaskDetailPage })));
const ConceptNavPage = lazy(() => import('@/components/onboarding/ConceptNavPage').then(m => ({ default: m.ConceptNavPage })));
const SettingsPage = lazy(() => import('@/components/SettingsPage').then(m => ({ default: m.SettingsPage })));
const SkillsPanel = lazy(() => import('@/components/SkillsPanel').then(m => ({ default: m.SkillsPanel })));
const WorkspacesPanel = lazy(() => import('@/components/settings/WorkspacesPanel').then(m => ({ default: m.WorkspacesPanel })));
const ExecutorsPanel = lazy(() => import('@/components/settings/ExecutorsPanel').then(m => ({ default: m.ExecutorsPanel })));
const ExpertsPanel = lazy(() => import('@/components/settings/ExpertsPanel').then(m => ({ default: m.ExpertsPanel })));
const BlackboardPage = lazy(() => import('@/components/BlackboardPage').then(m => ({ default: m.BlackboardPage })));
const MessagesPage = lazy(() => import('@/components/MessagesPage').then(m => ({ default: m.MessagesPage })));
const AssistantManagementPage = lazy(() => import('@/components/assistant-management/AssistantManagementPage').then(m => ({ default: m.AssistantManagementPage })));
const WikiViewPage = lazy(() => import('@/components/WikiViewPage').then(m => ({ default: m.WikiViewPage })));
import { ExecutionPanel } from './components/ExecutionPanel';
import { TodoDrawer } from './components/TodoDrawer';
import { SmartCreateModal } from './components/SmartCreateModal';
import { QuickCaptureModal } from './components/QuickCaptureModal';
import { LeftRail, type LeftRailKey } from './components/shell/LeftRail';
import { MobileHeader } from './components/shell/MobileHeader';
import { FloatingActionButton } from '@/components/shell/FloatingActionButton';
import { WikiChatFloatingWindow, type WikiChatMode } from '@/components/WikiChatFloatingWindow';
import { HelpPage } from '@/help/HelpPage';
import { viewToPageId, findHelpPage } from '@/help/useHelpContent';

import { EXECUTION_PANEL, LEFT_RAIL_WIDTH } from './constants';
import * as db from './utils/database';
import { loadDefaultExecutor } from '@/utils/executors';
import type { Config } from './types';
import zhCN from 'antd/locale/zh_CN';
import './App.css';

const { Content } = Layout;

// 091：lazy 页面加载占位——居中 Spin，避免切换到按需加载的页面时短暂白屏。
// 静态页面（Dashboard/TodoList/TodoDetail）不触发 Suspense，只有 lazy 页面首访时短暂出现。
function LazyFallback() {
  return (
    <div style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%' }}>
      <Spin />
    </div>
  );
}

function AppContent() {
  const { state, dispatch, clearSelection } = useApp();
  // 028：路由统一为 /#/todos + /#/todos/:id + /#/todos/:id/posts/:rid + /#/loops + /#/loops/:id
  // todoDetailId / loopDetailId / postRecordId 均来自 path 段，刷新可恢复
  const { activeView, todoDetailId, loopDetailId, taskDetailId, postRecordId, postBackFrom, postBackTaskId, activePanel, processGuid, processMode, showView, pushUrl, replaceUrl, backToList } = useViewState();
  const { themeMode, toggleTheme } = useTheme();
  // 底部执行日志面板的显隐开关：来自设置-界面显示，关掉后即使有运行中任务也不渲染面板。
  const { visible: consolePanelVisible, setVisible: setConsolePanelVisible } = useConsolePanel();
  // 临时关闭态：面板上的「临时关闭」按钮置位，仅本轮任务期间隐藏，不写 localStorage。
  // 与 consolePanelVisible 区分：永久关闭=setVisible(false) 落盘；临时关闭=会话内 dismiss。
  const [consolePanelDismissed, setConsolePanelDismissed] = useState(false);

  const [todoModalOpen, setTodoModalOpen] = useState(false);
  // 028：列表页 onEditTodo 触发时设置 editingTodo，TodoDrawer 切到编辑模式（todo != null）
  // 新建模式（顶部「新建」按钮）时 editingTodo 保持 null，TodoDrawer 走创建分支
  const [editingTodo, setEditingTodo] = useState<import('@/types').Todo | null>(null);
  const [smartCreateOpen, setSmartCreateOpen] = useState(false);
  const [quickCaptureOpen, setQuickCaptureOpen] = useState(false);
  // 帮助弹窗开关 + 初始选中 pageId：由 LeftRail 帮助按钮触发，关闭即全部关闭
  const [helpModalOpen, setHelpModalOpen] = useState(false);
  const [helpInitialPageId, setHelpInitialPageId] = useState<string | undefined>(undefined);
  const [wikiChatMode, setWikiChatMode] = useState<WikiChatMode>(() => {
    try {
      const saved = localStorage.getItem('wiki_chat_mode') as WikiChatMode | null;
      if (saved && ['minimized', 'side', 'maximized'].includes(saved)) return saved;
    } catch {}
    return 'minimized';
  });
  const [navDrawerOpen, setNavDrawerOpen] = useState(false);
  const [railCollapsed, setRailCollapsed] = useState(() => {
    try {
      const saved = localStorage.getItem('ntd_left_rail_collapsed');
      if (saved === 'true') return true;
      if (saved === 'false') return false;
      return true;
    } catch {
      return true;
    }
  });
  const [appConfig, setAppConfig] = useState<Config | null>(null);
  // 028：loopDetailId 已从 URL path 段派生，不再需要 selectedLoopId React state
  const [loopUpdateCount, setLoopUpdateCount] = useState(0);
  // 刷新回调：触发 loopUpdateCount 递增，LoopListPage/LoopDetailPage 通过 useEffect 监听该值自动重载

  const navKey = useMemo<LeftRailKey>(() => {
    return viewToNavKey(activeView) as LeftRailKey;
  }, [activeView]);
  const isMobile = useIsMobile();

  // 028：移动端 panel 由 useViewState 派生（todoDetailId/loopDetailId != null 时为 'detail'），
  // 其他视图移动端默认 'detail'。原 effectiveMobilePanel 已合并到 activePanel。

  const [panelCollapsed, setPanelCollapsed] = useState(() => {
    try {
      return localStorage.getItem('execution_panel_collapsed') === 'true';
    } catch {
      return false;
    }
  });

  useExecutionEvents();

  const hasRunningTasks = Object.keys(state.runningTasks).length > 0;

  // 临时关闭的撤销时机：
  // 1) 新一轮任务开始（running 从无到有）——让面板随新任务重新出现，符合「临时」语义。
  const prevHadRunningRef = useRef(false);
  useEffect(() => {
    if (!prevHadRunningRef.current && hasRunningTasks) {
      setConsolePanelDismissed(false);
    }
    prevHadRunningRef.current = hasRunningTasks;
  }, [hasRunningTasks]);

  // 2) 用户在设置里重新开启面板——清除上一轮遗留的临时关闭态，确保开关闭合后立刻可见。
  const prevVisibleRef = useRef(consolePanelVisible);
  useEffect(() => {
    if (!prevVisibleRef.current && consolePanelVisible) {
      setConsolePanelDismissed(false);
    }
    prevVisibleRef.current = consolePanelVisible;
  }, [consolePanelVisible]);

  // 面板真正隐藏的条件：永久开关关闭，或本轮被临时关闭。两者任一为真都不渲染、不占高度。
  const consolePanelHidden = !consolePanelVisible || consolePanelDismissed;
  // 隐藏时面板高度归零，主内容区不再留出底部避让空间；否则按折叠/展开状态给出高度。
  const panelHeight = !consolePanelHidden && hasRunningTasks
    ? (panelCollapsed ? EXECUTION_PANEL.collapsed : EXECUTION_PANEL.expanded)
    : 0;

  useEffect(() => {
    db.getConfig().then(setAppConfig).catch(() => {
      // 配置加载失败时使用默认值，非关键路径不阻塞主流程
    });
    // 启动时加载默认执行器配置，供创建 todo、快速捕获等场景使用
    loadDefaultExecutor().catch(() => {
      // 加载失败时内部会回退到常量值，这里静默处理
    });
  }, []);

  // 全局快捷键
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        setQuickCaptureOpen(true);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  // URL → React state 恢复：todos 视图需要让 selectedTodoId 与 URL 中的 todoDetailId 保持一致。
  // 028：loopDetailId 已直接由 useViewState 派生，App.tsx 不再维护 selectedLoopId state。
  // todos 视图不再校验 todo 是否在 state.todos 列表中：TodoDetail 已改为独立请求获取，
  // 即使目标 todo 不在当前 workspace 桶也能正常加载（跨 workspace 直达场景）。
  useEffect(() => {
    if (state.loading) return;
    if (activeView === 'todos') {
      // 直接用 URL 中的 todoDetailId 同步选中态，TodoDetail 会自己发请求加载数据。
      // id 缺失时（如 /#/todos 列表页）清空选中，避免详情页残留。
      dispatch({ type: 'SELECT_TODO', payload: todoDetailId ?? null });
    } else {
      // 非 todos 视图清空 todo 选中态，防止跨视图状态混淆
      dispatch({ type: 'SELECT_TODO', payload: null });
    }
  }, [activeView, todoDetailId, state.loading, dispatch]);

  // 028：选中事项 → 跳转到事项详情独立页 `/#/todos/:id`。
  // 立即 dispatch 让 TodoDetail 内部响应；URL 同步由 pushUrl 完成，history.back 可回到列表
  const handleSelectTodo = useCallback((todoId: string | number | null) => {
    if (todoId == null) return;
    dispatch({ type: 'SELECT_TODO', payload: Number(todoId) });
    pushUrl('todos', { id: Number(todoId) });
  }, [dispatch, pushUrl]);

  const handleOpenPost = useCallback((todoId: number, recordId: number) => {
    // 028：帖子页用 path 段 /#/todos/:id/posts/:rid，刷新可恢复
    pushUrl('todos', { id: todoId, recordId });
  }, [pushUrl]);

  // 028：选中环路 → 跳转到环路详情独立页 `/#/loops/:id`
  const handleSelectLoop = useCallback((loopId: number) => {
    clearSelection();
    pushUrl('loops', { id: loopId });
  }, [clearSelection, pushUrl]);

  // 091：抽出稳定 handler，替代 TodoListPage 的 inline 箭头（inline 每次渲染新建函数，
  // 会让依赖引用相等性的子组件 memo 失效）。
  const handleCreateTodo = useCallback(() => {
    // 新建模式：editingTodo 保持 null，TodoDrawer 走创建分支
    setEditingTodo(null);
    setTodoModalOpen(true);
  }, []);

  const handleEditTodo = useCallback((todo: import('@/types').Todo) => {
    // 编辑模式：设置 editingTodo，TodoDrawer 切到编辑分支
    setEditingTodo(todo);
    setTodoModalOpen(true);
  }, []);

  // 跳转来源工艺详情：环路详情「来源工艺」行的目标。
  // 040：携带 guid 参数，ProcessPage 据此自动打开该工艺的详情 Modal。
  const handleOpenProcess = useCallback((templateGuid: string) => {
    clearSelection();
    pushUrl('processes', { guid: templateGuid });
  }, [clearSelection, pushUrl]);

  const handleSmartCreateSubmitted = () => {
    // 056：全局 todos 桶已删除，创建成功后只需通知列表页重拉当前页
    window.dispatchEvent(new Event(TODO_LIST_REFRESH_EVENT));
  };

  const handleShowView = useCallback((view: View) => {
    clearSelection();
    showView(view);
  }, [clearSelection, showView]);

  const showSettings = useCallback((tab: string | null) => {
    clearSelection();
    showView('settings', { tab });
  }, [clearSelection, showView]);

  const showStandaloneSettingsPanel = useCallback((view: View) => {
    clearSelection();
    pushUrl(view);
  }, [clearSelection, pushUrl]);

  // 028：左侧导航点击「事项」/「环路」直接 replaceUrl 到对应列表页（path 段无 id）
  // 不再需要 forcedListMode，因为 todos 和 loops 是独立 View 类型，列表/详情由 path 段区分
  const showListSection = useCallback((mode: 'item' | 'loop') => {
    clearSelection();
    replaceUrl(mode === 'loop' ? 'loops' : 'todos');
  }, [replaceUrl, clearSelection]);

  const handleRailSelect = useCallback((key: LeftRailKey) => {
    setNavDrawerOpen(false);
    if (key === 'todos') { showListSection('item'); return; }
    if (key === 'loops') { showListSection('loop'); return; }
    if (key === 'processes') { handleShowView('processes'); return; }
    if (key === 'tasks') { handleShowView('tasks'); return; }
    // 概念导航首页：独立视图挂载，workspace 透传给子组件拉数据快照。
    if (key === 'onboarding') { handleShowView('onboarding'); return; }
    // 消息页：作为独立视图挂载，workspace 由左上角 WorkspaceSwitcher 联动传入。
    if (key === 'messages') { handleShowView('messages'); return; }
    if (key === 'dashboard') { handleShowView('dashboard'); return; }
    if (key === 'blackboard') { handleShowView('blackboard'); return; }
    if (key === 'settings') { showSettings(null); return; }
    if (key === 'settings_workspaces') { showStandaloneSettingsPanel('workspaces'); return; }
    if (key === 'settings_skills') { showStandaloneSettingsPanel('skills'); return; }
    if (key === 'settings_executors') { showStandaloneSettingsPanel('executors'); return; }
    if (key === 'settings_experts') { showStandaloneSettingsPanel('experts'); return; }
    if (key === 'settings_bots') { showStandaloneSettingsPanel('bots'); return; }
  }, [handleShowView, showListSection, showSettings, showStandaloneSettingsPanel]);

  return (
    <Layout style={{ height: '100vh', flexDirection: isMobile ? 'column' : 'row' }}>
      {/* Mobile Header */}
      {isMobile && (
        <MobileHeader
          activeView={activeView}
          activePanel={activePanel}
          onBackToList={backToList}
          onOpenNav={() => setNavDrawerOpen(true)}
        />
      )}

      {/* FAB (统一浮动操作按钮) */}
      <FloatingActionButton
        onOpenQuickCapture={() => setQuickCaptureOpen(true)}
        onOpenWikiChat={() => setWikiChatMode(isMobile ? 'maximized' : 'side')}
      />

      {/* Left Rail */}
      {!isMobile && (
        <div
          className="ntd-left-rail-slot"
          style={{
            width: railCollapsed ? LEFT_RAIL_WIDTH.collapsed : LEFT_RAIL_WIDTH.expanded,
            height: `calc(100vh - ${panelHeight}px)`,
          }}
        >
          <LeftRail
            activeKey={navKey}
            onSelect={handleRailSelect}
            collapsed={railCollapsed}
            onToggleCollapsed={() => {
              const next = !railCollapsed;
              setRailCollapsed(next);
              try { localStorage.setItem('ntd_left_rail_collapsed', String(next)); } catch {}
            }}
            workspace={state.selectedWorkspace}
            onWorkspaceChange={(workspace) => {
              dispatch({ type: 'SELECT_WORKSPACE', payload: workspace });
            }}
            themeMode={themeMode}
            toggleTheme={toggleTheme}
            onOpenHelp={() => {
              // 内嵌大弹窗打开帮助，直接跳到当前视图对应的具体帮助页
              // 而非每次都落帮助首页；找不到对应页时回退到概览首页
              const pageId = viewToPageId(activeView, todoDetailId != null || loopDetailId != null || taskDetailId != null);
              setHelpInitialPageId(findHelpPage(pageId) ? pageId : '_overview');
              setHelpModalOpen(true);
            }}
          />
        </div>
      )}

      {/* Main Content */}
      <Layout
        style={{
          flex: 1,
          minWidth: 0,
          position: 'relative',
        }}
      >
        <Content
          style={{
            flex: 1,
            minWidth: 0,
            display: 'flex',
            flexDirection: isMobile ? 'column' : 'row',
            padding: isMobile ? 0 : 12,
            paddingBottom: isMobile ? 0 : 12 + panelHeight,
            gap: isMobile ? 0 : 12,
            height: `calc(100vh - ${panelHeight}px)`,
            overflow: 'hidden',
            transition: 'height 0.3s ease, padding-bottom 0.3s ease',
          }}
        >
          {/* 091：所有页面级组件在此挂载；lazy 页面首访时由 Suspense 兜底显示 LazyFallback。 */}
          <Suspense fallback={<LazyFallback />}>
          {/* 帖子详情页（URL: /#/todos/:id/posts/:rid） */}
          {activeView === 'todos' && postRecordId != null && todoDetailId != null && (
            <TodoPostPage
              todoId={todoDetailId}
              recordId={postRecordId}
              onBack={() => {
                // 帖子页返回区分来源：从任务-讨论 tab 跳入 → 回到该任务的讨论 tab（?tab=discussion）；
                // 否则回父事项详情（旧逻辑）。用 replaceUrl 不污染浏览器历史。
                if (postBackFrom === 'task' && postBackTaskId != null) {
                  replaceUrl('tasks', { id: postBackTaskId, tab: 'discussion' });
                } else {
                  replaceUrl('todos', { id: todoDetailId });
                }
              }}
            />
          )}

          {/* 事项详情独立页（URL: /#/todos/:id） */}
          {/* 028：详情独立路由，不复用旧双栏；TodoDetail 内部读 state.selectedTodoId，App.tsx 同步即可 */}
          {activeView === 'todos' && todoDetailId != null && postRecordId == null && (
            <TodoDetailPage
              todoId={todoDetailId}
              onBack={() => backToList()}
              onOpenPost={handleOpenPost}
            />
          )}

          {/* 事项列表页（URL: /#/todos，卡片/列表形态切换由 TodoListPage 内部管理） */}
          {activeView === 'todos' && todoDetailId == null && postRecordId == null && (
            <TodoListPage
              onSelectTodo={handleSelectTodo}
              onSelectLoop={handleSelectLoop}
              onOpenCreateModal={handleCreateTodo}
              onEditTodo={handleEditTodo}
            />
          )}

          {/* 环路详情独立页（URL: /#/loops/:id） */}
          {activeView === 'loops' && loopDetailId != null && (
            <LoopDetailPage
              loopId={loopDetailId}
              workspaceId={state.selectedWorkspace}
              tags={state.tags}
              onBack={() => backToList()}
              onOpenProcess={handleOpenProcess}
              onSelectTodo={handleSelectTodo}
              onLoopChanged={() => setLoopUpdateCount(c => c + 1)}
            />
          )}

          {/* 环路列表页（URL: /#/loops） */}
          {/* 044：环路仅由工艺 install/upgrade 产生，列表页不再有「新建环路」入口 */}
          {activeView === 'loops' && loopDetailId == null && (
            <LoopListPage
              onSelectLoop={handleSelectLoop}
              onLoopChanged={() => setLoopUpdateCount(c => c + 1)}
              loopUpdateCount={loopUpdateCount}
            />
          )}

          {/* 非事项/环路视图（事项页单独在上块渲染，不能落到 Dashboard 兜底） */}
          {activeView !== 'todos' && activeView !== 'loops' && (
            <div
              style={{
                flex: 1,
                minWidth: 0,
                height: '100%',
                overflow: 'hidden',
              }}
            >
              {activeView === 'skills' ? (
                <SkillsPanel />
              ) : activeView === 'workspaces' ? (
                // 工作空间管理页：「智能助手配置」入口已迁移为独立菜单，
                // 这里注入回调：切视图到 messages 并联动 workspace id，实现从管理工作空间下钻到消息页。
                <WorkspacesPanel
                  onOpenMessages={(workspaceId) => {
                    dispatch({ type: 'SELECT_WORKSPACE', payload: workspaceId });
                    handleShowView('messages');
                  }}
                />
              ) : activeView === 'executors' ? (
                <ExecutorsPanel />
              ) : activeView === 'experts' ? (
                <ExpertsPanel />
              ) : activeView === 'bots' ? (
                <AssistantManagementPage />
              ) : activeView === 'settings' ? (
                <SettingsPage />
              ) : activeView === 'blackboard' ? (
                <BlackboardPage workspaceId={state.selectedWorkspace} />
              ) : activeView === 'messages' ? (
                // 消息页：workspaceId 由左上角 WorkspaceSwitcher 联动传入；
                // 未选中时 MessagesPage 内部给出空态引导，onManageWorkspace 落到工作空间管理页。
                <MessagesPage
                  workspaceId={state.selectedWorkspace}
                  onManageWorkspace={() => showStandaloneSettingsPanel('workspaces')}
                />
              ) : activeView === 'wiki' ? (
                <WikiViewPage />
              ) : activeView === 'tasks' ? (
                taskDetailId != null ? (
                  <TaskDetailPage
                    taskId={taskDetailId}
                    onBack={() => backToList()}
                    onSelectTodo={handleSelectTodo}
                    // NTD-014-A：任务删除成功后跳回任务列表。
                    onDeleted={() => backToList()}
                  />
                ) : (
                  <TasksPage workspaceId={state.selectedWorkspace} />
                )
              ) : activeView === 'onboarding' ? (
                <ConceptNavPage workspaceId={state.selectedWorkspace} />
              ) : activeView === 'processes' ? (
                <ProcessPage
                  workspaceId={state.selectedWorkspace}
                  onOpenLoop={handleSelectLoop}
                  processGuid={processGuid}
                  processMode={processMode}
                />
              ) : (
                <Dashboard />
              )}
            </div>
          )}
          </Suspense>
        </Content>
      </Layout>

      {/* Navigation Drawer */}
      <Drawer
        open={navDrawerOpen}
        onClose={() => setNavDrawerOpen(false)}
        placement="left"
        width={280}
        rootClassName="ntd-nav-drawer"
        styles={{ body: { padding: 0 } }}
      >
        <LeftRail
          activeKey={navKey}
          onSelect={handleRailSelect}
          variant="drawer"
          workspace={state.selectedWorkspace}
          onWorkspaceChange={(workspace) => {
            dispatch({ type: 'SELECT_WORKSPACE', payload: workspace });
          }}
          themeMode={themeMode}
          toggleTheme={toggleTheme}
        />
      </Drawer>

      {/* Todo Drawer */}
      {/* 028：todo prop 改为 editingTodo，支持列表页 onEditTodo 触发的编辑模式 */}
      <TodoDrawer
        open={todoModalOpen}
        todo={editingTodo}
        tags={state.tags}
        onClose={() => {
          setTodoModalOpen(false);
          // 关闭时清空 editingTodo，避免下次打开仍处于编辑模式
          setEditingTodo(null);
        }}
        onSaved={(created) => {
          // 056：全局 todos 桶已删除，保存后通知列表页重拉当前页即可
          window.dispatchEvent(new Event(TODO_LIST_REFRESH_EVENT));
          // NTD-014-B：新建事项成功后跳转详情页，让用户立即看到并触发刚创建的事项；
          // 编辑保存（created 为 undefined）不跳转，停留在原页面。
          if (created?.id != null) {
            pushUrl('todos', { id: created.id });
          }
        }}
        defaultWorkspaceId={state.selectedWorkspace}
      />

      {/* Smart Create Modal */}
      <SmartCreateModal
        open={smartCreateOpen}
        onClose={() => setSmartCreateOpen(false)}
        isMobile={isMobile}
        config={appConfig}
        workspaceId={state.selectedWorkspace}
        onGoToSettings={() => handleShowView('settings')}
        onSubmitted={handleSmartCreateSubmitted}
      />

      {/* Quick Capture Modal */}
      <QuickCaptureModal
        open={quickCaptureOpen}
        onClose={() => setQuickCaptureOpen(false)}
        isMobile={isMobile}
        defaultWorkspaceId={state.selectedWorkspace}
        onCreated={() => {
          // 056：全局 todos 桶已删除，创建后通知列表页重拉当前页
          window.dispatchEvent(new Event(TODO_LIST_REFRESH_EVENT));
        }}
        onExecuted={() => {}}
      />

      {/* Execution Panel */}
      {/* 始终挂载以保留其内部「完成后 5s 自动移除任务」的定时器逻辑；
          通过 hidden 让它在开关关闭/临时关闭/无运行任务时 return null，不占任何空间。 */}
      <ExecutionPanel
        hidden={consolePanelHidden}
        collapsed={panelCollapsed}
        onToggleCollapse={() => {
          const next = !panelCollapsed;
          setPanelCollapsed(next);
          try { localStorage.setItem('execution_panel_collapsed', String(next)); } catch {}
        }}
        onTemporaryClose={() => setConsolePanelDismissed(true)}
        onPermanentClose={() => setConsolePanelVisible(false)}
      />

      {/* Wiki 对话全局漂浮窗口 */}
      <WikiChatFloatingWindow
        forceMode={wikiChatMode}
        onClose={() => setWikiChatMode('minimized')}
      />

      {/* 帮助大弹窗：内嵌左菜单 + 右 PageCard，关闭即全部关闭 */}
      <HelpPage
        open={helpModalOpen}
        onClose={() => setHelpModalOpen(false)}
        initialPageId={helpInitialPageId}
      />
    </Layout>
  );
}

function ThemedApp() {
  const { themeConfig } = useTheme();
  return (
    <ConfigProvider locale={zhCN} theme={themeConfig}>
      <AntApp>
        <AppProvider>
          <AppContent />
        </AppProvider>
      </AntApp>
    </ConfigProvider>
  );
}

function App() {
  return (
    <ThemeProvider>
      <ConsolePanelProvider>
        <ThemedApp />
      </ConsolePanelProvider>
    </ThemeProvider>
  );
}

export default App;
