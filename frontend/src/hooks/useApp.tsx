import React, { createContext, useContext, useReducer, useEffect, useMemo, ReactNode } from 'react';
import { Todo, Tag, ExecutionRecord, RunningTask, LogEntry, TodoItem, ExecutionStats } from '../types';
import * as db from '../utils/database';

// --- Todo Context ---

interface TodoState {
  todos: Todo[];
  tags: Tag[];
  selectedTodoId: number | null;
  selectedTagId: number | null;
}

type TodoAction =
  | { type: 'SET_TODOS'; payload: Todo[] }
  | { type: 'SET_TAGS'; payload: Tag[] }
  | { type: 'ADD_TODO'; payload: Todo }
  | { type: 'UPDATE_TODO'; payload: Todo }
  | { type: 'DELETE_TODO'; payload: number }
  | { type: 'SELECT_TODO'; payload: number | null }
  | { type: 'SELECT_TAG'; payload: number | null }
  | { type: 'ADD_TAG'; payload: Tag }
  | { type: 'DELETE_TAG'; payload: number }
  | { type: 'UPDATE_TODO_STATUS'; payload: { id: number; status: string } };

const todoInitialState: TodoState = {
  todos: [],
  tags: [],
  selectedTodoId: null,
  selectedTagId: null,
};

function todoReducer(state: TodoState, action: TodoAction): TodoState {
  switch (action.type) {
    case 'SET_TODOS':
      return { ...state, todos: action.payload };
    case 'SET_TAGS':
      return { ...state, tags: action.payload };
    case 'ADD_TODO':
      return { ...state, todos: [action.payload, ...state.todos] };
    case 'UPDATE_TODO':
      return { ...state, todos: state.todos.map(t => t.id === action.payload.id ? action.payload : t) };
    case 'DELETE_TODO':
      return { ...state, todos: state.todos.filter(t => t.id !== action.payload) };
    case 'SELECT_TODO':
      return { ...state, selectedTodoId: action.payload };
    case 'SELECT_TAG':
      return { ...state, selectedTagId: action.payload };
    case 'ADD_TAG':
      return { ...state, tags: [...state.tags, action.payload] };
    case 'DELETE_TAG':
      return { ...state, tags: state.tags.filter(t => t.id !== action.payload) };
    case 'UPDATE_TODO_STATUS':
      return {
        ...state,
        todos: state.todos.map(t =>
          t.id === action.payload.id
            ? { ...t, status: action.payload.status as Todo['status'], updated_at: new Date().toISOString() }
            : t
        ),
      };
    default:
      return state;
  }
}

const TodoContext = createContext<{
  state: TodoState;
  dispatch: React.Dispatch<TodoAction>;
} | null>(null);

// --- Execution Context ---

interface ExecutionState {
  executionRecords: Record<number, ExecutionRecord[]>;
  runningTasks: Record<string, RunningTask>;
  activeTaskId: string | null;
}

type ExecutionAction =
  | { type: 'SET_EXECUTION_RECORDS'; payload: { todoId: number; records: ExecutionRecord[] } }
  | { type: 'ADD_EXECUTION_RECORD'; payload: { todoId: number; record: ExecutionRecord } }
  | { type: 'UPDATE_EXECUTION_RECORD'; payload: { todoId: number; record: ExecutionRecord } }
  | { type: 'ADD_RUNNING_TASK'; payload: RunningTask }
  | { type: 'APPEND_TASK_LOG'; payload: { taskId: string; log: LogEntry } }
  | { type: 'FINISH_TASK'; payload: { taskId: string; todoId: number; success: boolean; result: string | null } }
  | { type: 'REMOVE_RUNNING_TASK'; payload: string }
  | { type: 'CLEAR_RUNNING_TASKS' }
  | { type: 'SET_ACTIVE_TASK'; payload: string | null }
  | { type: 'UPDATE_TASK_TODO_PROGRESS'; payload: { taskId: string; progress: TodoItem[] } }
  | { type: 'UPDATE_TASK_EXECUTION_STATS'; payload: { taskId: string; stats: ExecutionStats } };

const executionInitialState: ExecutionState = {
  executionRecords: {},
  runningTasks: {},
  activeTaskId: null,
};

