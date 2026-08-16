import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { LoopListPage } from './index';
import * as dbLoops from '@/utils/database/loops';
import type { LoopListItem } from '@/types/loop';

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

vi.mock('@/utils/database/loops', () => ({
  listLoops: vi.fn().mockResolvedValue([]),
  deleteLoop: vi.fn().mockResolvedValue(undefined),
  updateLoopStatus: vi.fn().mockResolvedValue(undefined),
  // 111：LoopKanban 被整体 mock，listExecutions 仅为类型占位，不会被真实调用
  listExecutions: vi.fn().mockResolvedValue({ items: [] }),
}));

vi.mock('@/utils/database/todos', () => ({
  getWorkspaces: vi.fn().mockResolvedValue([{ id: 1, name: 'test', path: '/test' }]),
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

vi.mock('@/components/loop-list/LoopListView', () => ({
  LoopListView: (props: {
    items: LoopListItem[];
    loading: boolean;
    tags: Array<{ id: number; name: string; color: string }>;
    onSelectLoop: (id: number) => void;
    onDelete: (loop: LoopListItem) => void;
    onToggleStatus: (loop: LoopListItem) => void;
    onRefresh: () => void;
  }) => (
    <div data-testid="mock-loop-list-view">
      {/* 111：渲染 items 数量，供时间窗过滤断言 */}
      <div data-testid="mock-loop-count">{props.items.length}</div>
      <button onClick={() => props.onSelectLoop(1)} data-testid="mock-row-click">Row</button>
    </div>
  ),
}));

// 111：LoopKanban 替换为静态桩（真实组件会拉执行历史与渲染 Drawer，jsdom 下无必要），
// 渲染 hours 供断言 kanban 形态的默认值与「全部」切换。
vi.mock('@/components/loop-kanban', () => ({
  LoopKanban: (props: {
    searchText?: string;
    hours?: number | null;
    onOpenTodo?: (todoId: number) => void;
  }) => (
    <div data-testid="mock-loop-kanban" data-hours={String(props.hours)} />
  ),
}));

vi.mock('@/components/settings/workspace/WorkspaceLoopConfigPage', () => ({
  WorkspaceLoopConfigPage: (props: {
    workspace: { id: number; name: string; path: string };
    onBack: () => void;
  }) => (
    <div data-testid="mock-workspace-loop-config">
      <button onClick={props.onBack} data-testid="mock-config-back">Back</button>
    </div>
  ),
}));

describe('LoopListPage', () => {
  // 044：环路列表不再有「新建」入口，测试移除 onCreateLoop 相关用例
  const mockOnSelectLoop = vi.fn();
  const mockOnLoopChanged = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    // 111：URL ?view= 决定形态，用例间复位 hash 与 localStorage 防串台
    window.history.replaceState(null, '', '/');
    localStorage.clear();
  });

  afterEach(() => {
    window.history.replaceState(null, '', '/');
  });

  it('renders page card with correct title', async () => {
    render(
      <LoopListPage
        onSelectLoop={mockOnSelectLoop}
        onLoopChanged={mockOnLoopChanged}
      />,
    );

    expect(screen.getByTestId('mock-page-card')).toBeInTheDocument();
    expect(screen.getByTestId('mock-page-card-title')).toHaveTextContent('环路');
  });

  it('calls onSelectLoop when row is clicked', async () => {
    render(
      <LoopListPage
        onSelectLoop={mockOnSelectLoop}
        onLoopChanged={mockOnLoopChanged}
      />,
    );

    fireEvent.click(screen.getByTestId('mock-row-click'));
    expect(mockOnSelectLoop).toHaveBeenCalledWith(1);
  });

  it('renders search input', async () => {
    render(
      <LoopListPage
        onSelectLoop={mockOnSelectLoop}
        onLoopChanged={mockOnLoopChanged}
      />,
    );

    const searchInput = screen.getByPlaceholderText(/搜索环路名称/i);
    expect(searchInput).toBeInTheDocument();
  });

  it('renders refresh button', async () => {
    render(
      <LoopListPage
        onSelectLoop={mockOnSelectLoop}
        onLoopChanged={mockOnLoopChanged}
      />,
    );

    const refreshBtn = screen.getByRole('button', { name: /刷新/i });
    expect(refreshBtn).toBeInTheDocument();
  });

  describe('111 时间过滤', () => {
    // 构造窗口内（1 小时前）与窗口外（100 小时前）两条环路，验证 created_at 口径
    function makeLoops(): LoopListItem[] {
      const recent = new Date(Date.now() - 1 * 3600 * 1000).toISOString();
      const old = new Date(Date.now() - 100 * 3600 * 1000).toISOString();
      return [
        { id: 1, name: '最近环路', created_at: recent } as LoopListItem,
        { id: 2, name: '老旧环路', created_at: old } as LoopListItem,
      ];
    }

    it('列表形态默认「全部」：不过滤', async () => {
      vi.mocked(dbLoops.listLoops).mockResolvedValueOnce(makeLoops() as never);
      render(
        <LoopListPage
          onSelectLoop={mockOnSelectLoop}
          onLoopChanged={mockOnLoopChanged}
        />,
      );
      await waitFor(() => {
        expect(screen.getByTestId('mock-loop-count')).toHaveTextContent('2');
      });
    });

    it('列表形态选择 24h：只保留窗口内环路', async () => {
      vi.mocked(dbLoops.listLoops).mockResolvedValueOnce(makeLoops() as never);
      render(
        <LoopListPage
          onSelectLoop={mockOnSelectLoop}
          onLoopChanged={mockOnLoopChanged}
        />,
      );
      await waitFor(() => {
        expect(screen.getByTestId('mock-loop-count')).toHaveTextContent('2');
      });
      fireEvent.click(screen.getByText('24h'));
      expect(screen.getByTestId('mock-loop-count')).toHaveTextContent('1');
      // 切回「全部」恢复全量
      fireEvent.click(screen.getByText('全部'));
      expect(screen.getByTestId('mock-loop-count')).toHaveTextContent('2');
    });

    it('kanban 形态默认 24h，切「全部」回传 null', async () => {
      window.history.replaceState(null, '', '#/loops?view=kanban');
      render(
        <LoopListPage
          onSelectLoop={mockOnSelectLoop}
          onLoopChanged={mockOnLoopChanged}
        />,
      );
      // kanban 默认保持历史 24h（需求 111 决策 3A：仅新增「全部」选项，不改变默认值）
      expect(screen.getByTestId('mock-loop-kanban')).toHaveAttribute('data-hours', '24');
      fireEvent.click(screen.getByText('全部'));
      expect(screen.getByTestId('mock-loop-kanban')).toHaveAttribute('data-hours', 'null');
    });
  });
});
