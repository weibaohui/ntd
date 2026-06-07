import { useState, useEffect } from 'react';
import { ConfigProvider, Layout, App as AntApp } from 'antd';
import { PlusOutlined, ThunderboltOutlined, CloseOutlined } from '@ant-design/icons';
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
  const panelHeight = hasRunningTasks ? (panelCollapsed ? 40 : 280) : 0;

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
      dispatch({ type: 'SELECT_TODO', payload: Number(todoId) });
      selectTodo(Number(todoId));
    }
  };

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
              width: isMobile ? '100%' : 350,
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
              onShowSettings={() => { clearSelection(); showView('settings'); }}
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
                onBack={isMobile ? () => showView('dashboard') : undefined}
                onShowDashboard={isMobile ? () => showView('dashboard') : undefined}
              />
            ) : activeView === 'settings' ? (
              <SettingsPage onBack={isMobile ? backToList : undefined} />
            ) : activeView === 'memorial' ? (
              <MemorialBoard onBack={isMobile ? backToList : undefined} />
            ) : activeView === 'relation' ? (
              <RelationMap onBack={isMobile ? backToList : undefined} />
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