function executionReducer(state: ExecutionState, action: ExecutionAction): ExecutionState {
  switch (action.type) {
    case 'SET_EXECUTION_RECORDS':
      return {
        ...state,
        executionRecords: {
          ...state.executionRecords,
          [action.payload.todoId]: action.payload.records,
        },
      };
    case 'ADD_EXECUTION_RECORD':
      return {
        ...state,
        executionRecords: {
          ...state.executionRecords,
          [action.payload.todoId]: [
            action.payload.record,
            ...(state.executionRecords[action.payload.todoId] || []),
          ],
        },
      };
    case 'UPDATE_EXECUTION_RECORD':
      return {
        ...state,
        executionRecords: {
          ...state.executionRecords,
          [action.payload.todoId]: (state.executionRecords[action.payload.todoId] || []).map(
            r => r.id === action.payload.record.id ? action.payload.record : r
          ),
        },
      };
    case 'ADD_RUNNING_TASK': {
      const task = action.payload;
      return {
        ...state,
        runningTasks: { ...state.runningTasks, [task.taskId]: task },
        activeTaskId: state.activeTaskId || task.taskId,
      };
    }
    case 'APPEND_TASK_LOG': {
      const { taskId, log } = action.payload;
      const task = state.runningTasks[taskId];
      if (!task) return state;
      return {
        ...state,
        runningTasks: {
          ...state.runningTasks,
          [taskId]: { ...task, logs: [...task.logs, log] },
        },
      };
    }
    case 'FINISH_TASK': {
      const { taskId, success, result } = action.payload;
      const task = state.runningTasks[taskId];
      if (!task) return state;
      const now = new Date().toISOString();
      return {
        ...state,
        runningTasks: {
          ...state.runningTasks,
          [taskId]: {
            ...task,
            status: 'finished' as const,
            success,
            result,
            finishedAt: now,
          },
        },
      };
    }
    case 'REMOVE_RUNNING_TASK': {
      const taskId = action.payload;
      const { [taskId]: _, ...rest } = state.runningTasks;
      const remainingIds = Object.keys(rest);
      return {
        ...state,
        runningTasks: rest,
        activeTaskId: state.activeTaskId === taskId
          ? (remainingIds[0] || null)
          : state.activeTaskId,
      };
    }
    case 'CLEAR_RUNNING_TASKS':
      return { ...state, runningTasks: {}, activeTaskId: null };
    case 'SET_ACTIVE_TASK':
      return { ...state, activeTaskId: action.payload };
    case 'UPDATE_TASK_TODO_PROGRESS': {
      const task = state.runningTasks[action.payload.taskId];
      if (!task) return state;
      return {
        ...state,
        runningTasks: {
          ...state.runningTasks,
          [action.payload.taskId]: { ...task, todoProgress: action.payload.progress },
        },
      };
    }
    case 'UPDATE_TASK_EXECUTION_STATS': {
      const task = state.runningTasks[action.payload.taskId];
      if (!task) return state;
      return {
        ...state,
        runningTasks: {
          ...state.runningTasks,
          [action.payload.taskId]: { ...task, executionStats: action.payload.stats },
        },
      };
    }
    default:
      return state;
  }
}

const ExecutionContext = createContext<{
  state: ExecutionState;
  dispatch: React.Dispatch<ExecutionAction>;
} | null>(null);

// --- UI Context ---

interface UIState {
  loading: boolean;
}

type UIAction = { type: 'SET_LOADING'; payload: boolean };

const uiInitialState: UIState = { loading: true };

function uiReducer(state: UIState, action: UIAction): UIState {
  switch (action.type) {
    case 'SET_LOADING':
      return { ...state, loading: action.payload };
    default:
      return state;
  }
}

const UIContext = createContext<{
  state: UIState;
  dispatch: React.Dispatch<UIAction>;
} | null>(null);

// --- Combined types for useApp backward compatibility ---

export type AppState = TodoState & ExecutionState & UIState;
export type Action = TodoAction | ExecutionAction | UIAction;

// --- Providers ---

