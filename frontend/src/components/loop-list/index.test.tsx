import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { LoopListPage } from './index';
import type { LoopListItem } from '@/types/loop';

vi.mock('@/hooks/useApp', () => ({
  useApp: () => ({
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
}));

vi.mock('@/utils/database/todos', () => ({
  getProjectDirectories: vi.fn().mockResolvedValue([{ id: 1, name: 'test', path: '/test' }]),
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
      <button onClick={() => props.onSelectLoop(1)} data-testid="mock-row-click">Row</button>
    </div>
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
});