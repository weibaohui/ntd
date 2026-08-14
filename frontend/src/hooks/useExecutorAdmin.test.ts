import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import * as db from '@/utils/database';
import { useExecutorAdmin } from './useExecutorAdmin';
import type { ExecutorConfig } from '@/types';

// 屏蔽 antd App.useApp 的 message 依赖（hook 内错误/成功提示路径）。
// 关键：message 必须是稳定引用——vi.hoisted 让它在 vi.mock 工厂（被提升到文件顶）之前就定义好，
// 且每次 useApp() 返回同一 message 对象。否则 message 每次渲染变新引用 → loadExecutors 的
// useCallback([message]) 每次渲染重建 → mount useEffect 无限重触发 → OOM。（见 useTaskDetail.test 范式）
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

// mock db：各 handler 的数据源。vi.mock 工厂被提升到文件顶部，故内部只能用字面 vi.fn()，
// 不能引用外部变量；用例通过 vi.mocked(db.X) 读写具体实现。
vi.mock('@/utils/database', () => ({
  getExecutors: vi.fn(),
  updateExecutor: vi.fn(),
  detectExecutor: vi.fn(),
  setDefaultExecutor: vi.fn(),
  getExecutorModels: vi.fn(),
}));

// setDefaultExecutorCache 是 setAsDefault 的副作用（更新前端缓存），mock 成空实现即可。
vi.mock('@/utils/executors', () => ({ setDefaultExecutorCache: vi.fn() }));

/** 默认 executor 配置工厂：减少每个用例的样板。 */
function makeExecutor(over: Partial<ExecutorConfig> = {}): ExecutorConfig {
  return {
    id: 1,
    name: 'claude',
    path: '/usr/bin/claude',
    enabled: true,
    display_name: 'Claude',
    session_dir: '',
    is_default: false,
    created_at: null,
    updated_at: null,
    ...over,
  };
}

