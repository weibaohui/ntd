import React, { createContext, useContext, useReducer, useMemo, ReactNode } from 'react';
import type { Todo, Tag } from '@/types';

// ─── Design Overview ──────────────────────────────────────────
//
// This context stores the source-of-truth for todos and tags, plus the two
// selection states (selectedTodoId / selectedTagId).  The separation of
// TodoState from ExecutionState is intentional: components that only care
// about which todo is selected should import useTodos() and not re-render
// when execution records change.
//
// selectedTodoId / selectedTagId can both be null — null means "no filter"
// (show all).  Null is used for both the "all" selection and the "nothing
// selected" state; they are equivalent in the UI so we don't need a third
// sentinel value.

// ─── State ───────────────────────────────────────────────────

interface TodoState {
  todos: Todo[];
  tags: Tag[];
  /** null = no todo selected (list view); number = detail view */
  selectedTodoId: number | null;
  /** null = no tag selected (show all); number = filtered view */
  selectedTagId: number | null;
  /** null = show all workspaces; string = filter by workspace path */
  selectedWorkspace: string | null;
}

// ─── Actions ─────────────────────────────────────────────────
//
// Actions follow the standard CRUD pattern:
//   SET   – replace collection (initial load / reload)
//   ADD   – prepend to collection (optimistic insert)
//   UPDATE – replace single item by id
//   DELETE – remove single item by id
//   SELECT – update UI selection state (not persisted)

type TodoAction =
  | { type: 'SET_TODOS'; payload: Todo[] }          // full reload from server
  | { type: 'SET_TAGS'; payload: Tag[] }             // full reload from server
  | { type: 'ADD_TODO'; payload: Todo }             // optimistic insert (newest first)
  | { type: 'UPDATE_TODO'; payload: Todo }           // inline edit or status change
  | { type: 'DELETE_TODO'; payload: number }        // remove by id
  | { type: 'SELECT_TODO'; payload: number | null } // open detail / close detail
  | { type: 'SELECT_TAG'; payload: number | null }   // filter by tag / clear filter
  | { type: 'SELECT_WORKSPACE'; payload: string | null } // filter by workspace / clear filter
  | { type: 'ADD_TAG'; payload: Tag }               // create tag
  | { type: 'DELETE_TAG'; payload: number }         // remove tag by id
  | { type: 'UPDATE_TODO_STATUS'; payload: { id: number; status: Todo['status'] } }; // quick status toggle

// 从 localStorage 读取上次选中的 workspace，刷新后保持选择
function getInitialWorkspace(): string | null {
  try {
    const saved = localStorage.getItem('selected_workspace');
    return saved || null;
  } catch {
    return null;
  }
}

const initialState: TodoState = {
  todos: [],
  tags: [],
  selectedTodoId: null,
  selectedTagId: null,
  selectedWorkspace: getInitialWorkspace(),
};

// ─── Reducer ─────────────────────────────────────────────────

function reducer(state: TodoState, action: TodoAction): TodoState {
  switch (action.type) {
    case 'SET_TODOS': return { ...state, todos: action.payload };
    case 'SET_TAGS': return { ...state, tags: action.payload };

    // ADD_TODO prepends so new todos appear at the top of the list.
    case 'ADD_TODO': return { ...state, todos: [action.payload, ...state.todos] };

    // UPDATE_TODO replaces the item with the same id; leaves others untouched.
    case 'UPDATE_TODO': return { ...state, todos: state.todos.map(t => t.id === action.payload.id ? action.payload : t) };

    // DELETE_TODO removes by id; safe when id doesn't exist (no-op).
    case 'DELETE_TODO': return { ...state, todos: state.todos.filter(t => t.id !== action.payload) };

    // Selection is stored here so the list and detail panel stay in sync
    // without prop-drilling.  null clears the selection.
    case 'SELECT_TODO': return { ...state, selectedTodoId: action.payload };
    case 'SELECT_TAG': return { ...state, selectedTagId: action.payload };
    case 'SELECT_WORKSPACE': {
      // 持久化到 localStorage，刷新后保持选择
      try {
        if (action.payload) {
          localStorage.setItem('selected_workspace', action.payload);
        } else {
          localStorage.removeItem('selected_workspace');
        }
      } catch {}
      return { ...state, selectedWorkspace: action.payload };
    }

    // ADD_TAG appends the new tag (tags are displayed in insertion order).
    case 'ADD_TAG': return { ...state, tags: [...state.tags, action.payload] };

    // DELETE_TAG removes by id; safe when id doesn't exist.
    case 'DELETE_TAG': return { ...state, tags: state.tags.filter(t => t.id !== action.payload) };

    // UPDATE_TODO_STATUS is a fast-path for Kanban drag-and-drop: only the
    // status field and updated_at change; we avoid a full record fetch.
    case 'UPDATE_TODO_STATUS':
      return {
        ...state,
        todos: state.todos.map(t =>
          t.id === action.payload.id
            ? { ...t, status: action.payload.status as Todo['status'], updated_at: new Date().toISOString() }
            : t
        ),
      };

    default: return state;
  }
}

// ─── Context ──────────────────────────────────────────────────

const TodoContext = createContext<{ state: TodoState; dispatch: React.Dispatch<TodoAction> } | null>(null);

// ─── Provider ─────────────────────────────────────────────────
//
// Memoizes the context value so that useTodos() subscribers only re-render
// when TodoState actually changes, not on every dispatch.

export function TodoProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, initialState);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const ctx = useMemo(() => ({ state, dispatch }), [state]);
  return <TodoContext.Provider value={ctx}>{children}</TodoContext.Provider>;
}

// ─── Hook ─────────────────────────────────────────────────────
//
// Throws if called outside TodoProvider — fail-fast prevents subtle bugs where
// a component gets a null context and silently reads stale state.

export function useTodos() {
  const ctx = useContext(TodoContext);
  if (!ctx) throw new Error('useTodos must be used within TodoProvider');
  return ctx;
}

export type { TodoAction };
