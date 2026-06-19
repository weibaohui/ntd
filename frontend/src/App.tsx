import { useState, useEffect, useCallback } from 'react';
import { ConfigProvider, Layout, App as AntApp, message } from 'antd';
import { PlusOutlined, ThunderboltOutlined, CloseOutlined, LeftOutlined } from '@ant-design/icons';
import { AppProvider, useApp } from './hooks/useApp';
import { useIsMobile } from './hooks/useIsMobile';
import { useExecutionEvents } from './hooks/useExecutionEvents';
import { useViewState } from './hooks/useViewState';
import { ThemeProvider, useTheme } from './hooks/useTheme';
import { TodoList } from './components/TodoList';
import { TodoDetail } from './components/TodoDetail';
import { Dashboard } from './components/Dashboard';
import { MemorialBoard } from './components/MemorialBoard';
import { RelationMap } from './components/relation-map';
import { SettingsPage } from './components/SettingsPage';
import { ExecutionPanel } from './components/ExecutionPanel';
import { TodoDrawer } from './components/TodoDrawer';
import { SmartCreateModal } from './components/SmartCreateModal';
import { StepList } from './components/StepList';
import { LoopStudio } from './components/LoopStudio';
import { LoopDetailPanel } from './components/LoopStudioDetailPanel';
import * as dbLoops from './utils/database/loops';
import { EXECUTION_PANEL, SIDEBAR_WIDTH } from './constants';
import * as db from './utils/database';
import type { Config } from './types';
import zhCN from 'antd/locale/zh_CN';
import './App.css';

const { Content } = Layout;

function AppContent() {
  const { state, dispatch, clearSelection } = useApp();
  const { activeView, selectedPanel, setSelectedPanel, showView, selectTodo, backToList } = useViewState();

  const [todoModalOpen, setTodoModalOpen] = useState(false);
  const [smartCreateOpen, setSmartCreateOpen] = useState(false);
  const [fabExpanded, setFabExpanded] = useState(false);
  const [appConfig, setAppConfig] = useState<Config | null>(null);
  // 从左侧环路列表选中某个 loop 时记录其 id，右侧面板展示 LoopDetailPanel
  const [selectedLoopId, setSelectedLoopId] = useState<number | null>(null);
  const isMobile = useIsMobile();

  const [panelCollapsed, setPanelCollapsed] = useState(() => {
    try {
      return localStorage.getItem('execution_panel_collapsed') === 'true';
    } catch {
      return false;
    }
  });

  useExecutionEvents();

  const hasRunningTasks = Object.keys(state.runningTasks).length > 0;
  const panelHeight = hasRunningTasks ? (panelCollapsed ? EXECUTION_PANEL.collapsed : EXECUTION_PANEL.expanded) : 0;

  // Load app config on mount
  useEffect(() => {
    db.getConfig().then(setAppConfig).catch(() => {});
  }, []);

  // On initial load, restore todo selection from URL (only when loading finishes)
  useEffect(() => {
    if (state.loading) return;
    const params = new URLSearchParams(window.location.search);
    const todoId = params.get('todo');
    if (todoId && state.todos.some(t => String(t.id) === todoId)) {
      dispatch({ type: 'SELECT_TODO', payload: Number(todoId) });
      setSelectedPanel('detail');
    }
  }, [state.loading, state.todos, dispatch, setSelectedPanel]);

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
    setSelectedLoopId(loopId);
    setSelectedPanel('detail');
  }, [setSelectedPanel]);

  const handleSmartCreateSubmitted = () => {
    db.getAllTodos().then(todos => {
      dispatch({ type: 'SET_TODOS', payload: todos });
    });
  };

  const handleGoToSettings = () => showView('settings');

  // FAB backdrop click to collapse
  const handleFabBackdropClick = () => setFabExpanded(false);

  return (
    <Layout style={{ height: '100vh' }}>
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

      <Layout>
        <Content
          style={{
            display: 'flex',
            flexDirection: isMobile ? 'column' : 'row',
            padding: isMobile ? 0 : 16,
            paddingBottom: isMobile ? 0 : 16 + panelHeight,
            gap: isMobile ? 0 : 16,
            height: `calc(100vh - ${panelHeight}px)`,
            overflow: 'hidden',
            transition: 'height 0.3s ease, padding-bottom 0.3s ease',
          }}
        >
          {/* Todo List Panel */}
          <div
            className={(!isMobile || selectedPanel === 'list') ? 'animate-fade-in' : ''}
            style={{
              width: isMobile ? SIDEBAR_WIDTH.mobile : SIDEBAR_WIDTH.desktop,
              flexShrink: 0,
              height: '100%',
              display: !isMobile || selectedPanel === 'list' ? 'block' : 'none',
            }}
          >
            <TodoList
              onOpenCreateModal={() => setTodoModalOpen(true)}
              onOpenSmartCreate={() => setSmartCreateOpen(true)}
              onSelectTodo={handleSelectTodo}
              onShowDashboard={() => { clearSelection(); showView('dashboard'); }}
              onShowMemorial={() => { clearSelection(); showView('memorial'); }}
              onShowRelationMap={() => { clearSelection(); showView('relation'); }}
              onShowSteps={() => { clearSelection(); showView('steps'); }}
              onShowLoop={() => { clearSelection(); showView('loop'); }}
              onShowSettings={() => { clearSelection(); showView('settings'); }}
              onSelectLoop={handleSelectLoop}
            />
          </div>

          {/* Detail Panel */}
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
                  onTrigger={async () => {
                    try {
                      const res = await dbLoops.triggerLoop(selectedLoopId);
                      message.success(`已触发 (execution #${res.execution_id})`);
                    } catch { /* ignore */ }
                  }}
                  onDuplicate={async () => {
                    try {
                      await dbLoops.duplicateLoop(selectedLoopId);
                      message.success('已复制');
                    } catch { /* ignore */ }
                  }}
                  onDelete={async () => {
                    try {
                      await dbLoops.deleteLoop(selectedLoopId);
                      message.success('已删除');
                      setSelectedLoopId(null);
                    } catch { /* ignore */ }
                  }}
                  onToggleStatus={async () => {
                    try {
                      const loops = await dbLoops.listLoops();
                      const loop = loops.find(l => l.id === selectedLoopId);
                      if (!loop) return;
                      const next = loop.status === 'enabled' ? 'paused' : 'enabled';
                      await dbLoops.updateLoopStatus(selectedLoopId, { status: next } as any);
                      message.success(`已${next === 'enabled' ? '启用' : '暂停'}`);
                    } catch { /* ignore */ }
                  }}
                  onChanged={() => {
                    // detail 变更后，如果左侧有 LoopStudio 也通知刷新
                  }}
                />
              </div>
            ) : activeView === 'settings' ? (
              <SettingsPage onBack={isMobile ? backToList : undefined} />
            ) : activeView === 'memorial' ? (
              <MemorialBoard onBack={isMobile ? backToList : undefined} />
            ) : activeView === 'relation' ? (
              <RelationMap onBack={isMobile ? backToList : undefined} />
            ) : activeView === 'steps' ? (
              <StepList onBack={isMobile ? backToList : undefined} />
            ) : activeView === 'loop' ? (
              <LoopStudio onBack={isMobile ? backToList : undefined} />
            ) : (
              <Dashboard onBack={isMobile ? backToList : undefined} />
            )}
          </div>
        </Content>
      </Layout>

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
