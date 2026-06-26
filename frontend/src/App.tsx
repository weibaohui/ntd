import { useState, useEffect, useCallback, useMemo } from 'react';
import { ConfigProvider, Layout, App as AntApp, Drawer, message, Form } from 'antd';
import { PlusOutlined, ThunderboltOutlined, CloseOutlined, LeftOutlined } from '@ant-design/icons';
import { AppProvider, useApp } from './hooks/useApp';
import { useIsMobile } from './hooks/useIsMobile';
import { useExecutionEvents } from './hooks/useExecutionEvents';
import { useViewState, type View } from './hooks/useViewState';
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
import { ExecutionPanel } from './components/ExecutionPanel';
import { TodoDrawer } from './components/TodoDrawer';
import { SmartCreateModal } from './components/SmartCreateModal';
import { LoopDetailPanel } from './components/LoopStudioDetailPanel';
import { LoopFormModal } from './components/LoopFormModal';
import { LeftRail, type LeftRailKey } from './components/shell/LeftRail';
import * as dbLoops from './utils/database/loops';
import { EXECUTION_PANEL, LEFT_RAIL_WIDTH, SIDEBAR_WIDTH } from './constants';
import * as db from './utils/database';
import type { Config, ExecutorConfig } from './types';
import zhCN from 'antd/locale/zh_CN';
import './App.css';

const { Content } = Layout;

