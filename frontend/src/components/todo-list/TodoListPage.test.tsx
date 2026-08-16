import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { TodoListPage } from './TodoListPage';
import * as db from '@/utils/database';
import type { TodoCenterItem } from '@/types';

// 093：组件已从合并版 useApp 迁移到细粒度 useTodos，mock 目标同步切换；
// mock 形状不变（useTodos 同样返回 { state, dispatch }，本组件只读 state）。
vi.mock('@/hooks/useTodoContext', () => ({
  useTodos: () => ({
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
    hours: number | null;
    extra: React.ReactNode;
    refreshKey: number;
  }) => (
    <div data-testid="mock-todo-center-card-view">
      {/* 111：extra 是宿主注入的 header（含时间分段），mock 也必须渲染它，
          否则「全部/24h」等时间选项在测试里不可见。 */}
      <div data-testid="mock-card-extra">{props.extra}</div>
      {/* 111：把 hours 渲染出来供断言卡片视图是否收到时间窗 */}
      <div data-testid="mock-card-hours">{String(props.hours)}</div>
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

// 111：running 形态渲染 RunningBoard（自带统计栏+实时 WS），测试中替换为静态桩，
// 避免真实组件在 jsdom 里拉数据/建 WS。
vi.mock('@/components/running-board', () => ({
  RunningBoard: () => <div data-testid="mock-running-board" />,
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
    // 111：视图形态由 URL hash ?view= 决定，用例间必须复位，避免串台。
    window.history.replaceState(null, '', '/');
  });

  afterEach(() => {
    window.history.replaceState(null, '', '/');
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

  describe('111 时间过滤', () => {
    it('卡片形态默认渲染「全部」时间分段', () => {
      render(
        <TodoListPage
          onSelectTodo={mockOnSelectTodo}
          onSelectLoop={mockOnSelectLoop}
          onOpenCreateModal={mockOnOpenCreateModal}
          onEditTodo={mockOnEditTodo}
        />,
      );
      // showAll 形态含「全部」选项；默认值为 null（全部不过滤）
      expect(screen.getByText('全部')).toBeInTheDocument();
      expect(screen.getByTestId('mock-card-hours')).toHaveTextContent('null');
    });

    it('选择 24h 后卡片视图收到 hours=24，切回全部恢复 null', () => {
      render(
        <TodoListPage
          onSelectTodo={mockOnSelectTodo}
          onSelectLoop={mockOnSelectLoop}
          onOpenCreateModal={mockOnOpenCreateModal}
          onEditTodo={mockOnEditTodo}
        />,
      );
      fireEvent.click(screen.getByText('24h'));
      expect(screen.getByTestId('mock-card-hours')).toHaveTextContent('24');
      fireEvent.click(screen.getByText('全部'));
      expect(screen.getByTestId('mock-card-hours')).toHaveTextContent('null');
    });

    it('列表形态选择 6h 后 getTodoCenter 请求携带 hours=6', async () => {
      // 109：URL ?view=list 直达列表形态（useViewState 挂载时从 hash 解析）
      window.history.replaceState(null, '', '#/todos?view=list');
      render(
        <TodoListPage
          onSelectTodo={mockOnSelectTodo}
          onSelectLoop={mockOnSelectLoop}
          onOpenCreateModal={mockOnOpenCreateModal}
          onEditTodo={mockOnEditTodo}
        />,
      );
      // 首拉 hours=null（默认全部不过滤）
      await waitFor(() => {
        expect(db.getTodoCenter).toHaveBeenCalledWith(1, expect.objectContaining({ hours: null }));
      });
      fireEvent.click(screen.getByText('6h'));
      // 时间窗变化后重拉，请求必须下推 hours=6（服务端过滤，保证分页口径）
      await waitFor(() => {
        expect(db.getTodoCenter).toHaveBeenCalledWith(1, expect.objectContaining({ hours: 6 }));
      });
    });

    it('执行监控（running）形态不渲染时间分段', () => {
      window.history.replaceState(null, '', '#/todos?view=running');
      render(
        <TodoListPage
          onSelectTodo={mockOnSelectTodo}
          onSelectLoop={mockOnSelectLoop}
          onOpenCreateModal={mockOnOpenCreateModal}
          onEditTodo={mockOnEditTodo}
        />,
      );
      expect(screen.getByTestId('mock-running-board')).toBeInTheDocument();
      // running 态 header 精简：不渲染「全部」（RunningBoard 自带时间统计）
      expect(screen.queryByText('全部')).not.toBeInTheDocument();
    });
  });
});
