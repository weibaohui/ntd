import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { LoopListHeader } from './LoopListPageParts';

// LoopListHeader 渲染测试（111）：验证 list/kanban 两形态均渲染带「全部」的
// 时间分段，且 onChange 正确回传 null（全部）与小时数。
describe('LoopListHeader', () => {
  const mockOnViewChange = vi.fn();
  const mockOnSearchChange = vi.fn();
  const mockOnHoursChange = vi.fn();
  const mockOnReload = vi.fn();
  const mockOnOpenConfig = vi.fn();

  // 组装一组固定 props，避免每个用例重复书写 9 个参数
  function makeProps(viewMode: 'list' | 'kanban', hours: number | null) {
    return {
      viewMode,
      onViewChange: mockOnViewChange,
      searchKeyword: '',
      hours,
      onHoursChange: mockOnHoursChange,
      loading: false,
      workspaceId: 1,
      onSearchChange: mockOnSearchChange,
      onReload: mockOnReload,
      onOpenConfig: mockOnOpenConfig,
    };
  }

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('list 形态渲染「全部」时间分段', () => {
    render(<LoopListHeader {...makeProps('list', null)} />);
    expect(screen.getByText('全部')).toBeInTheDocument();
  });

  it('kanban 形态渲染「全部」时间分段', () => {
    render(<LoopListHeader {...makeProps('kanban', 24)} />);
    expect(screen.getByText('全部')).toBeInTheDocument();
  });

  it('点击 7d 回传 hours=168', () => {
    render(<LoopListHeader {...makeProps('list', null)} />);
    fireEvent.click(screen.getByText('7d'));
    expect(mockOnHoursChange).toHaveBeenCalledWith(168);
  });

  it('点击「全部」回传 null（不过滤）', () => {
    render(<LoopListHeader {...makeProps('kanban', 24)} />);
    fireEvent.click(screen.getByText('全部'));
    expect(mockOnHoursChange).toHaveBeenCalledWith(null);
  });

  it('list 形态显示配置与刷新按钮，kanban 形态隐藏', () => {
    const { rerender } = render(<LoopListHeader {...makeProps('list', null)} />);
    expect(screen.getByRole('button', { name: /配置/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /刷新/ })).toBeInTheDocument();
    rerender(<LoopListHeader {...makeProps('kanban', 24)} />);
    expect(screen.queryByRole('button', { name: /配置/ })).not.toBeInTheDocument();
  });
});