function AppContent() {
  const { state, dispatch, clearSelection } = useApp();
  const { activeView, selectedPanel, setSelectedPanel, showView, selectTodo, backToList } = useViewState();
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
  const [activeNavKey, setActiveNavKey] = useState<LeftRailKey>(() => {
    try {
      const params = new URLSearchParams(window.location.search);
      const view = params.get('view');
      const tab = params.get('tab');
      if (view === 'settings' && tab) {
        if (tab === 'projectDirectories') return 'settings_projectDirectories';
        if (tab === 'sessions') return 'settings_sessions';
        if (tab === 'skills') return 'settings_skills';
        if (tab === 'runtime') return 'settings_runtime';
        return 'settings';
      }
      if (view === 'memorial') return 'memorial';
      if (view === 'settings') return 'settings';
      if (view === 'dashboard') return 'dashboard';
      const saved = localStorage.getItem('ntd_list_mode');
      return saved === 'loop' ? 'loops' : 'items';
    } catch {
      return 'items';
    }
  });
  const isMobile = useIsMobile();

  const [panelCollapsed, setPanelCollapsed] = useState(() => {
    try {
      return localStorage.getItem('execution_panel_collapsed') === 'true';
    } catch {
      return false;
    }
  });

  useExecutionEvents();

  useEffect(() => {
    const handler = (event: Event) => {
      const custom = event as CustomEvent<{ tab?: string }>;
      const tab = custom.detail?.tab;
      if (tab === 'projectDirectories') setActiveNavKey('settings_projectDirectories');
      else if (tab === 'sessions') setActiveNavKey('settings_sessions');
      else if (tab === 'skills') setActiveNavKey('settings_skills');
      else if (tab === 'runtime') setActiveNavKey('settings_runtime');
      else setActiveNavKey('settings');
    };
    window.addEventListener('settingsTabChanged', handler);
    return () => window.removeEventListener('settingsTabChanged', handler);
  }, []);

  const hasRunningTasks = Object.keys(state.runningTasks).length > 0;
  const panelHeight = hasRunningTasks ? (panelCollapsed ? EXECUTION_PANEL.collapsed : EXECUTION_PANEL.expanded) : 0;

  // Load app config on mount
  useEffect(() => {
    db.getConfig().then(setAppConfig).catch(() => {});
  }, []);

  // —— 独立页面：「运行管理」与「设置页」共享的配置表单状态 —
  // RuntimePanel 从设置标签页剥离成独立页面后，需要自己的 Form 实例 + 配置加载/保存逻辑。
  const [runtimeConfigForm] = Form.useForm();
  const [runtimeConfigSaving, setRuntimeConfigSaving] = useState(false);
  // 执行器列表供 executorDisplayNames 使用
  const [runtimeExecutors, setRuntimeExecutors] = useState<ExecutorConfig[]>([]);

  useEffect(() => {
    db.getConfig().then((cfg) => {
      runtimeConfigForm.setFieldsValue(cfg);
    }).catch(() => {});
  }, [runtimeConfigForm]);

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

  const handleRuntimeSaveConfig = useCallback(async () => {
    try {
      const values = await runtimeConfigForm.validateFields();
      setRuntimeConfigSaving(true);
      await db.updateConfig(values);
      message.success('配置已保存');
    } catch (err: any) {
      if (err?.errorFields) return;
      message.error('保存失败: ' + (err?.message || String(err)));
    } finally {
      setRuntimeConfigSaving(false);
    }
  }, [runtimeConfigForm]);

  // On initial load, restore todo/loop selection from URL (only when loading finishes)
  useEffect(() => {
    if (state.loading) return;
    const params = new URLSearchParams(window.location.search);
    const todoId = params.get('todo');
    const loopId = params.get('loop');
    if (todoId && state.todos.some(t => String(t.id) === todoId)) {
      dispatch({ type: 'SELECT_TODO', payload: Number(todoId) });
      setSelectedPanel('detail');
    } else if (loopId) {
      setSelectedLoopId(Number(loopId));
      setSelectedPanel('detail');
    }
  }, [state.loading, state.todos, dispatch, setSelectedPanel]);

  // Browser back/forward: restore loop selection from URL
  useEffect(() => {
    const onPopState = () => {
      const params = new URLSearchParams(window.location.search);
      const todoId = params.get('todo');
      const loopId = params.get('loop');
      if (todoId) {
        // useViewState handles todo selection; just clear loop
        setSelectedLoopId(null);
      } else if (loopId) {
        setSelectedLoopId(Number(loopId));
        setSelectedPanel('detail');
        dispatch({ type: 'SELECT_TODO', payload: null });
        clearSelection();
      } else {
        setSelectedLoopId(null);
      }
    };
    window.addEventListener('popstate', onPopState);
    return () => window.removeEventListener('popstate', onPopState);
  }, [dispatch, clearSelection, setSelectedPanel]);

  const handleSelectTodo = (todoId: string | number | null) => {
    if (todoId != null) {
      // 选中 todo 时清除 loop 选择，避免右侧面板显示冲突
      setSelectedLoopId(null);
      dispatch({ type: 'SELECT_TODO', payload: Number(todoId) });
      selectTodo(Number(todoId));
    }
  };

  // 从左侧环路列表选中一个 loop，在右侧展示 LoopDetailPanel
  const handleSelectLoop = useCallback((loopId: number) => {
    // 清除 todo 选择，避免 state.selectedTodoId 抢占右侧面板
    dispatch({ type: 'SELECT_TODO', payload: null });
    clearSelection();
    setSelectedLoopId(loopId);
    setSelectedPanel('detail');
    // 更新 URL，支持浏览器前进/后退导航
    const params = new URLSearchParams();
    params.set('loop', String(loopId));
    window.history.pushState(null, '', `/?${params.toString()}`);
  }, [dispatch, clearSelection, setSelectedPanel]);

  const handleSmartCreateSubmitted = () => {
    db.getAllTodos().then(todos => {
      dispatch({ type: 'SET_TODOS', payload: todos });
    });
  };

  // 统一导航处理：切换 view 时清空 loop 选择，避免旧选择抢占右侧面板
  const handleShowView = useCallback((view: View) => {
    setSelectedLoopId(null);
    clearSelection();
    showView(view);
    setActiveNavKey(view === 'settings' ? 'settings' : view === 'memorial' ? 'memorial' : 'dashboard');
  }, [clearSelection, showView]);

  const handleGoToSettings = () => handleShowView('settings');

  const showSettings = useCallback((tab: string | null, navKey: LeftRailKey) => {
    setSelectedLoopId(null);
    dispatch({ type: 'SELECT_TODO', payload: null });
    clearSelection();
    setActiveNavKey(navKey);
    showView('settings', { tab });
  }, [clearSelection, dispatch, showView]);

  /**
   * 切换到独立的配置管理页面（运行管理 / Skills / 工作空间 / 会话）。
   * 这些页面已从设置页标签页中剥离，独立为左侧导航菜单项。
   */
  const showStandaloneSettingsPanel = useCallback((navKey: LeftRailKey) => {
    setSelectedLoopId(null);
    dispatch({ type: 'SELECT_TODO', payload: null });
    clearSelection();
    setActiveNavKey(navKey);
    // 保持 activeView 不变（应为 'dashboard'），不触发视图切换，
    // 让右侧面板根据 activeNavKey 渲染对应的独立页面。
  }, [clearSelection, dispatch]);

  /**
   * 切换到“事项/环路”这类列表型入口。
   * 目标：在桌面端保持三栏结构（左主导航 + 中间列表 + 右工作区），移动端回到列表面板。
   * 进入后自动选中第一项，让右侧直接展示详情，避免显示空白仪表盘页。
   */
  const showListSection = useCallback((mode: 'item' | 'loop') => {
    // 先清除旧选择，再设置新的列表模式
    setSelectedLoopId(null);
    dispatch({ type: 'SELECT_TODO', payload: null });
    clearSelection();
    setForcedListMode(mode);
    setActiveNavKey(mode === 'loop' ? 'loops' : 'items');
    backToList();

    // 自动选中第一项：事项模式选中第一个 todo，环路模式由 TodoList 加载后自动选中第一项
    if (mode === 'item') {
      const todos = state.todos;
      if (todos.length > 0) {
        const firstId = todos[0].id;
        dispatch({ type: 'SELECT_TODO', payload: firstId });
        selectTodo(firstId);
      }
    }
  }, [backToList, clearSelection, dispatch, state.todos, selectTodo]);

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
      setActiveNavKey('dashboard');
      handleShowView('dashboard');
      return;
    }
    if (key === 'memorial') {
      setActiveNavKey('memorial');
      handleShowView('memorial');
      return;
    }
    if (key === 'settings') {
      showSettings(null, 'settings');
      return;
    }
    if (key === 'settings_projectDirectories') {
      showStandaloneSettingsPanel('settings_projectDirectories');
      return;
    }
    if (key === 'settings_sessions') {
      showStandaloneSettingsPanel('settings_sessions');
      return;
    }
    if (key === 'settings_skills') {
      showStandaloneSettingsPanel('settings_skills');
      return;
    }
    showStandaloneSettingsPanel('settings_runtime');
  }, [handleShowView, showListSection, showSettings, showStandaloneSettingsPanel]);

  // FAB backdrop click to collapse
  const handleFabBackdropClick = () => setFabExpanded(false);

  return (
    <Layout style={{ height: '100vh', flexDirection: isMobile ? 'column' : 'row' }}>
      {/* Mobile FAB Group */}
      {isMobile && selectedPanel === 'list' && (
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
            activeKey={activeNavKey}
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

      <Layout>
        <Content
          style={{
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
          {/* 中间列表面板：仅在「事项」或「环路」导航选中时显示；
              仪表盘/看板/配置等页面由右侧面板独占，不需要中间列表 */}
          <div
            className={(!isMobile || selectedPanel === 'list') ? 'animate-fade-in' : ''}
            style={{
              width: isMobile ? SIDEBAR_WIDTH.mobile : SIDEBAR_WIDTH.desktop,
              flexShrink: 0,
              height: '100%',
              display: isMobile
                ? (selectedPanel === 'list' ? 'block' : 'none')
                : (activeNavKey === 'items' || activeNavKey === 'loops' ? 'block' : 'none'),
            }}
          >
            <TodoList
              onOpenCreateModal={() => setTodoModalOpen(true)}
              onSelectTodo={(todoId) => {
                setActiveNavKey('items');
                handleSelectTodo(todoId);
              }}
              loopUpdateCount={loopUpdateCount}
              onSelectLoop={(loopId) => {
                setActiveNavKey('loops');
                handleSelectLoop(loopId);
              }}
              onCreateLoop={() => {
                // 打开 LoopFormModal 创建模式，用户填写完整信息后创建环路
                setLoopCreateModalOpen(true);
              }}
              forcedListMode={forcedListMode}
              onListModeChange={(mode) => {
                setForcedListMode(undefined);
                if (activeNavKey === 'items' || activeNavKey === 'loops') {
                  setActiveNavKey(mode === 'loop' ? 'loops' : 'items');
                }
              }}
            />
          </div>

          {/* Right Workspace */}
          <div
            className={(!isMobile || selectedPanel === 'detail') ? 'animate-slide-in-right' : ''}
            style={{
              flex: 1,
              height: '100%',
              overflow: 'hidden',
              display: !isMobile || selectedPanel === 'detail' ? 'block' : 'none',
            }}
          >
            {state.selectedTodoId ? (
              <TodoDetail
                onBack={isMobile ? backToList : undefined}
              />
            ) : selectedLoopId !== null ? (
              // 从左侧环路列表选中某个 loop，右侧展示 LoopDetailPanel；
              // 借用一个轻量容器提供 overflow:auto + 返回按钮。
              <div style={{ height: '100%', overflow: 'auto' }}>
                {isMobile && (
                  <div style={{ padding: '8px 12px', borderBottom: '1px solid var(--color-border, #e2e8f0)' }}>
                    <button
                      onClick={() => setSelectedLoopId(null)}
                      style={{
                        background: 'none', border: 'none', cursor: 'pointer',
                        display: 'flex', alignItems: 'center', gap: 4,
                        color: 'var(--color-text-secondary, #475569)', fontSize: 14,
                      }}
                    >
                      <LeftOutlined /> 返回
                    </button>
                  </div>
                )}
                <LoopDetailPanel
                  loopId={selectedLoopId}
                  tags={state.tags}
                  onTrigger={async () => {
                    try {
                      const res = await dbLoops.triggerLoop(selectedLoopId);
                      message.success(`已触发 (execution #${res.execution_id})`);
                    } catch (err) {
                      // 触发失败时给用户反馈，避免静默吞掉错误
                      message.error(`触发失败: ${err instanceof Error ? err.message : '未知错误'}`);
                    }
                  }}
                  onDuplicate={async () => {
                    try {
                      await dbLoops.duplicateLoop(selectedLoopId);
                      message.success('已复制');
                    } catch (err) {
                      // 复制失败时给用户反馈，避免静默吞掉错误
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
                      // 状态切换失败时给用户反馈，避免静默吞掉错误
                      message.error(`状态切换失败: ${err instanceof Error ? err.message : '未知错误'}`);
                    }
                  }}
                  onChanged={() => {
                    setLoopUpdateCount(c => c + 1);
                  }}
                />
              </div>
            ) : activeNavKey === 'settings_runtime' ? (
              // 运行管理 — 独立页面（非设置内嵌标签页）
              <div className="detail-panel" style={{ height: '100%', overflowY: 'auto', padding: 16 }}>
                <RuntimePanel
                  configForm={runtimeConfigForm}
                  configSaving={runtimeConfigSaving}
                  handleSaveConfig={handleRuntimeSaveConfig}
                  executorDisplayNames={runtimeExecutorDisplayNames}
                />
              </div>
            ) : activeNavKey === 'settings_skills' ? (
              <div className="detail-panel" style={{ height: '100%', overflowY: 'auto', padding: 16 }}>
                <SkillsPanel />
              </div>
            ) : activeNavKey === 'settings_projectDirectories' ? (
              <div className="detail-panel" style={{ height: '100%', overflowY: 'auto', padding: 16 }}>
                <ProjectDirectoriesPanel />
              </div>
            ) : activeNavKey === 'settings_sessions' ? (
              <div className="detail-panel" style={{ height: '100%', overflowY: 'auto', padding: 16 }}>
                <SessionManager />
              </div>
            ) : activeView === 'settings' ? (
              <SettingsPage onBack={isMobile ? backToList : undefined} />
            ) : activeView === 'memorial' ? (
              <MemorialBoard onBack={isMobile ? backToList : undefined} />
            ) : (
              <Dashboard onBack={isMobile ? backToList : undefined} />
            )}
          </div>
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
          activeKey={activeNavKey}
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
