import React, { createContext, useContext, useReducer, useMemo, ReactNode } from 'react';

// ─── State & Reducer ─────────────────────────────────────────

interface UIState {
  loading: boolean;
}

type UIAction = { type: 'SET_LOADING'; payload: boolean };

const initialState: UIState = { loading: true };

function reducer(state: UIState, action: UIAction): UIState {
  switch (action.type) {
    case 'SET_LOADING': return { ...state, loading: action.payload };
    default: return state;
  }
}

// ─── Context ──────────────────────────────────────────────────

const UIContext = createContext<{ state: UIState; dispatch: React.Dispatch<UIAction> } | null>(null);
// 093 批次2：dispatch 独立 context（与 Todo/Execution 同款双 context 拆分）
const UIDispatchContext = createContext<React.Dispatch<UIAction> | null>(null);

// ─── Provider ─────────────────────────────────────────────────

export function UIProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, initialState);
  const ctx = useMemo(() => ({ state, dispatch }), [state, dispatch]);
  return (
    <UIDispatchContext.Provider value={dispatch}>
      <UIContext.Provider value={ctx}>{children}</UIContext.Provider>
    </UIDispatchContext.Provider>
  );
}

// ─── Hook ─────────────────────────────────────────────────────

export function useUI() {
  const ctx = useContext(UIContext);
  if (!ctx) throw new Error('useUI must be used within UIProvider');
  return ctx;
}

/** 只取 dispatch（引用恒定）：订阅它不会因 UI state 变化重渲染。 */
export function useUIDispatch() {
  const dispatch = useContext(UIDispatchContext);
  if (!dispatch) throw new Error('useUIDispatch must be used within UIProvider');
  return dispatch;
}

export type { UIAction };
