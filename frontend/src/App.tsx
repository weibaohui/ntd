import { useState, useEffect, useCallback, useMemo } from 'react';
import { ConfigProvider, Layout, App as AntApp, Drawer, message } from 'antd';
import { PlusOutlined, ThunderboltOutlined, CloseOutlined, ArrowLeftOutlined, MenuOutlined } from '@ant-design/icons';
import { AppProvider, useApp } from './hooks/useApp';
import { useIsMobile } from './hooks/useIsMobile';
import { useExecutionEvents } from './hooks/useExecutionEvents';
import { useViewState, viewToNavKey, type View } from './hooks/useViewState';
import { ThemeProvider, useTheme } from './hooks/useTheme';
import { TodoList } from './components/TodoList';
import { TodoDetail } from './components/TodoDetail';
import { Dashboard } from './components/Dashboard';
import { MemorialBoard } from './components/MemorialBoard';
import { SettingsPage } from './components/SettingsPage';
import { SkillsPanel } from './components/SkillsPanel';
import { SessionManager } from './components/SessionManager';
import { ProjectDirectoriesPanel } from './components/settings/ProjectDirectoriesPanel';
import { RuntimePanel } from './components/settings/RuntimePanel';
import { ExecutorsPanel } from './components/settings/ExecutorsPanel';
import { ExecutionPanel } from './components/ExecutionPanel';
import { TodoDrawer } from './components/TodoDrawer';
import { SmartCreateModal } from './components/SmartCreateModal';
import { LoopDetailPanel } from './components/LoopStudioDetailPanel';
import { LoopFormModal } from './components/LoopFormModal';
import { LeftRail, type LeftRailKey } from './components/shell/LeftRail';
import { EmptyDetailPlaceholder } from './components/EmptyDetailPlaceholder';
import { PageCard } from './components/common/PageCard';
import { UnorderedListOutlined, RetweetOutlined } from '@ant-design/icons';

import * as dbLoops from './utils/database/loops';
import { EXECUTION_PANEL, LEFT_RAIL_WIDTH, SIDEBAR_WIDTH } from './constants';
import * as db from './utils/database';
import type { Config, ExecutorConfig } from './types';
import zhCN from 'antd/locale/zh_CN';
import './App.css';

const { Content } = Layout;

