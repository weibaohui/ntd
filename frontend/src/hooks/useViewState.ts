/**
 * useViewState — URL-driven view navigation state.
 *
 * Manages activeView (dashboard | settings | memorial | relation | steps),
 * selectedPanel (list | detail), and browser history (pushState / popstate).
 * Decoupled from TodoContext so it can be tested / reused independently.
 */

import { useState, useEffect, useCallback } from 'react';

// 'steps' = 环节管理页面 (v3 kind=step)。环路编排通过左栏 Segmented 的"环路"tab 访问。
// 复用现有的 'detail' panel,环节页面本身就是 detail panel 内容。
export type View = 'dashboard' | 'settings' | 'memorial' | 'relation' | 'steps';
export type Panel = 'list' | 'detail';

// 哪些 view 是合法的 URL 参数(popstate / 直接改 URL 都需要校验)
const KNOWN_VIEWS: View[] = ['dashboard', 'settings', 'memorial', 'relation', 'steps'];

// ─── Helpers ─────────────────────────────────────────────────

function getInitialView(): View {
  const view = new URLSearchParams(window.location.search).get('view');
  if (view && (KNOWN_VIEWS as string[]).includes(view)) return view as View;
  return 'dashboard';
}

function viewToPanel(v: View): Panel {
  return v === 'dashboard' ? 'list' : 'detail';
}

// ─── Hook ────────────────────────────────────────────────────

export function useViewState() {
  const [activeView, setActiveView] = useState<View>(getInitialView);
  const [selectedPanel, setSelectedPanel] = useState<Panel>(() => viewToPanel(getInitialView()));

  // Sync view → panel (when view changes externally e.g. popstate)
  useEffect(() => {
    setSelectedPanel(viewToPanel(activeView));
  }, [activeView]);

  // Push URL helper — updates history without triggering popstate
  const pushUrl = useCallback((view: string, todoId?: string | number | null) => {
    const params = new URLSearchParams();
    if (todoId) {
      params.set('todo', String(todoId));
    } else if (view !== 'dashboard') {
      params.set('view', view);
    }
    const qs = params.toString();
    window.history.pushState(null, '', qs ? `/?${qs}` : '/');
  }, []);

  // Browser back/forward handler
  useEffect(() => {
    const onPopState = () => {
      const params = new URLSearchParams(window.location.search);
      const todoId = params.get('todo');
      const view = params.get('view') as View | null;

      if (todoId) {
        // Selecting a todo always opens detail panel
        setSelectedPanel('detail');
        if (view) setActiveView(view);
      } else if (view && (KNOWN_VIEWS as string[]).includes(view)) {
        setActiveView(view);
        setSelectedPanel('detail');
      } else {
        setActiveView('dashboard');
        setSelectedPanel('list');
      }
    };
    window.addEventListener('popstate', onPopState);
    return () => window.removeEventListener('popstate', onPopState);
  }, []);

  // Navigate to a different view (clears todo selection)
  const showView = useCallback((view: View) => {
    setActiveView(view);
    // On mobile, dashboard renders in the detail panel, so always show it
    const panel: Panel = view === 'dashboard' ? 'detail' : viewToPanel(view);
    setSelectedPanel(panel);
    pushUrl(view);
  }, [pushUrl]);

  // Select a todo (opens detail panel, preserves current view)
  // Guard: callers pass Number(todoId) which returns NaN for invalid strings like "";
  // we silently ignore NaN to avoid dispatching a SELECT_TODO with NaN.
  const selectTodo = useCallback((todoId: number) => {
    if (!Number.isFinite(todoId)) return;
    setSelectedPanel('detail');
    pushUrl(activeView, todoId);
  }, [activeView, pushUrl]);

  // Go back to the list panel (dashboard view)
  const backToList = useCallback(() => {
    setActiveView('dashboard');
    setSelectedPanel('list');
    pushUrl('dashboard');
  }, [pushUrl]);

  return {
    activeView,
    selectedPanel,
    setSelectedPanel,
    showView,
    selectTodo,
    backToList,
    pushUrl,
  };
}
