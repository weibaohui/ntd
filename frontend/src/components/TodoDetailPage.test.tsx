import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { TodoDetailPage } from './TodoDetailPage';

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

vi.mock('@/components/TodoDetail', () => ({
  TodoDetail: (props: {
    hideTitleRow?: boolean;
    onOpenPost?: (todoId: number, recordId: number) => void;
  }) => (
    <div data-testid="mock-todo-detail">
      <div data-testid="mock-hide-title-row">{props.hideTitleRow ? 'true' : 'false'}</div>
      <button onClick={() => props.onOpenPost?.(1, 100)} data-testid="mock-open-post">Open Post</button>
    </div>
  ),
}));

describe('TodoDetailPage', () => {
  const mockOnBack = vi.fn();
  const mockOnOpenPost = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders page card with correct title', () => {
    render(
      <TodoDetailPage
        todoId={123}
        onBack={mockOnBack}
        onOpenPost={mockOnOpenPost}
      />,
    );

    expect(screen.getByTestId('mock-page-card')).toBeInTheDocument();
    expect(screen.getByTestId('mock-page-card-title')).toHaveTextContent('事项 #123');
  });

  it('renders back button and calls onBack when clicked', () => {
    render(
      <TodoDetailPage
        todoId={123}
        onBack={mockOnBack}
        onOpenPost={mockOnOpenPost}
      />,
    );

    const backBtn = screen.getByRole('button', { name: /返回列表/i });
    expect(backBtn).toBeInTheDocument();
    fireEvent.click(backBtn);
    expect(mockOnBack).toHaveBeenCalledTimes(1);
  });

  it('passes hideTitleRow to TodoDetail', () => {
    render(
      <TodoDetailPage
        todoId={123}
        onBack={mockOnBack}
        onOpenPost={mockOnOpenPost}
      />,
    );

    expect(screen.getByTestId('mock-todo-detail')).toBeInTheDocument();
    expect(screen.getByTestId('mock-hide-title-row')).toHaveTextContent('true');
  });

  it('passes onOpenPost to TodoDetail', () => {
    render(
      <TodoDetailPage
        todoId={123}
        onBack={mockOnBack}
        onOpenPost={mockOnOpenPost}
      />,
    );

    fireEvent.click(screen.getByTestId('mock-open-post'));
    expect(mockOnOpenPost).toHaveBeenCalledWith(1, 100);
  });
});