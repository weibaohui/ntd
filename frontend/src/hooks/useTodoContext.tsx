import React, { createContext, useContext, useReducer, useMemo, ReactNode } from 'react';
import type { Todo, Tag } from '@/types';

// 模块级空数组常量：useVisibleTodos 在桶不存在时复用此引用，
// 避免每次渲染都 `?? []` 产生新数组，进而触发依赖 state.todos 的 useEffect 无限循环（issue: React error #185）。
const EMPTY_TODOS: Todo[] = [];

// ─── Design Overview ──────────────────────────────────────────
//
// 分桶改造（2026-06-27）：
// - `todos: Todo[]` → `todosByWorkspace: Record<number, Todo[]>`。
//   每个工作空间的 todo 独立放在一个桶里，切换 workspace 时不重拉。
// - `visibleTodos` 由 `useVisibleTodos()` selector 根据
//   `selectedWorkspace` 合成，不再用 flat todos 过滤。
// - 所有 mutation action（ADD / UPDATE / DELETE / UPDATE_STATUS）
//   现在需要 **显式传递 workspaceId**，不再靠遍历扁平数组。
//
// selectedTodoId / selectedTagId 语义不变：
// null = "nothing selected"。

// ─── State ───────────────────────────────────────────────────

interface TodoState {
  /** 按 workspace_id 分桶存储。key = workspace.id，value = 该空间下的 todo 列表。
   *  切换 workspace 时不需要重新拉取（如果已经拉过）。 */
  todosByWorkspace: Record<number, Todo[]>;
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
//
// 分桶改造后，所有 mutation action 都必须携带 workspaceId（ADD / UPDATE / DELETE
// 都要知道操作哪个桶）。跨桶操作在 reducer 内部遍历 todosByWorkspace 查找。

type TodoAction =
  // ── 替换整个桶（从后端拉完一整个 workspace 的数据后调用）─
  | { type: 'SET_TODOS_BY_WORKSPACE'; workspaceId: number; payload: Todo[] }

  // ── 全量替换 tags（数据量极小，不分桶）──
  | { type: 'SET_TAGS'; payload: Tag[] }

  // ── 新增 todo：必须携带 workspaceId，reducer 推入对应桶的顶部 ─
  | { type: 'ADD_TODO'; workspaceId: number; payload: Todo }

  // ── 更新单条 todo：遍历所有桶找到同 id 的替换 ─
  | { type: 'UPDATE_TODO'; payload: Todo }

  // ── 删除单条 todo：遍历所有桶找到同 id 的删除 ─
  | { type: 'DELETE_TODO'; payload: number }

  // ── 快速更新 todo status（Kanban 拖拽专用）─
  | { type: 'UPDATE_TODO_STATUS'; payload: { id: number; status: Todo['status'] } }

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
  todosByWorkspace: {},
  tags: [],
  selectedTodoId: null,
  selectedTagId: null,
  selectedWorkspace: getInitialWorkspace(),
};

// ─── Reducer ─────────────────────────────────────────────────

function reducer(state: TodoState, action: TodoAction): TodoState {
  switch (action.type) {
    // 替换整个桶：同一个 workspace 的 todo 全量覆盖。
    // 如果桶不存在则新建，已存在则替换。
    case 'SET_TODOS_BY_WORKSPACE':
      return {
        ...state,
        todosByWorkspace: {
          ...state.todosByWorkspace,
          [action.workspaceId]: action.payload,
        },
      };

    case 'SET_TAGS': return { ...state, tags: action.payload };

    // 新增：推入对应桶的顶部（最新排最前）。
    // 桶不存在时也创建新桶，避免丢失新增数据。
    case 'ADD_TODO': {
      const bucket = state.todosByWorkspace[action.workspaceId];
      return {
        ...state,
        todosByWorkspace: {
          ...state.todosByWorkspace,
          [action.workspaceId]: bucket
            ? [action.payload, ...bucket]
            : [action.payload],
        },
      };
    }

    // 更新：遍历所有桶，替换同 id 的 todo。
    // 效率 O(n*m)：todolist 总量一般 < 2000，可接受。
    case 'UPDATE_TODO': {
      const updated = action.payload;
      const newBuckets: Record<number, Todo[]> = {};
      let changed = false;
      for (const [key, todos] of Object.entries(state.todosByWorkspace)) {
        const wid = Number(key);
        const idx = todos.findIndex(t => t.id === updated.id);
        if (idx !== -1) {
          const copy = [...todos];
          copy[idx] = updated;
          newBuckets[wid] = copy;
          changed = true;
        } else {
          newBuckets[wid] = todos;
        }
      }
      // 找不到时 TODO 可能刚被创建且桶还不存在 —— 尝试从 payload.workspace_id 推入
      if (!changed && updated.workspace_id != null) {
        const bucket = state.todosByWorkspace[updated.workspace_id] || [];
        newBuckets[updated.workspace_id] = [
          updated,
          ...bucket.filter(t => t.id !== updated.id),
        ];
      }
      return { ...state, todosByWorkspace: newBuckets };
    }

    // 删除：遍历所有桶删除同 id 的 todo。
    case 'DELETE_TODO': {
      const id = action.payload;
      const newBuckets: Record<number, Todo[]> = {};
      for (const [key, todos] of Object.entries(state.todosByWorkspace)) {
        newBuckets[Number(key)] = todos.filter(t => t.id !== id);
      }
      return { ...state, todosByWorkspace: newBuckets };
    }

    // 选择态：payload 与当前值相等时直接返回原 state，避免产生新引用触发依赖重渲染。
    // 这是阻断 React error #185 无限循环的第二道防线：即便上层 effect 无条件 dispatch，
    // reducer 不产生新 state，下游 useMemo（如 useVisibleTodos）也不会重算。
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

    case 'UPDATE_TODO_STATUS': {
      const { id, status } = action.payload;
      const newBuckets: Record<number, Todo[]> = {};
      for (const [key, todos] of Object.entries(state.todosByWorkspace)) {
        newBuckets[Number(key)] = todos.map(t =>
          t.id === id
            ? { ...t, status: status as Todo['status'], updated_at: new Date().toISOString() }
            : t,
        );
      }
      return { ...state, todosByWorkspace: newBuckets };
    }

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

/**
 * 根据当前 selectedWorkspace 返回可见的 todo 列表。
 * - selectedWorkspace 非 null → 返回对应桶的 todos（无桶时返回空数组）。
 * - selectedWorkspace 为 null → 返回所有桶的 todos 扁平合并
 *   （极少数场景，如 BackupPanel 导入去重需要全局视图）。
 */
export function useVisibleTodos(): Todo[] {
  const { state } = useTodos();
  const { selectedWorkspace, todosByWorkspace } = state;
  // useMemo 稳定返回引用：仅当 selectedWorkspace 或 todosByWorkspace 变化时才重新计算。
  // 之前未 memoize 时，桶不存在会每次渲染返回新 `[]`，导致 App.tsx 中依赖 state.todos 的
  // useEffect 反复触发 dispatch SELECT_TODO null，而 reducer 不判等又产生新 state，
  // 形成「渲染 → useEffect → dispatch → 新 state → 渲染」的无限循环（React error #185）。
  return useMemo(() => {
    if (selectedWorkspace != null) {
      // 桶不存在时复用模块级 EMPTY_TODOS，保证相同输入下引用稳定。
      return todosByWorkspace[selectedWorkspace] ?? EMPTY_TODOS;
    }
    return Object.values(todosByWorkspace).flat();
  }, [selectedWorkspace, todosByWorkspace]);
}

export type { TodoAction };
