// 主应用入口组件。

import { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { ConfigProvider, Layout, App as AntApp, Drawer } from 'antd';
import { AppProvider, useApp } from './hooks/useApp';
import { useIsMobile } from './hooks/useIsMobile';
import { useExecutionEvents } from './hooks/useExecutionEvents';
import { useViewState, viewToNavKey, type View } from './hooks/useViewState';
import { ThemeProvider, useTheme } from '@/hooks/useTheme';
import { ConsolePanelProvider, useConsolePanel } from '@/hooks/useConsolePanel';
import { TodoPage } from './components/TodoPage';
import { TodoPostPage } from './components/todo-post';
import { LoopPage } from './components/LoopPage';
import { TodoMobilePage } from './components/mobile/TodoMobilePage';
import { LoopMobilePage } from './components/mobile/LoopMobilePage';
import { Dashboard } from './components/Dashboard';
import { MemorialBoard } from './components/MemorialBoard';
import { SettingsPage } from './components/SettingsPage';
import { SkillsPanel } from './components/SkillsPanel';
import { ProjectDirectoriesPanel } from './components/settings/ProjectDirectoriesPanel';
import { ExecutorsPanel } from './components/settings/ExecutorsPanel';
import { BlackboardPage } from './components/BlackboardPage';
import { ExecutionPanel } from './components/ExecutionPanel';
import { TodoDrawer } from './components/TodoDrawer';
import { SmartCreateModal } from './components/SmartCreateModal';
import { QuickCaptureModal } from './components/QuickCaptureModal';
import { LoopFormModal } from './components/LoopFormModal';
import { LeftRail, type LeftRailKey } from './components/shell/LeftRail';
import { MobileHeader } from './components/shell/MobileHeader';
import { FloatingActionButton } from '@/components/shell/FloatingActionButton';
import { WikiChatFloatingWindow, type WikiChatMode } from '@/components/WikiChatFloatingWindow';
import { WikiViewPage } from '@/components/WikiViewPage';

import { EXECUTION_PANEL, LEFT_RAIL_WIDTH } from './constants';
import * as db from './utils/database';
import type { Config } from './types';
import zhCN from 'antd/locale/zh_CN';
import './App.css';

const { Content } = Layout;

function AppContent() {
  const { state, dispatch, clearSelection } = useApp();
  const { activeView, selectedId, activePanel, selectedRecordId, showView, pushUrl, replaceUrl, backToList } = useViewState();
  const { themeMode, toggleTheme } = useTheme();
  // 底部执行日志面板的显隐开关：来自设置-界面显示，关掉后即使有运行中任务也不渲染面板。
  const { visible: consolePanelVisible, setVisible: setConsolePanelVisible } = useConsolePanel();
  // 临时关闭态：面板上的「临时关闭」按钮置位，仅本轮任务期间隐藏，不写 localStorage。
  // 与 consolePanelVisible 区分：永久关闭=setVisible(false) 落盘；临时关闭=会话内 dismiss。
  const [consolePanelDismissed, setConsolePanelDismissed] = useState(false);

  const [todoModalOpen, setTodoModalOpen] = useState(false);
  const [smartCreateOpen, setSmartCreateOpen] = useState(false);
  const [quickCaptureOpen, setQuickCaptureOpen] = useState(false);
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
  const [loopCreateModalOpen, setLoopCreateModalOpen] = useState(false);
  const [selectedLoopId, setSelectedLoopId] = useState<number | null>(null);
  const [loopUpdateCount, setLoopUpdateCount] = useState(0);
  const [forcedListMode, setForcedListMode] = useState<'item' | 'loop' | undefined>(undefined);

  const navKey = useMemo<LeftRailKey>(() => {
    return viewToNavKey(activeView) as LeftRailKey;
  }, [activeView]);
  const isMobile = useIsMobile();

  const effectiveMobilePanel = isMobile && activeView !== 'items' && activeView !== 'loops'
    ? 'detail'
    : activePanel === 'post' ? 'list' : activePanel;

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

  // URL → React state 恢复：根据 URL 中的 view/id 参数同步选中态。
  // 合并为单个 useEffect，监听 activeView、selectedId、state.loading 和 state.todos。
  // items 视图需要校验 todo 是否存在（可能已被删除），loops 和其他视图直接同步。
  useEffect(() => {
    if (state.loading) return;
    if (activeView === 'items') {
      // items 视图必须让 selectedTodoId 与 URL 中的 id 保持一致：
      // 找到有效 todo 时选中它；id 缺失或指向已删除的 todo 时显式清空，
      // 避免 TodoPage/TodoMobilePage 残留上一次选中的详情态。
      const matched = selectedId != null && state.todos.some(t => t.id === selectedId);
      // matched 为 true 时 payload 是 selectedId，否则 null（清空旧选中态）
      dispatch({ type: 'SELECT_TODO', payload: matched ? selectedId : null });
      // items 视图下清空 loop 选中态，防止跨视图状态混淆
      setSelectedLoopId(null);
    } else if (activeView === 'loops' && selectedId != null) {
      setSelectedLoopId(selectedId);
      dispatch({ type: 'SELECT_TODO', payload: null });
      clearSelection();
    } else {
      setSelectedLoopId(null);
      if (activeView !== 'loops') {
        dispatch({ type: 'SELECT_TODO', payload: null });
      }
    }
  }, [activeView, selectedId, state.loading, state.todos, dispatch, clearSelection]);

  const handleSelectTodo = (todoId: string | number | null) => {
    if (todoId != null) {
      setSelectedLoopId(null);
      dispatch({ type: 'SELECT_TODO', payload: Number(todoId) });
      replaceUrl('items', { id: Number(todoId), panel: 'detail' });
    }
  };

  const handleOpenPost = useCallback((todoId: number, recordId: number) => {
    pushUrl('items', { id: todoId, panel: 'post', record: recordId });
  }, [pushUrl]);

  const handleSelectLoop = useCallback((loopId: number) => {
    clearSelection();
    setSelectedLoopId(loopId);
    replaceUrl('loops', { id: loopId, panel: 'detail' });
  }, [clearSelection, replaceUrl]);

  const handleSmartCreateSubmitted = () => {
    const wid = state.selectedWorkspace;
    if (wid == null) return;
    db.getAllTodos(wid).then(todos => {
      dispatch({ type: 'SET_TODOS_BY_WORKSPACE', workspaceId: wid, payload: todos });
    });
  };

  const handleShowView = useCallback((view: View) => {
    setSelectedLoopId(null);
    clearSelection();
    showView(view);
  }, [clearSelection, showView]);

  const showSettings = useCallback((tab: string | null) => {
    setSelectedLoopId(null);
    clearSelection();
    showView('settings', { tab });
  }, [clearSelection, showView]);

  const showStandaloneSettingsPanel = useCallback((view: View) => {
    setSelectedLoopId(null);
    clearSelection();
    pushUrl(view);
  }, [clearSelection, pushUrl]);

  const showListSection = useCallback((mode: 'item' | 'loop') => {
    setSelectedLoopId(null);
    clearSelection();
    setForcedListMode(mode);
    replaceUrl(mode === 'loop' ? 'loops' : 'items', { panel: 'list' });
  }, [replaceUrl, clearSelection]);

  const handleRailSelect = useCallback((key: LeftRailKey) => {
    setNavDrawerOpen(false);
    if (key === 'items') { showListSection('item'); return; }
    if (key === 'loops') { showListSection('loop'); return; }
    if (key === 'dashboard') { handleShowView('dashboard'); return; }
    if (key === 'memorial') { handleShowView('memorial'); return; }
    if (key === 'blackboard') { handleShowView('blackboard'); return; }
    if (key === 'settings') { showSettings(null); return; }
    if (key === 'settings_projectDirectories') { showStandaloneSettingsPanel('projectDirectories'); return; }
    if (key === 'settings_skills') { showStandaloneSettingsPanel('skills'); return; }
    if (key === 'settings_executors') { showStandaloneSettingsPanel('executors'); return; }
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
          />
        </div>
      )}

      {/* Main Content */}
      <Layout
        style={{
          flex: 1,
          minWidth: 0,
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
          {/* 帖子详情页 */}
          {activeView === 'items' && activePanel === 'post' && selectedId != null && selectedRecordId != null && (
            <TodoPostPage
              todoId={selectedId}
              recordId={selectedRecordId}
              onBack={() => replaceUrl('items', { id: selectedId, panel: 'list' })}
            />
          )}

          {/* 事项页面 */}
          {activeView === 'items' && activePanel !== 'post' && (
            isMobile ? (
              <TodoMobilePage
                selectedTodoId={state.selectedTodoId}
                onOpenCreateModal={() => setTodoModalOpen(true)}
                onSelectTodo={handleSelectTodo}
                loopUpdateCount={loopUpdateCount}
                onSelectLoop={handleSelectLoop}
                onCreateLoop={() => setLoopCreateModalOpen(true)}
                forcedListMode={forcedListMode}
                onListModeChange={() => setForcedListMode(undefined)}
                effectiveMobilePanel={effectiveMobilePanel}
                onOpenPost={handleOpenPost}
              />
            ) : (
              <TodoPage
                selectedTodoId={state.selectedTodoId}
                onOpenCreateModal={() => setTodoModalOpen(true)}
                onSelectTodo={handleSelectTodo}
                loopUpdateCount={loopUpdateCount}
                onSelectLoop={handleSelectLoop}
                onCreateLoop={() => setLoopCreateModalOpen(true)}
                forcedListMode={forcedListMode}
                onListModeChange={() => setForcedListMode(undefined)}
                effectiveMobilePanel={effectiveMobilePanel}
                onOpenPost={handleOpenPost}
              />
            )
          )}

          {/* 环路页面 */}
          {activeView === 'loops' && (
            isMobile ? (
              <LoopMobilePage
                selectedLoopId={selectedLoopId}
                tags={state.tags}
                onOpenCreateModal={() => setTodoModalOpen(true)}
                onSelectTodo={handleSelectTodo}
                loopUpdateCount={loopUpdateCount}
                onSelectLoop={handleSelectLoop}
                onCreateLoop={() => setLoopCreateModalOpen(true)}
                forcedListMode={forcedListMode}
                onListModeChange={() => setForcedListMode(undefined)}
                onLoopChanged={() => setLoopUpdateCount(c => c + 1)}
                effectiveMobilePanel={effectiveMobilePanel}
              />
            ) : (
              <LoopPage
                selectedLoopId={selectedLoopId}
                tags={state.tags}
                onOpenCreateModal={() => setTodoModalOpen(true)}
                onSelectTodo={handleSelectTodo}
                loopUpdateCount={loopUpdateCount}
                onSelectLoop={handleSelectLoop}
                onCreateLoop={() => setLoopCreateModalOpen(true)}
                forcedListMode={forcedListMode}
                onListModeChange={() => setForcedListMode(undefined)}
                onLoopChanged={() => setLoopUpdateCount(c => c + 1)}
                effectiveMobilePanel={effectiveMobilePanel}
              />
            )
          )}

          {/* 非事项/环路视图 */}
          {activeView !== 'items' && activeView !== 'loops' && (
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
              ) : activeView === 'projectDirectories' ? (
                <ProjectDirectoriesPanel />
              ) : activeView === 'executors' ? (
                <ExecutorsPanel />
              ) : activeView === 'settings' ? (
                <SettingsPage />
              ) : activeView === 'memorial' ? (
                <MemorialBoard />
              ) : activeView === 'blackboard' ? (
                <BlackboardPage workspaceId={state.selectedWorkspace} />
              ) : activeView === 'wiki' ? (
                <WikiViewPage />
              ) : (
                <Dashboard />
              )}
            </div>
          )}
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
      <TodoDrawer
        open={todoModalOpen}
        todo={null}
        tags={state.tags}
        onClose={() => setTodoModalOpen(false)}
        onSaved={() => {
          const wid = state.selectedWorkspace;
          if (wid == null) return;
          db.getAllTodos(wid).then(todos => {
            dispatch({ type: 'SET_TODOS_BY_WORKSPACE', workspaceId: wid, payload: todos });
          });
        }}
        defaultWorkspaceId={state.selectedWorkspace}
      />

      {/* Smart Create Modal */}
      <SmartCreateModal
        open={smartCreateOpen}
        onClose={() => setSmartCreateOpen(false)}
        isMobile={isMobile}
        config={appConfig}
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
          const wid = state.selectedWorkspace;
          if (wid != null) {
            db.getAllTodos(wid).then(todos => {
              dispatch({ type: 'SET_TODOS_BY_WORKSPACE', workspaceId: wid, payload: todos });
            });
          }
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

      {/* Loop Create Modal */}
      <LoopFormModal
        open={loopCreateModalOpen}
        mode="create"
        tags={state.tags}
        onSaved={(newLoopId) => {
          if (newLoopId) setSelectedLoopId(newLoopId);
          setLoopUpdateCount(c => c + 1);
          setLoopCreateModalOpen(false);
        }}
        onClose={() => setLoopCreateModalOpen(false)}
        defaultWorkspaceId={state.selectedWorkspace}
      />

      {/* Wiki 对话全局漂浮窗口 */}
      <WikiChatFloatingWindow
        forceMode={wikiChatMode}
        onClose={() => setWikiChatMode('minimized')}
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
