import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { InstallExecutorButton } from './InstallExecutorButton';
import { ExecutionProvider } from '@/hooks/useExecutionContext';

/**
 * 模拟 ActionButton 以避免 antd Drawer 渲染副作用。
 * 通过断言 ActionButton 收到的 onApply props 间接验证 onInstalled 回调。
 */
// 模块级变量：被 mock ActionButton 赋值为 onApply 回调，测试中读取并触发
let capturedOnApply: ((result: string) => void) | null = null;
vi.mock('@/components/ActionButton', () => ({
  ActionButton: (props: any) => {
    // 捕获 onApply 回调供测试触发；赋值给外部变量保持引用
    capturedOnApply = props.onApply || null;
    return (
      <div>
        <button onClick={() => props.onApply?.('installed')} data-testid="mock-action-apply">
          安装
        </button>
        <div data-testid="mock-action-title">{props.panelTitle}</div>
        <div data-testid="mock-action-prompt">{props.prompt}</div>
      </div>
    );
  },
}));
// 引用 capturedOnApply 消除 TS6133"声明未读取"警告；其值由 mock 写入、测试读取
void capturedOnApply;

vi.mock('@/hooks/useIsMobile', () => ({
  useIsMobile: () => false,
}));

/**
 * 渲染辅助函数：包裹 ExecutionProvider，模拟应用启动时的上下文。
 */
function renderWithProvider(ui: React.ReactElement) {
  return render(<ExecutionProvider>{ui}</ExecutionProvider>);
}

describe('InstallExecutorButton', () => {
  // 每次测试前重置 capturedOnApply，避免跨测试污染
  beforeEach(() => {
    capturedOnApply = null;
  });

  it('renders install button with label', () => {
    renderWithProvider(
      <InstallExecutorButton
        executorName="claudecode"
        displayName="Claude Code"
        prompt="install prompt"
      />,
    );
    expect(screen.getByRole('button', { name: /安装/i })).toBeInTheDocument();
  });

  it('displays executor name in title and passes prompt', () => {
    renderWithProvider(
      <InstallExecutorButton
        executorName="claudecode"
        displayName="Claude Code"
        prompt="Please install Claude Code on this machine."
      />,
    );
    // mock 的 ActionButton 会渲染 panelTitle 与 prompt
    expect(screen.getByTestId('mock-action-title')).toHaveTextContent('安装 Claude Code');
    expect(screen.getByTestId('mock-action-prompt')).toHaveTextContent('Please install Claude Code on this machine.');
  });

  it('calls onInstalled when apply is triggered', () => {
    const onInstalled = vi.fn();
    renderWithProvider(
      <InstallExecutorButton
        executorName="claudecode"
        displayName="Claude Code"
        prompt="install prompt"
        onInstalled={onInstalled}
      />,
    );
    // 触发 mock ActionButton 的 onApply（即 capturedOnApply），验证 onInstalled 被调用
    fireEvent.click(screen.getByTestId('mock-action-apply'));
    expect(onInstalled).toHaveBeenCalledTimes(1);
  });

  it('does not crash when onInstalled is not provided', () => {
    renderWithProvider(
      <InstallExecutorButton
        executorName="claudecode"
        displayName="Claude Code"
        prompt="install prompt"
        // 不传 onInstalled
      />,
    );
    // onApply 为空时不应抛出异常
    expect(() => fireEvent.click(screen.getByTestId('mock-action-apply'))).not.toThrow();
  });
});