describe('useExecutorAdmin', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // 默认 getExecutors 返回空（首屏 useEffect 会调一次）。
    vi.mocked(db.getExecutors).mockResolvedValue([]);
  });

  it('test_loadExecutors_加载列表并切换loading', async () => {
    const list = [makeExecutor({ name: 'a' }), makeExecutor({ name: 'b', id: 2 })];
    vi.mocked(db.getExecutors).mockResolvedValue(list);
    const { result } = renderHook(() => useExecutorAdmin());

    // 首屏 useEffect 已触发一次 loadExecutors。
    await waitFor(() => expect(result.current.executors).toHaveLength(2));
    expect(result.current.executorsLoading).toBe(false);
  });

  it('test_replaceExecutor_按name替换命中项未命中name不变', async () => {
    vi.mocked(db.getExecutors).mockResolvedValue([makeExecutor({ name: 'a', path: 'old' })]);
    const { result } = renderHook(() => useExecutorAdmin());
    await waitFor(() => expect(result.current.executors).toHaveLength(1));

    act(() => {
      result.current.replaceExecutor('a', makeExecutor({ name: 'a', path: 'new' }));
    });
    expect(result.current.executors[0].path).toBe('new');

    // 未命中的 name 不应新增条目。
    act(() => {
      result.current.replaceExecutor('zzz', makeExecutor({ name: 'zzz' }));
    });
    expect(result.current.executors).toHaveLength(1);
  });

  it('test_batchDetect_按检测结果翻转enabled并仅计可用项', async () => {
    // a：找到但禁用 → 应启用；b：未找到但启用 → 应禁用；c：找到且已启用 → 不改。
    vi.mocked(db.getExecutors).mockResolvedValue([
      makeExecutor({ name: 'a', id: 1, enabled: false }),
      makeExecutor({ name: 'b', id: 2, enabled: true }),
      makeExecutor({ name: 'c', id: 3, enabled: true }),
    ]);
    vi.mocked(db.detectExecutor).mockImplementation(async (name: string) =>
      name === 'b'
        ? { binary_found: false, path_resolved: null }
        : { binary_found: true, path_resolved: `/p/${name}` },
    );
    // 回传带新 enabled 的完整对象，模拟后端返回更新后的配置。
    vi.mocked(db.updateExecutor).mockImplementation(
      async (name: string, data: { enabled?: boolean }) =>
        makeExecutor({ name, enabled: data.enabled ?? true }),
    );

    const { result } = renderHook(() => useExecutorAdmin());
    await waitFor(() => expect(result.current.executors).toHaveLength(3));

    await act(async () => {
      await result.current.batchDetect();
    });

    // updateExecutor 仅对 a（false→true）与 b（true→false）调用，c 不调用。
    const calls = vi.mocked(db.updateExecutor).mock.calls;
    expect(calls.map((c) => c[0])).toEqual(['a', 'b']);
    expect(calls[0][1]).toEqual({ enabled: true });
    expect(calls[1][1]).toEqual({ enabled: false });
    // 可用计数 a、c = 2；detectResults 三行都落（b 为 found:false）。
    expect(result.current.batchDetecting).toBe(false);
    expect(result.current.detectResults.a.found).toBe(true);
    expect(result.current.detectResults.b.found).toBe(false);
    expect(result.current.detectResults.c.found).toBe(true);
  });

  it('test_detectExecutorByName_落检测结果found与resolved', async () => {
    vi.mocked(db.getExecutors).mockResolvedValue([makeExecutor({ name: 'a' })]);
    vi.mocked(db.detectExecutor).mockResolvedValue({ binary_found: true, path_resolved: '/p/a' });
    const { result } = renderHook(() => useExecutorAdmin());
    await waitFor(() => expect(result.current.executors).toHaveLength(1));

    await act(async () => {
      await result.current.detectExecutorByName(makeExecutor({ name: 'a', display_name: 'A' }));
    });
    expect(result.current.detectResults.a).toEqual({ found: true, resolved: '/p/a' });
    expect(result.current.detectingExecutor).toBeNull();
  });

  it('test_clearDetectResult_仅清除指定name的检测结果', async () => {
    vi.mocked(db.detectExecutor).mockResolvedValue({ binary_found: true, path_resolved: '/x' });
    const { result } = renderHook(() => useExecutorAdmin());

    // 落两条检测结果，再清一条，验证另一条仍在。
    await act(async () => {
      await result.current.detectExecutorByName(makeExecutor({ name: 'a' }));
      await result.current.detectExecutorByName(makeExecutor({ name: 'b' }));
    });
    expect(Object.keys(result.current.detectResults)).toHaveLength(2);

    act(() => {
      result.current.clearDetectResult('a');
    });
    expect(result.current.detectResults.a).toBeUndefined();
    expect(result.current.detectResults.b).toBeDefined();
  });

  it('test_setAsDefault_新默认置true其余置false全表重算', async () => {
    // 初始默认是 a，要把 b 设为默认：a→false、b→true。
    vi.mocked(db.getExecutors).mockResolvedValue([
      makeExecutor({ name: 'a', is_default: true }),
      makeExecutor({ name: 'b', is_default: false }),
    ]);
    // setDefaultExecutor 返回新默认执行器（后端只回新默认本身）。
    vi.mocked(db.setDefaultExecutor).mockResolvedValue(makeExecutor({ name: 'b' }));
    const { result } = renderHook(() => useExecutorAdmin());
    await waitFor(() => expect(result.current.executors).toHaveLength(2));

    await act(async () => {
      await result.current.setAsDefault(makeExecutor({ name: 'b', display_name: 'B' }));
    });

    const byName = Object.fromEntries(result.current.executors.map((e) => [e.name, e.is_default]));
    expect(byName).toEqual({ a: false, b: true });
    expect(result.current.settingDefaultExecutor).toBeNull();
  });

  it('test_setAsDefault_已是默认时直接return不调后端', async () => {
    vi.mocked(db.setDefaultExecutor).mockResolvedValue(makeExecutor({ name: 'a' }));
    const { result } = renderHook(() => useExecutorAdmin());
    await act(async () => {
      await result.current.setAsDefault(makeExecutor({ name: 'a', is_default: true }));
    });
    expect(db.setDefaultExecutor).not.toHaveBeenCalled();
  });

  it('test_handleModelsDropdown_展开拉取并缓存收起与重复展开不重请求', async () => {
    vi.mocked(db.getExecutorModels).mockResolvedValue(['openai/gpt-4', 'anthropic/claude']);
    const { result } = renderHook(() => useExecutorAdmin());

    // 收起不拉取。
    act(() => result.current.handleModelsDropdown('a', false));
    expect(db.getExecutorModels).not.toHaveBeenCalled();

    // 首次展开拉取一次。
    act(() => result.current.handleModelsDropdown('a', true));
    await waitFor(() => expect(result.current.executorModels.a).toHaveLength(2));
    expect(db.getExecutorModels).toHaveBeenCalledTimes(1);

    // 再次展开：已缓存（fetchedModelsRef 命中），不再请求。
    act(() => result.current.handleModelsDropdown('a', true));
    expect(db.getExecutorModels).toHaveBeenCalledTimes(1);
  });
});
