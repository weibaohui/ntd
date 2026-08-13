import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import * as db from '@/utils/database';
import { useExecutorFieldSaver } from './useExecutorFieldSaver';
import type { ExecutorConfig } from '@/types';

// 稳定的 message mock（vi.hoisted 避免工厂 hoisting 顺序问题；引用稳定防 useCallback 依赖每渲染变新）。
const mockMessage = vi.hoisted(() => ({
  error: vi.fn(),
  success: vi.fn(),
  warning: vi.fn(),
  info: vi.fn(),
}));
vi.mock('antd', async (importOriginal) => {
  const actual = await importOriginal<typeof import('antd')>();
  return {
    ...actual,
    App: { ...actual.App, useApp: () => ({ message: mockMessage }) },
  };
});

vi.mock('@/utils/database', () => ({ updateExecutor: vi.fn() }));

function makeExecutor(over: Partial<ExecutorConfig> = {}): ExecutorConfig {
  return {
    id: 1,
    name: 'claude',
    path: '/x',
    enabled: true,
    display_name: 'Claude',
    session_dir: '',
    is_default: false,
    created_at: null,
    updated_at: null,
    ...over,
  };
}

describe('useExecutorFieldSaver', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('saveExecutorField：成功 → 回写列表 + 返回更新值 + 清 saving', async () => {
    const replaceExecutor = vi.fn();
    const updated = makeExecutor({ path: '/new' });
    vi.mocked(db.updateExecutor).mockResolvedValue(updated);
    const { result } = renderHook(() => useExecutorFieldSaver(replaceExecutor));

    let ret: ExecutorConfig | null = null;
    await act(async () => {
      ret = await result.current.saveExecutorField('claude', { path: '/new' });
    });

    expect(db.updateExecutor).toHaveBeenCalledWith('claude', { path: '/new' });
    // 回写列表：把后端返回的完整配置交给 replaceExecutor。
    expect(replaceExecutor).toHaveBeenCalledWith('claude', updated);
    expect(ret).toEqual(updated);
    // saving 已清，不再持有该执行器名。
    expect(result.current.savingExecutor).toBeNull();
    expect(mockMessage.error).not.toHaveBeenCalled();
  });

  it('saveExecutorField：失败 → message.error + 返回 null + 仍清 saving', async () => {
    const replaceExecutor = vi.fn();
    vi.mocked(db.updateExecutor).mockRejectedValue(new Error('boom'));
    const { result } = renderHook(() => useExecutorFieldSaver(replaceExecutor));

    let ret: ExecutorConfig | null = 'sentinel' as unknown as ExecutorConfig | null;
    await act(async () => {
      ret = await result.current.saveExecutorField('claude', { enabled: false });
    });

    expect(ret).toBeNull();
    // 失败不回写列表（保留旧缓存）。
    expect(replaceExecutor).not.toHaveBeenCalled();
    expect(mockMessage.error).toHaveBeenCalledWith('保存失败: boom');
    // 即使失败也清 saving，避免按钮卡 loading。
    expect(result.current.savingExecutor).toBeNull();
  });

  it('inlineFieldSave.onBlur：值未改（去空格后等于当前值）→ 不调 onSave', async () => {
    const replaceExecutor = vi.fn();
    const onSave = vi.fn().mockResolvedValue(undefined);
    const { result } = renderHook(() => useExecutorFieldSaver(replaceExecutor));

    const handlers = result.current.inlineFieldSave('claude', '/x', onSave);
    // 模拟 Input 失焦事件：target.value 带空格但 trim 后等于 '/x'。
    const fakeEvent = { target: { value: '  /x  ' } } as unknown as React.FocusEvent<HTMLInputElement>;
    await act(async () => {
      await handlers.onBlur(fakeEvent);
    });
    expect(onSave).not.toHaveBeenCalled();
  });

  it('inlineFieldSave.onBlur：值改动 → 以去空格后的新值调 onSave', async () => {
    const replaceExecutor = vi.fn();
    const onSave = vi.fn().mockResolvedValue(undefined);
    const { result } = renderHook(() => useExecutorFieldSaver(replaceExecutor));

    const handlers = result.current.inlineFieldSave('claude', '/old', onSave);
    const fakeEvent = { target: { value: ' /new ' } } as unknown as React.FocusEvent<HTMLInputElement>;
    await act(async () => {
      await handlers.onBlur(fakeEvent);
    });
    expect(onSave).toHaveBeenCalledWith('/new');
  });

  it('inlineFieldSave.onPressEnter：触发 target.blur()', () => {
    const replaceExecutor = vi.fn();
    const blur = vi.fn();
    const { result } = renderHook(() => useExecutorFieldSaver(replaceExecutor));

    const handlers = result.current.inlineFieldSave('claude', '/x', vi.fn());
    const fakeEvent = { target: { blur } } as unknown as React.KeyboardEvent<HTMLInputElement>;
    act(() => {
      handlers.onPressEnter(fakeEvent);
    });
    expect(blur).toHaveBeenCalled();
  });

  it('inlineFieldSave.saving：保存进行中为 true，空闲为 false', async () => {
    const replaceExecutor = vi.fn();
    // 让 updateExecutor 挂起，便于在 in-flight 时断言 saving。
    let resolveUpdate: (v: ExecutorConfig) => void = () => undefined;
    vi.mocked(db.updateExecutor).mockImplementation(
      () => new Promise((resolve) => {
        resolveUpdate = resolve;
      }),
    );
    const { result } = renderHook(() => useExecutorFieldSaver(replaceExecutor));

    // 保存未发起：saving 为 false。
    expect(result.current.inlineFieldSave('claude', '/x', vi.fn()).saving).toBe(false);

    let done = false;
    act(() => {
      void result.current.saveExecutorField('claude', { path: '/y' }).then(() => {
        done = true;
      });
    });
    // in-flight：saving 为 true。
    await waitFor(() => expect(result.current.savingExecutor).toBe('claude'));
    expect(result.current.inlineFieldSave('claude', '/x', vi.fn()).saving).toBe(true);

    // 放行挂起的请求，等保存结束。
    await act(async () => {
      resolveUpdate(makeExecutor({ path: '/y' }));
      await waitFor(() => expect(done).toBe(true));
    });
    expect(result.current.inlineFieldSave('claude', '/x', vi.fn()).saving).toBe(false);
  });
});