export function AppProvider({ children }: { children: ReactNode }) {
  const [todoState, todoDispatch] = useReducer(todoReducer, todoInitialState);
  const [execState, execDispatch] = useReducer(executionReducer, executionInitialState);
  const [uiState, uiDispatch] = useReducer(uiReducer, uiInitialState);

  useEffect(() => {
    async function loadData() {
      try {
        const todos = await db.getAllTodos();
        const tags = await db.getAllTags();
        todoDispatch({ type: 'SET_TODOS', payload: todos });
        todoDispatch({ type: 'SET_TAGS', payload: tags });
      } catch (err) {
        // Initial data load failure - dispatch will handle UI state
      } finally {
        uiDispatch({ type: 'SET_LOADING', payload: false });
      }
    }
    loadData();
  }, []);

  const todoCtx = useMemo(() => ({ state: todoState, dispatch: todoDispatch }), [todoState, todoDispatch]);
  const execCtx = useMemo(() => ({ state: execState, dispatch: execDispatch }), [execState, execDispatch]);
  const uiCtx = useMemo(() => ({ state: uiState, dispatch: uiDispatch }), [uiState, uiDispatch]);

  return (
    <UIContext.Provider value={uiCtx}>
      <ExecutionContext.Provider value={execCtx}>
        <TodoContext.Provider value={todoCtx}>
          {children}
        </TodoContext.Provider>
      </ExecutionContext.Provider>
    </UIContext.Provider>
  );
}

// --- Targeted hooks (preferred for new code) ---

export function useTodos() {
  const ctx = useContext(TodoContext);
  if (!ctx) throw new Error('useTodos must be used within AppProvider');
  return ctx;
}

export function useExecution() {
  const ctx = useContext(ExecutionContext);
  if (!ctx) throw new Error('useExecution must be used within AppProvider');
  return ctx;
}

export function useUI() {
  const ctx = useContext(UIContext);
  if (!ctx) throw new Error('useUI must be used within AppProvider');
  return ctx;
}

// --- Combined hook (backward compatibility) ---

export function useApp() {
  const todoCtx = useContext(TodoContext);
  const execCtx = useContext(ExecutionContext);
  const uiCtx = useContext(UIContext);
  if (!todoCtx || !execCtx || !uiCtx) {
    throw new Error('useApp must be used within AppProvider');
  }

  const state: AppState = useMemo(() => ({
    ...todoCtx.state,
    ...execCtx.state,
    ...uiCtx.state,
  }), [todoCtx.state, execCtx.state, uiCtx.state]);

  const dispatch = React.useCallback((action: Action) => {
    switch (action.type) {
      case 'SET_TODOS':
      case 'SET_TAGS':
      case 'ADD_TODO':
      case 'UPDATE_TODO':
      case 'DELETE_TODO':
      case 'SELECT_TODO':
      case 'SELECT_TAG':
      case 'ADD_TAG':
      case 'DELETE_TAG':
      case 'UPDATE_TODO_STATUS':
        todoCtx.dispatch(action);
        break;
      case 'SET_EXECUTION_RECORDS':
      case 'ADD_EXECUTION_RECORD':
      case 'UPDATE_EXECUTION_RECORD':
      case 'ADD_RUNNING_TASK':
      case 'APPEND_TASK_LOG':
      case 'FINISH_TASK':
      case 'REMOVE_RUNNING_TASK':
      case 'CLEAR_RUNNING_TASKS':
      case 'SET_ACTIVE_TASK':
      case 'UPDATE_TASK_TODO_PROGRESS':
      case 'UPDATE_TASK_EXECUTION_STATS':
        execCtx.dispatch(action);
        break;
      case 'SET_LOADING':
        uiCtx.dispatch(action);
        break;
    }
  }, [todoCtx.dispatch, execCtx.dispatch, uiCtx.dispatch]);

  const clearSelection = React.useCallback(() => {
    todoCtx.dispatch({ type: 'SELECT_TODO', payload: null });
    todoCtx.dispatch({ type: 'SELECT_TAG', payload: null });
  }, [todoCtx.dispatch]);

  return useMemo(() => ({
    state,
    dispatch,
    clearSelection,
  }), [state, dispatch, clearSelection]);
}
