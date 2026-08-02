import React, { createContext, useContext, useReducer, useMemo, ReactNode } from 'react';
import type { Tag } from '@/types';

// ─── Design Overview ──────────────────────────────────────────
//
// 056 简化（决策 2a）：
// - `todosByWorkspace` 全量桶已彻底删除——列表数据一律走服务端分页/轻量接口，
//   单条详情走 `useTodoById` 按需查询，不再在客户端维护全量镜像。
// - 这里只保留 tags（基数小、按 id 高频查找）与选择态（selectedTodoId/TagId/Workspace）。
//
// selectedTodoId / selectedTagId 语义不变：
// null = "nothing selected"。

// ─── State ───────────────────────────────────────────────────

interface TodoState {
  tags: Tag[];
  /** null = no todo selected (list view); number = detail view */
  selectedTodoId: number | null;
  /** null = no tag selected (show all); number = filtered view */
  selectedTagId: number | null;
  /**
   * null = show all workspaces; number = filter by workspace id (唯一键).
   * 组件间一律传 id，path 仅后端内部 cwd 用。
   */
  selectedWorkspace: number | null;
}

// ─── Actions ─────────────────────────────────────────────────

type TodoAction =
  // ── 全量替换 tags（数据量极小，不分桶）──
  | { type: 'SET_TAGS'; payload: Tag[] }

  // ── 选择态 ─
  | { type: 'SELECT_TODO'; payload: number | null }
  | { type: 'SELECT_TAG'; payload: number | null }
  | { type: 'SELECT_WORKSPACE'; payload: number | null }

  // ── tag CRUD ─
  | { type: 'ADD_TAG'; payload: Tag }
  | { type: 'DELETE_TAG'; payload: number };

// 从 localStorage 读取上次选中的 workspace id，刷新后保持选择。
// 字符串 → 数字：旧数据可能残留 path 字符串，统一按 Number 解析；失败时回退到 null。
function getInitialWorkspace(): number | null {
  try {
    const saved = localStorage.getItem('selected_workspace');
    if (!saved) return null;
    const n = Number(saved);
    return Number.isFinite(n) && n > 0 ? n : null;
  } catch {
    return null;
  }
}

const initialState: TodoState = {
  tags: [],
  selectedTodoId: null,
  selectedTagId: null,
  selectedWorkspace: getInitialWorkspace(),
};

// ─── Reducer ─────────────────────────────────────────────────

function reducer(state: TodoState, action: TodoAction): TodoState {
  switch (action.type) {
    case 'SET_TAGS': return { ...state, tags: action.payload };

    // 选择态：payload 与当前值相等时直接返回原 state，避免产生新引用触发依赖重渲染。
    // 这是阻断 React error #185 无限循环的第二道防线：即便上层 effect 无条件 dispatch，
    // reducer 不产生新 state，下游 useMemo 也不会重算。
    case 'SELECT_TODO':
      if (action.payload === state.selectedTodoId) return state;
      return { ...state, selectedTodoId: action.payload };
    case 'SELECT_TAG':
      if (action.payload === state.selectedTagId) return state;
      return { ...state, selectedTagId: action.payload };
    case 'SELECT_WORKSPACE': {
      // workspace 切换相等时跳过 localStorage 写入与 state 更新，保持引用稳定。
      if (action.payload === state.selectedWorkspace) return state;
      try {
        if (action.payload != null) {
          localStorage.setItem('selected_workspace', String(action.payload));
        } else {
          localStorage.removeItem('selected_workspace');
        }
      } catch {}
      return { ...state, selectedWorkspace: action.payload };
    }

    case 'ADD_TAG': return { ...state, tags: [...state.tags, action.payload] };
    case 'DELETE_TAG': return { ...state, tags: state.tags.filter(t => t.id !== action.payload) };

    default: return state;
  }
}

// ─── Context ──────────────────────────────────────────────────

const TodoContext = createContext<{ state: TodoState; dispatch: React.Dispatch<TodoAction> } | null>(null);

// ─── Provider ─────────────────────────────────────────────────

export function TodoProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, initialState);
  const ctx = useMemo(() => ({ state, dispatch }), [state]);
  return <TodoContext.Provider value={ctx}>{children}</TodoContext.Provider>;
}

// ─── Hooks ─────────────────────────────────────────────────────

export function useTodos() {
  const ctx = useContext(TodoContext);
  if (!ctx) throw new Error('useTodos must be used within TodoProvider');
  return ctx;
}

export type { TodoAction };
