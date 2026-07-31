import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { LoopDetailPage } from './LoopDetailPage';

vi.mock('@/components/common/PageCard', () => ({
  PageCard: (props: {
    icon?: React.ReactNode;
    title?: string;
    titleSuffix?: React.ReactNode;
    children?: React.ReactNode;
    style?: React.CSSProperties;
    contentStyle?: React.CSSProperties;
  }) => (
    <div data-testid="mock-page-card">
      <div data-testid="mock-page-card-title">{props.title}</div>
      <div data-testid="mock-page-card-title-suffix">{props.titleSuffix}</div>
      {props.children}
    </div>
  ),
}));

vi.mock('@/components/LoopStudioDetailPanel', () => ({
  LoopDetailPanel: (props: {
    loopId: number;
    workspaceId: number | null;
    tags: Array<{ id: number; name: string; color: string }>;
    hideTitleRow?: boolean;
    onDelete: () => void;
    onToggleStatus: () => void;
    onChanged: () => void;
    onOpenProcess?: (templateName: string) => void;
    onOpenTodo?: (todoId: number) => void;
  }) => (
    <div data-testid="mock-loop-detail-panel">
      <div data-testid="mock-hide-title-row">{props.hideTitleRow ? 'true' : 'false'}</div>
      <button onClick={props.onDelete} data-testid="mock-delete">Delete</button>
      <button onClick={props.onToggleStatus} data-testid="mock-toggle-status">Toggle Status</button>
    </div>
  ),
}));

vi.mock('@/components/LoopDetailPageParts', () => ({
  useLoopDetailActions: () => ({
    handleDelete: vi.fn(),
    handleToggleStatus: vi.fn(),
  }),
}));

describe('LoopDetailPage', () => {
  const mockOnBack = vi.fn();
  const mockOnOpenProcess = vi.fn();
  const mockOnSelectTodo = vi.fn();
  const mockOnLoopChanged = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders page card with correct title', () => {
    render(
      <LoopDetailPage
        loopId={456}
        workspaceId={1}
        tags={[]}
        onBack={mockOnBack}
        onOpenProcess={mockOnOpenProcess}
        onSelectTodo={mockOnSelectTodo}
        onLoopChanged={mockOnLoopChanged}
      />,
    );

    expect(screen.getByTestId('mock-page-card')).toBeInTheDocument();
    expect(screen.getByTestId('mock-page-card-title')).toHaveTextContent('环路 #456');
  });

  it('renders back button and calls onBack when clicked', () => {
    render(
      <LoopDetailPage
        loopId={456}
        workspaceId={1}
        tags={[]}
        onBack={mockOnBack}
        onOpenProcess={mockOnOpenProcess}
        onSelectTodo={mockOnSelectTodo}
        onLoopChanged={mockOnLoopChanged}
      />,
    );

    const backBtn = screen.getByRole('button', { name: /返回列表/i });
    expect(backBtn).toBeInTheDocument();
    fireEvent.click(backBtn);
    expect(mockOnBack).toHaveBeenCalledTimes(1);
  });

  it('passes hideTitleRow to LoopDetailPanel', () => {
    render(
      <LoopDetailPage
        loopId={456}
        workspaceId={1}
        tags={[]}
        onBack={mockOnBack}
        onOpenProcess={mockOnOpenProcess}
        onSelectTodo={mockOnSelectTodo}
        onLoopChanged={mockOnLoopChanged}
      />,
    );

    expect(screen.getByTestId('mock-loop-detail-panel')).toBeInTheDocument();
    expect(screen.getByTestId('mock-hide-title-row')).toHaveTextContent('true');
  });

  it('passes workspaceId as null when not provided', () => {
    render(
      <LoopDetailPage
        loopId={456}
        tags={[]}
        onBack={mockOnBack}
        onOpenProcess={mockOnOpenProcess}
        onSelectTodo={mockOnSelectTodo}
        onLoopChanged={mockOnLoopChanged}
      />,
    );

    expect(screen.getByTestId('mock-loop-detail-panel')).toBeInTheDocument();
  });
});