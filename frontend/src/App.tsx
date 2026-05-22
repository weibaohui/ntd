import { useState, useEffect } from 'react';
import { ConfigProvider, Layout, Spin, App as AntApp } from 'antd';
import { PlusOutlined, ThunderboltOutlined, CloseOutlined } from '@ant-design/icons';
import { AppProvider, useApp } from './hooks/useApp';
import { useIsMobile } from './hooks/useIsMobile';
import { useExecutionEvents } from './hooks/useExecutionEvents';
import { ThemeProvider, useTheme } from './hooks/useTheme';
import { TodoList } from './components/TodoList';
import { TodoDetail } from './components/TodoDetail';
import { Dashboard } from './components/Dashboard';
import { MemorialBoard } from './components/MemorialBoard';
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
  const [todoModalOpen, setTodoModalOpen] = useState(false);
  const [smartCreateOpen, setSmartCreateOpen] = useState(false);
  const [fabExpanded, setFabExpanded] = useState(false);
  const [appConfig, setAppConfig] = useState<Config | null>(null);
  const isMobile = useIsMobile();
  const [selectedPanel, setSelectedPanel] = useState<'list' | 'detail'>('list');

  // Read initial state from URL
  const [activeView, setActiveView] = useState<'dashboard' | 'settings' | 'memorial'>(() => {
    const params = new URLSearchParams(window.location.search);
    const view = params.get('view');
    if (view === 'settings' || view === 'memorial') return view;
    return 'dashboard';
  });

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

  // 加载配置
  useEffect(() => {
    db.getConfig().then(setAppConfig).catch(() => {});
  }, []);

  // Push URL state helper
  const pushUrl = (view: string, todoId?: string | number | null) => {
    const params = new URLSearchParams();
    if (todoId) {
      params.set('todo', String(todoId));
    } else if (view !== 'dashboard') {
      params.set('view', view);
    }
    const qs = params.toString();
    const url = qs ? `/?${qs}` : '/';
    window.history.pushState(null, '', url);
  };

  // Handle browser back/forward
  useEffect(() => {
    const onPopState = () => {
      const params = new URLSearchParams(window.location.search);
      const todoId = params.get('todo');
      const view = params.get('view');

      if (todoId) {
        dispatch({ type: 'SELECT_TODO', payload: Number(todoId) });
        setSelectedPanel('detail');
      } else {
        dispatch({ type: 'SELECT_TODO', payload: null });
        if (view === 'settings' || view === 'memorial') {
          setActiveView(view);
          setSelectedPanel('detail');
        } else {
          setActiveView('dashboard');
          setSelectedPanel('list');
        }
      }
    };
    window.addEventListener('popstate', onPopState);
    return () => window.removeEventListener('popstate', onPopState);
  }, [dispatch]);

  // On initial load, restore todo selection from URL
  useEffect(() => {
    if (state.loading) return;
    const params = new URLSearchParams(window.location.search);
    const todoId = params.get('todo');
    if (todoId && state.todos.some(t => String(t.id) === todoId)) {
      dispatch({ type: 'SELECT_TODO', payload: Number(todoId) });
      setSelectedPanel('detail');
    }
  }, [state.loading, state.todos, dispatch]);

  if (state.loading) {
    return (
      <div className="flex-center" style={{ height: '100vh' }}>
        <Spin size="large" description="加载中..." />
      </div>
    );
  }

  const handleSelectTodo = (todoId: string | number | null) => {
    if (todoId != null) {
      setSelectedPanel('detail');
      pushUrl(activeView, todoId);
    }
  };

  const handleShowMemorial = () => {
    clearSelection();
    setActiveView('memorial');
    setSelectedPanel('detail');
    pushUrl('memorial');
  };

  const handleShowDashboard = () => {
    clearSelection();
    setActiveView('dashboard');
    setSelectedPanel('detail');
    pushUrl('dashboard');
  };

  const handleShowSettings = () => {
    clearSelection();
    setActiveView('settings');
    setSelectedPanel('detail');
    pushUrl('settings');
  };


  const handleBackToList = () => {
    clearSelection();
    setActiveView('dashboard');
    setSelectedPanel('list');
    pushUrl('dashboard');
  };

  const handleSmartCreateSubmitted = () => {
    // 刷新 todo 列表
    db.getAllTodos().then(todos => {
      dispatch({ type: 'SET_TODOS', payload: todos });
    });
  };

  const handleGoToSettings = () => {
    handleShowSettings();
  };

  // 点击 FAB 外部收起
  const handleFabBackdropClick = () => {
    setFabExpanded(false);
  };

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
                    onClick={() => {
                      setFabExpanded(false);
                      setSmartCreateOpen(true);
                    }}
                    aria-label="智能新建"
                  >
                    <ThunderboltOutlined style={{ fontSize: 20, color: '#fff' }} />
                  </button>
                </div>
                <div className="mobile-fab-item" style={{ animationDelay: '50ms' }}>
                  <span className="mobile-fab-item-label">新建</span>
                  <button
                    className="mobile-fab-item-btn mobile-fab-create"
                    onClick={() => {
                      setFabExpanded(false);
                      setTodoModalOpen(true);
                    }}
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
              {fabExpanded ? (
                <CloseOutlined style={{ fontSize: 22, color: '#fff' }} />
              ) : (
                <PlusOutlined style={{ fontSize: 24, color: '#fff' }} />
              )}
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
              onShowDashboard={handleShowDashboard}
              onShowMemorial={handleShowMemorial}
              onShowSettings={handleShowSettings}
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
                <TodoDetail onBack={isMobile ? handleBackToList : undefined} />
              ) : activeView === 'settings' ? (
                <SettingsPage onBack={isMobile ? handleBackToList : undefined} />
              ) : activeView === 'memorial' ? (
                <MemorialBoard onBack={isMobile ? handleBackToList : undefined} />
              ) : (
                <Dashboard onBack={isMobile ? handleBackToList : undefined} />
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
    <ConfigProvider
      locale={zhCN}
      theme={themeConfig}
    >
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