function AppContent() {
  const { state, dispatch, clearSelection } = useApp();
  const { activeView, selectedId, selectedPanel, showView, pushUrl } = useViewState();
  const { themeMode, toggleTheme } = useTheme();

  const [todoModalOpen, setTodoModalOpen] = useState(false);
  const [smartCreateOpen, setSmartCreateOpen] = useState(false);
  const [fabExpanded, setFabExpanded] = useState(false);
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
  // 新建环路弹窗（使用 LoopFormModal create 模式）
  const [loopCreateModalOpen, setLoopCreateModalOpen] = useState(false);
  // 从左侧环路列表选中某个 loop 时记录其 id，右侧面板展示 LoopDetailPanel
  const [selectedLoopId, setSelectedLoopId] = useState<number | null>(null);
  // 环路变更计数，驱动左侧 loop 列表刷新
  const [loopUpdateCount, setLoopUpdateCount] = useState(0);
  const [forcedListMode, setForcedListMode] = useState<'item' | 'loop' | undefined>(undefined);
  // 从 activeView 派生出 LeftRailKey，去除独立的状态源
  const navKey = useMemo<LeftRailKey>(() => {
    return viewToNavKey(activeView) as LeftRailKey;
  }, [activeView]);
  const isMobile = useIsMobile();

  // 手机端有效面板：items/loops 视图使用 selectedPanel（基于 id 判断），
  // 其他视图（dashboard/memorial/settings 等）始终显示 detail 面板
  const effectiveMobilePanel = isMobile && activeView !== 'items' && activeView !== 'loops'
    ? 'detail'
    : selectedPanel;

  // 切换视图或面板时收起 FAB，避免遗留
  useEffect(() => {
    setFabExpanded(false);
  }, [effectiveMobilePanel]);

  const [panelCollapsed, setPanelCollapsed] = useState(() => {
    try {
      return localStorage.getItem('execution_panel_collapsed') === 'true';
    } catch {
      return false;
    }
  });

  useExecutionEvents();

  // settingsTabChanged 事件不再需要 — activeView + activeTab 由 useViewState 统一管理

  const hasRunningTasks = Object.keys(state.runningTasks).length > 0;
  const panelHeight = hasRunningTasks ? (panelCollapsed ? EXECUTION_PANEL.collapsed : EXECUTION_PANEL.expanded) : 0;

  // Load app config on mount
  useEffect(() => {
    db.getConfig().then(setAppConfig).catch(() => {});
  }, []);

  // 执行器列表供 RuntimePanel 的 executorDisplayNames 使用
  const [runtimeExecutors, setRuntimeExecutors] = useState<ExecutorConfig[]>([]);

  useEffect(() => {
    db.getExecutors().then(setRuntimeExecutors).catch(() => {});
  }, []);

  const runtimeExecutorDisplayNames = useMemo(() => {
    const map: Record<string, string> = {};
    for (const ec of runtimeExecutors) {
      map[ec.name] = ec.display_name;
    }
    return map;
  }, [runtimeExecutors]);

  // URL → React state 恢复（首次加载完成后执行）
  useEffect(() => {
    if (state.loading) return;
    if (activeView === 'items' && selectedId != null && state.todos.some(t => t.id === selectedId)) {
      dispatch({ type: 'SELECT_TODO', payload: selectedId });
      setSelectedLoopId(null);
    } else if (activeView === 'loops' && selectedId != null) {
      setSelectedLoopId(selectedId);
      dispatch({ type: 'SELECT_TODO', payload: null });
      clearSelection();
    } else if (activeView !== 'items' && activeView !== 'loops') {
      // 非列表视图时清除选择
      setSelectedLoopId(null);
      dispatch({ type: 'SELECT_TODO', payload: null });
    }
  }, [state.loading, state.todos, dispatch, clearSelection, activeView, selectedId]);

  // popstate 由 useViewState 统一处理；这里监听 view/id 变化来同步 React 状态
  useEffect(() => {
    if (activeView === 'items' && selectedId != null) {
      setSelectedLoopId(null);
      if (!state.loading) {
        dispatch({ type: 'SELECT_TODO', payload: selectedId });
      }
    } else if (activeView === 'loops' && selectedId != null) {
      setSelectedLoopId(selectedId);
      dispatch({ type: 'SELECT_TODO', payload: null });
      clearSelection();
    } else {
      setSelectedLoopId(null);
    }
  }, [activeView, selectedId, dispatch, clearSelection, state.loading]);

  const handleSelectTodo = (todoId: string | number | null) => {
    if (todoId != null) {
      // 选中 todo 时清除 loop 选择，避免右侧面板显示冲突
      setSelectedLoopId(null);
      dispatch({ type: 'SELECT_TODO', payload: Number(todoId) });
      pushUrl('items', { id: Number(todoId) });
    }
  };

  // 从左侧环路列表选中一个 loop，在右侧展示 LoopDetailPanel
  const handleSelectLoop = useCallback((loopId: number) => {
    // 清除 todo 选择，避免 state.selectedTodoId 抢占右侧面板
    clearSelection();
    setSelectedLoopId(loopId);
    pushUrl('loops', { id: loopId });
  }, [clearSelection, pushUrl]);

  const handleSmartCreateSubmitted = () => {
    db.getAllTodos().then(todos => {
      dispatch({ type: 'SET_TODOS', payload: todos });
    });
  };

  // 统一导航处理：切换 view 时清空 loop 选择，避免旧选择抢占右侧面板
  // 手机端：非 items/loops 视图需要切换到 detail 面板，确保显示右侧内容而非中间列表
  const handleShowView = useCallback((view: View) => {
    setSelectedLoopId(null);
    clearSelection();
    showView(view);
  }, [clearSelection, showView]);

  const handleGoToSettings = () => handleShowView('settings');

  const showSettings = useCallback((tab: string | null) => {
    setSelectedLoopId(null);
    clearSelection();
    showView('settings', { tab });
  }, [clearSelection, showView]);

  /**
   * 切换到独立的配置管理页面（运行管理 / Skills / 工作空间 / 会话）。
   * 这些页面已从设置页标签页中剥离，独立为左侧导航菜单项。
   */
  const showStandaloneSettingsPanel = useCallback((view: View) => {
    setSelectedLoopId(null);
    clearSelection();
    pushUrl(view);
  }, [clearSelection, pushUrl]);

  /**
   * 切换到“事项/环路”这类列表型入口。
   * 目标：在桌面端保持三栏结构（左主导航 + 中间列表 + 右工作区），移动端回到列表面板。
   * 进入后自动选中第一项的工作交由 TodoList 统一处理（需等目录加载 → 工作空间确定）。
   */
  const showListSection = useCallback((mode: 'item' | 'loop') => {
    // 先清除旧选择，再设置新的列表模式
    setSelectedLoopId(null);
    clearSelection();
    setForcedListMode(mode);
    pushUrl(mode === 'loop' ? 'loops' : 'items');
  }, [pushUrl, clearSelection]);

  /**
   * 左侧主导航点击处理（桌面侧栏/移动抽屉共用）。
   */
  const handleRailSelect = useCallback((key: LeftRailKey) => {
    setNavDrawerOpen(false);
    if (key === 'items') {
      showListSection('item');
      return;
    }
    if (key === 'loops') {
      showListSection('loop');
      return;
    }
    if (key === 'dashboard') {
      handleShowView('dashboard');
      return;
    }
    if (key === 'memorial') {
      handleShowView('memorial');
      return;
    }
    if (key === 'settings') {
      showSettings(null);
      return;
    }
    if (key === 'settings_projectDirectories') {
      showStandaloneSettingsPanel('projectDirectories');
      return;
    }
    if (key === 'settings_sessions') {
      showStandaloneSettingsPanel('sessions');
      return;
    }
    if (key === 'settings_skills') {
      showStandaloneSettingsPanel('skills');
      return;
    }
    if (key === 'settings_executors') {
      showStandaloneSettingsPanel('executors');
      return;
    }
    showStandaloneSettingsPanel('runtime');
  }, [handleShowView, showListSection, showSettings, showStandaloneSettingsPanel]);

  // FAB backdrop click to collapse
  const handleFabBackdropClick = () => setFabExpanded(false);

  return (
    <Layout style={{ height: '100vh', flexDirection: isMobile ? 'column' : 'row' }}>
      {/* Mobile Header with Back Button + Hamburger Menu */}
      {isMobile && (
        <div className="mobile-header">
          {activeView !== 'items' ? (
            // 非事项视图时显示返回按钮，使用浏览器历史记录返回
            <button
              className="mobile-header-menu-btn"
              onClick={() => window.history.back()}
              aria-label="返回"
            >
              <ArrowLeftOutlined />
            </button>
          ) : null}
          <button
            className="mobile-header-menu-btn"
            onClick={() => setNavDrawerOpen(true)}
            aria-label="打开菜单"
          >
            <MenuOutlined />
          </button>
        </div>
      )}
      {/* Mobile FAB Group */}
      {isMobile && effectiveMobilePanel === 'list' && (
        <>
          {fabExpanded && (
            <div className="mobile-fab-backdrop" onClick={handleFabBackdropClick} />
          )}
          <div className="mobile-fab-group">
            {fabExpanded && (
              <>
                <div className="mobile-fab-item" style={{ animationDelay: '0ms' }}>
                  <span className="mobile-fab-item-label">智能新建</span>
                  <button
                    className="mobile-fab-item-btn mobile-fab-smart"
                    onClick={() => { setFabExpanded(false); setSmartCreateOpen(true); }}
                    aria-label="智能新建"
                  >
                    <ThunderboltOutlined style={{ fontSize: 20, color: '#fff' }} />
                  </button>
                </div>
                <div className="mobile-fab-item" style={{ animationDelay: '50ms' }}>
                  <span className="mobile-fab-item-label">新建</span>
                  <button
                    className="mobile-fab-item-btn mobile-fab-create"
                    onClick={() => { setFabExpanded(false); setTodoModalOpen(true); }}
                    aria-label="新建任务"
                  >
                    <PlusOutlined style={{ fontSize: 20, color: '#fff' }} />
                  </button>
                </div>
              </>
            )}
            <button
              className={`mobile-fab-main ${fabExpanded ? 'expanded' : ''}`}
              onClick={() => setFabExpanded(!fabExpanded)}
              aria-label={fabExpanded ? '关闭' : '创建任务'}
            >
              {fabExpanded
                ? <CloseOutlined style={{ fontSize: 22, color: '#fff' }} />
                : <PlusOutlined style={{ fontSize: 24, color: '#fff' }} />}
            </button>
          </div>
        </>
      )}

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
              try {
                localStorage.setItem('ntd_left_rail_collapsed', String(next));
              } catch {}
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

      <Layout
        style={{
          // 右侧主区域在外层横向布局里必须允许收缩，
          // 否则内部超宽内容会把这一整列 Layout 撑宽到视口外。
          flex: 1,
          minWidth: 0,
        }}
      >
        <Content
          style={{
            // Content 也要显式参与剩余空间分配，
            // 避免按内容宽度自适应时把整个页面主区域撑宽。
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
          {/* 事项 / 环路视图：左右两栏统一包裹在 PageCard 中 */}
          {(activeView === 'items' || activeView === 'loops') && !isMobile && (
            <PageCard
              icon={activeView === 'items' ? <UnorderedListOutlined /> : <RetweetOutlined />}
              title={activeView === 'items' ? '事项' : '环路'}
              className="todo-view-page-card"
              style={{ height: '100%', flex: 1, minWidth: 0 }}
              contentStyle={{ padding: 0, display: 'flex', flexDirection: 'row', height: 'calc(100% - 49px)' }}
            >
              {/* 左侧列表 */}
              <div
                className={effectiveMobilePanel === 'list' ? 'animate-fade-in' : ''}
                style={{
                  width: SIDEBAR_WIDTH.desktop,
                  flexShrink: 0,
                  height: '100%',
                  borderRight: '1px solid var(--color-border-light)',
                }}
              >
                <TodoList
                  onOpenCreateModal={() => setTodoModalOpen(true)}
                  onSelectTodo={(todoId) => {
                    handleSelectTodo(todoId);
                  }}
                  loopUpdateCount={loopUpdateCount}
                  onSelectLoop={(loopId) => {
                    handleSelectLoop(loopId);
                  }}
                  onCreateLoop={() => {
                    setLoopCreateModalOpen(true);
                  }}
                  forcedListMode={forcedListMode}
                  onListModeChange={() => {
                    setForcedListMode(undefined);
                  }}
                />
              </div>

              {/* 右侧详情 */}
              <div
                className={effectiveMobilePanel === 'detail' ? 'animate-slide-in-right' : ''}
                style={{
                  flex: 1,
                  minWidth: 0,
                  height: '100%',
                  overflow: 'hidden',
                  padding: 16,
                }}
              >
                {state.selectedTodoId ? (
                  <TodoDetail />
                ) : selectedLoopId !== null ? (
                  <LoopDetailPanel
                    loopId={selectedLoopId}
                    tags={state.tags}
                    onTrigger={async () => {
                      try {
                        const res = await dbLoops.triggerLoop(selectedLoopId);
                        message.success(`已触发 (execution #${res.execution_id})`);
                      } catch (err) {
                        message.error(`触发失败: ${err instanceof Error ? err.message : '未知错误'}`);
                      }
                    }}
                    onDuplicate={async () => {
                      try {
                        await dbLoops.duplicateLoop(selectedLoopId);
                        message.success('已复制');
                      } catch (err) {
                        message.error(`复制失败: ${err instanceof Error ? err.message : '未知错误'}`);
                      }
                    }}
                    onDelete={async () => {
                      try {
                        await dbLoops.deleteLoop(selectedLoopId);
                        message.success('已删除');
                        setLoopUpdateCount(c => c + 1);
                      } catch (err) {
                        message.error('删除失败，环路可能正在被引用');
                      }
                    }}
                    onToggleStatus={async () => {
                      try {
                        const loops = await dbLoops.listLoops();
                        const loop = loops.find(l => l.id === selectedLoopId);
                        if (!loop) return;
                        const next = loop.status === 'enabled' ? 'paused' : 'enabled';
                        await dbLoops.updateLoopStatus(selectedLoopId, { status: next } as any);
                        message.success(`已${next === 'enabled' ? '启用' : '暂停'}`);
                      } catch (err) {
                        message.error(`状态切换失败: ${err instanceof Error ? err.message : '未知错误'}`);
                      }
                    }}
                    onChanged={() => {
                      setLoopUpdateCount(c => c + 1);
                    }}
                  />
                ) : activeView === 'items' ? (
                  <EmptyDetailPlaceholder />
                ) : (
                  <EmptyDetailPlaceholder />
                )}
              </div>
            </PageCard>
          )}

          {/* 移动端：事项 / 环路视图（单列切换） */}
          {(activeView === 'items' || activeView === 'loops') && isMobile && (
            <>
              <div
                className={effectiveMobilePanel === 'list' ? 'animate-fade-in' : ''}
                style={{
                  width: SIDEBAR_WIDTH.mobile,
                  flexShrink: 0,
                  height: '100%',
                  display: effectiveMobilePanel === 'list' ? 'block' : 'none',
                }}
              >
                <TodoList
                  onOpenCreateModal={() => setTodoModalOpen(true)}
                  onSelectTodo={(todoId) => {
                    handleSelectTodo(todoId);
                  }}
                  loopUpdateCount={loopUpdateCount}
                  onSelectLoop={(loopId) => {
                    handleSelectLoop(loopId);
                  }}
                  onCreateLoop={() => {
                    setLoopCreateModalOpen(true);
                  }}
                  forcedListMode={forcedListMode}
                  onListModeChange={() => {
                    setForcedListMode(undefined);
                  }}
                />
              </div>
              <div
                className={effectiveMobilePanel === 'detail' ? 'animate-slide-in-right' : ''}
                style={{
                  flex: 1,
                  minWidth: 0,
                  height: '100%',
                  overflow: 'hidden',
                  display: effectiveMobilePanel === 'detail' ? 'block' : 'none',
                }}
              >
                {state.selectedTodoId ? (
                  <TodoDetail />
                ) : selectedLoopId !== null ? (
                  <LoopDetailPanel
                    loopId={selectedLoopId}
                    tags={state.tags}
                    onTrigger={async () => {
                      try {
                        const res = await dbLoops.triggerLoop(selectedLoopId);
                        message.success(`已触发 (execution #${res.execution_id})`);
                      } catch (err) {
                        message.error(`触发失败: ${err instanceof Error ? err.message : '未知错误'}`);
                      }
                    }}
                    onDuplicate={async () => {
                      try {
                        await dbLoops.duplicateLoop(selectedLoopId);
                        message.success('已复制');
                      } catch (err) {
                        message.error(`复制失败: ${err instanceof Error ? err.message : '未知错误'}`);
                      }
                    }}
                    onDelete={async () => {
                      try {
                        await dbLoops.deleteLoop(selectedLoopId);
                        message.success('已删除');
                        setLoopUpdateCount(c => c + 1);
                      } catch (err) {
                        message.error('删除失败，环路可能正在被引用');
                      }
                    }}
                    onToggleStatus={async () => {
                      try {
                        const loops = await dbLoops.listLoops();
                        const loop = loops.find(l => l.id === selectedLoopId);
                        if (!loop) return;
                        const next = loop.status === 'enabled' ? 'paused' : 'enabled';
                        await dbLoops.updateLoopStatus(selectedLoopId, { status: next } as any);
                        message.success(`已${next === 'enabled' ? '启用' : '暂停'}`);
                      } catch (err) {
                        message.error(`状态切换失败: ${err instanceof Error ? err.message : '未知错误'}`);
                      }
                    }}
                    onChanged={() => {
                      setLoopUpdateCount(c => c + 1);
                    }}
                  />
                ) : activeView === 'items' ? (
                  <EmptyDetailPlaceholder />
                ) : (
                  <EmptyDetailPlaceholder />
                )}
              </div>
            </>
          )}

          {/* 非事项/环路视图：右侧独占 */}
          {(activeView !== 'items' && activeView !== 'loops') && (
            <div
              style={{
                flex: 1,
                minWidth: 0,
                height: '100%',
                overflow: 'hidden',
              }}
            >
              {activeView === 'runtime' ? (
                <RuntimePanel
                  executorDisplayNames={runtimeExecutorDisplayNames}
                />
              ) : activeView === 'skills' ? (
                <SkillsPanel />
              ) : activeView === 'projectDirectories' ? (
                <ProjectDirectoriesPanel />
              ) : activeView === 'sessions' ? (
                <SessionManager />
              ) : activeView === 'executors' ? (
                <ExecutorsPanel />
              ) : activeView === 'settings' ? (
                <SettingsPage />
              ) : activeView === 'memorial' ? (
                <MemorialBoard />
              ) : (
                <Dashboard />
              )}
            </div>
          )}
        </Content>
      </Layout>

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

      <TodoDrawer
        open={todoModalOpen}
        todo={null}
        tags={state.tags}
        onClose={() => setTodoModalOpen(false)}
        onSaved={() => {
          db.getAllTodos().then(todos => {
            dispatch({ type: 'SET_TODOS', payload: todos });
          });
        }}
      />

      <SmartCreateModal
        open={smartCreateOpen}
        onClose={() => setSmartCreateOpen(false)}
        isMobile={isMobile}
        config={appConfig}
        onGoToSettings={handleGoToSettings}
        onSubmitted={handleSmartCreateSubmitted}
      />

      <ExecutionPanel
        collapsed={panelCollapsed}
        onToggleCollapse={() => {
          const next = !panelCollapsed;
          setPanelCollapsed(next);
          try {
            localStorage.setItem('execution_panel_collapsed', String(next));
          } catch {}
        }}
      />

      {/* 新建环路弹窗 — 复用 LoopFormModal create 模式，用户填写完整信息后创建 */}
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
      <ThemedApp />
    </ThemeProvider>
  );
}

export default App;
