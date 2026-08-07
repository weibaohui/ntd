import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { LoopDetailPage } from './LoopDetailPage';

vi.mock('@/components/common/PageCard', () => ({
  // 062：页面改为传 onBack/backLabel，mock 模拟 PageCard 在 extra 最右端渲染返回按钮的行为
  PageCard: (props: {
    icon?: React.ReactNode;
    title?: string;
    onBack?: () => void;
    backLabel?: string;
    children?: React.ReactNode;
    style?: React.CSSProperties;
    contentStyle?: React.CSSProperties;
  }) => (
    <div data-testid="mock-page-card">
      <div data-testid="mock-page-card-title">{props.title}</div>
      {props.onBack && (
        <button data-testid="mock-page-card-back" onClick={props.onBack}>
          {props.backLabel ?? '返回列表'}
        </button>
      )}
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
    onTitleReady?: (name: string) => void;
  }) => (
    <div data-testid="mock-loop-detail-panel">
      <div data-testid="mock-hide-title-row">{props.hideTitleRow ? 'true' : 'false'}</div>
      <button onClick={props.onDelete} data-testid="mock-delete">Delete</button>
      <button onClick={props.onToggleStatus} data-testid="mock-toggle-status">Toggle Status</button>
      {/* 062：模拟 detail 加载完成后上报环路名称 */}
      <button onClick={() => props.onTitleReady?.('测试环路')} data-testid="mock-title-ready">Title Ready</button>
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

  it('updates title with loop name when onTitleReady fires', () => {
    // 062：标题应从「环路 #id」升级为「环路 #id: 名称」
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

    fireEvent.click(screen.getByTestId('mock-title-ready'));
    expect(screen.getByTestId('mock-page-card-title')).toHaveTextContent('环路 #456: 测试环路');
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