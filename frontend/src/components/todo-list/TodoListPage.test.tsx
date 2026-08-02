import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { TodoListPage } from './TodoListPage';
import type { TodoCenterItem } from '@/types';

vi.mock('@/hooks/useApp', () => ({
  useApp: () => ({
    state: {
      selectedWorkspace: 1,
      tags: [],
    },
  }),
}));

vi.mock('@/hooks/useIsMobile', () => ({
  useIsMobile: () => false,
}));

vi.mock('@/constants', () => ({
  TODO_LIST_REFRESH_EVENT: 'todo-list-refresh',
}));

vi.mock('@/utils/database', () => ({
  // 056：getTodoCenter 响应为分页结构 { items, total, page, page_size, bucket_counts, action_types }
  getTodoCenter: vi.fn().mockResolvedValue({
    items: [], total: 0, page: 1, page_size: 20, bucket_counts: {}, action_types: [],
  }),
  deleteTodo: vi.fn().mockResolvedValue(undefined),
  executeTodo: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('@/components/TodoCenterCardView', () => ({
  TodoCenterCardView: (props: {
    onSelectTodo: (id: number) => void;
    onSelectLoop: (id: number) => void;
    isMobile: boolean;
    searchKeyword: string;
    extra: React.ReactNode;
    refreshKey: number;
  }) => (
    <div data-testid="mock-todo-center-card-view">
      <button onClick={() => props.onSelectTodo(1)} data-testid="mock-card-click">Card</button>
    </div>
  ),
}));

vi.mock('@/components/todo-list/TodoListView', () => ({
  TodoListView: (props: {
    items: TodoCenterItem[];
    loading: boolean;
    tags: Array<{ id: number; name: string; color: string }>;
    onSelectTodo: (id: number) => void;
    onEditTodo: (todo: TodoCenterItem) => void;
    onDeleteTodo: (todo: TodoCenterItem) => void;
    onExecuteTodo: (todo: TodoCenterItem) => void;
    onExecuteWithArgs: (todo: TodoCenterItem) => void;
    onRefresh: () => void;
  }) => (
    <div data-testid="mock-todo-list-view">
      <button onClick={() => props.onSelectTodo(1)} data-testid="mock-row-click">Row</button>
    </div>
  ),
}));

vi.mock('@/components/common/PageCard', () => ({
  PageCard: (props: {
    icon?: React.ReactNode;
    title?: string;
    extra?: React.ReactNode;
    children?: React.ReactNode;
    style?: React.CSSProperties;
    contentStyle?: React.CSSProperties;
  }) => (
    <div data-testid="mock-page-card">
      <div data-testid="mock-page-card-title">{props.title}</div>
      <div data-testid="mock-page-card-extra">{props.extra}</div>
      {props.children}
    </div>
  ),
}));

describe('TodoListPage', () => {
  const mockOnSelectTodo = vi.fn();
  const mockOnSelectLoop = vi.fn();
  const mockOnOpenCreateModal = vi.fn();
  const mockOnEditTodo = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders card view by default', async () => {
    render(
      <TodoListPage
        onSelectTodo={mockOnSelectTodo}
        onSelectLoop={mockOnSelectLoop}
        onOpenCreateModal={mockOnOpenCreateModal}
        onEditTodo={mockOnEditTodo}
      />,
    );

    expect(screen.getByTestId('mock-todo-center-card-view')).toBeInTheDocument();
  });

  it('calls onSelectTodo when card is clicked', async () => {
    render(
      <TodoListPage
        onSelectTodo={mockOnSelectTodo}
        onSelectLoop={mockOnSelectLoop}
        onOpenCreateModal={mockOnOpenCreateModal}
        onEditTodo={mockOnEditTodo}
      />,
    );

    fireEvent.click(screen.getByTestId('mock-card-click'));
    expect(mockOnSelectTodo).toHaveBeenCalledWith(1);
  });

  it('passes onOpenCreateModal to header', async () => {
    render(
      <TodoListPage
        onSelectTodo={mockOnSelectTodo}
        onSelectLoop={mockOnSelectLoop}
        onOpenCreateModal={mockOnOpenCreateModal}
        onEditTodo={mockOnEditTodo}
      />,
    );

    expect(screen.getByTestId('mock-todo-center-card-view')).toBeInTheDocument();
  });
});