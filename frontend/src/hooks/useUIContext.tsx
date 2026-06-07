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

// ─── Provider ─────────────────────────────────────────────────

export function UIProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, initialState);
  const ctx = useMemo(() => ({ state, dispatch }), [state, dispatch]);
  return <UIContext.Provider value={ctx}>{children}</UIContext.Provider>;
}

// ─── Hook ─────────────────────────────────────────────────────

export function useUI() {
  const ctx = useContext(UIContext);
  if (!ctx) throw new Error('useUI must be used within UIProvider');
  return ctx;
}
