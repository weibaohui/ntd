// useProcessEditorState 单测：覆盖加载（成功/失败/YAML 解析失败）、双向联动
// （可视化→YAML 全链路、YAML→可视化 debounced、isSyncing 防循环早退）、markClean、选中节点。
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import type { ProcessDefinition } from '@/types/process';
import { useProcessEditorState } from './useProcessEditorState';

// 稳定的 message mock（vi.hoisted 避免工厂 hoisting 顺序问题）。
const mockMessage = vi.hoisted(() => ({
  error: vi.fn(),
  success: vi.fn(),
  warning: vi.fn(),
  info: vi.fn(),
}));
vi.mock('antd', async (importOriginal) => {
  const actual = await importOriginal<typeof import('antd')>();
  return { ...actual, message: mockMessage };
});

// bundledApi / 解析器全部桩化，隔离 hook 行为。
vi.mock('@/api/bundled', () => ({
  bundledApi: { getProcess: vi.fn() },
}));
vi.mock('@/components/process/processYamlValidator', () => ({
  parseYaml: vi.fn(),
  yamlDump: vi.fn(),
}));

// 动态取 mock 引用（vi.mock 提升后模块级 const 取不到）。
async function mocks() {
  const { bundledApi } = await import('@/api/bundled');
  const sv = await import('@/components/process/processYamlValidator');
  return { bundledApi, parseYaml: sv.parseYaml, yamlDump: sv.yamlDump };
}

const GUID = 'proc-1';
const DEF = { phases: [{ id: 'p1' }] } as unknown as ProcessDefinition;

describe('useProcessEditorState — 加载族', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useRealTimers();
  });

  it('test_loadDetail_成功解析definition并填yamlText与isSystem', async () => {
    const { bundledApi, parseYaml, yamlDump } = await mocks();
    vi.mocked(bundledApi.getProcess).mockResolvedValue({
      definition: 'name: x',
      is_system: true,
    } as never);
    vi.mocked(parseYaml).mockReturnValue({ parsed: DEF } as never);
    vi.mocked(yamlDump).mockReturnValue('name: x');

    const { result } = renderHook(() => useProcessEditorState(GUID));

    await waitFor(() => expect(result.current.detail).not.toBeNull());
    expect(result.current.yamlText).toBe('name: x');
    expect(result.current.isSystem).toBe(true);
    expect(result.current.definition).toBe(DEF);
    expect(result.current.loading).toBe(false);
    expect(mockMessage.error).not.toHaveBeenCalled();
  });

  it('test_loadDetail_请求失败message_error且loading清空detail为null', async () => {
    const { bundledApi } = await mocks();
    vi.mocked(bundledApi.getProcess).mockRejectedValue(new Error('boom'));

    const { result } = renderHook(() => useProcessEditorState(GUID));

    await waitFor(() => expect(mockMessage.error).toHaveBeenCalledWith(`加载工艺「${GUID}」失败`));
    expect(result.current.detail).toBeNull();
    expect(result.current.loading).toBe(false);
  });

  it('test_loadDetail_YAML解析失败message_error且definition保持null', async () => {
    const { bundledApi, parseYaml } = await mocks();
    vi.mocked(bundledApi.getProcess).mockResolvedValue({
      definition: 'bad',
      is_system: false,
    } as never);
    vi.mocked(parseYaml).mockReturnValue({ parsed: null, error: new Error('解析炸了') } as never);

    const { result } = renderHook(() => useProcessEditorState(GUID));

    await waitFor(() => expect(mockMessage.error).toHaveBeenCalledWith('YAML 解析失败：解析炸了'));
    expect(result.current.definition).toBeNull();
  });
});

describe('useProcessEditorState — 双向联动族', () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    // 加载失败（快失败），避免 mount effect 干扰联动断言。
    const { bundledApi } = await mocks();
    vi.mocked(bundledApi.getProcess).mockRejectedValue(new Error('skip-load'));
  });
  afterEach(() => vi.useRealTimers());

  it('test_handleDefinitionChange_更新definition刷新yamlText并标dirty', async () => {
    const { yamlDump } = await mocks();
    vi.mocked(yamlDump).mockReturnValue('dumped');
    const { result } = renderHook(() => useProcessEditorState(GUID));

    await act(async () => {
      result.current.handleDefinitionChange(DEF);
      // 推完清 isSyncing 的 setTimeout(0)：推进到轮次结束。
      await vi.runAllTimersAsync();
    });

    expect(result.current.definition).toBe(DEF);
    expect(result.current.yamlText).toBe('dumped');
    expect(result.current.isDirty).toBe(true);
  });

  it('test_handleYamlChange_isSyncing期间忽略不触发debounced解析', async () => {
    const { parseYaml, yamlDump } = await mocks();
    vi.mocked(yamlDump).mockReturnValue('dumped');
    vi.mocked(parseYaml).mockReturnValue({ parsed: DEF } as never);
    const { result } = renderHook(() => useProcessEditorState(GUID));

    // 先触发可视化→YAML：isSyncing 置 true 且（fake timer 下）setTimeout(0) 未执行 → 仍 true。
    await act(async () => {
      result.current.handleDefinitionChange(DEF);
    });
    // isSyncing 为 true 期间发 YAML 变更：应早退，不排 debounced parseYaml。
    await act(async () => {
      result.current.handleYamlChange('edited');
    });
    expect(parseYaml).not.toHaveBeenCalled();
    // 推完残留 timer，避免泄漏到后续用例。
    await act(async () => {
      await vi.runAllTimersAsync();
    });
  });

  it('test_handleYamlChange_非syncing时debounced300ms后解析回写definition', async () => {
    const { parseYaml } = await mocks();
    const DEF2 = { phases: [{ id: 'p2' }] } as unknown as ProcessDefinition;
    vi.mocked(parseYaml).mockReturnValue({ parsed: DEF2 } as never);
    const { result } = renderHook(() => useProcessEditorState(GUID));

    await act(async () => {
      result.current.handleYamlChange('edited');
    });
    // 未到 300ms：尚未解析。
    expect(parseYaml).not.toHaveBeenCalled();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });
    expect(parseYaml).toHaveBeenCalledWith('edited');
    expect(result.current.definition).toBe(DEF2);
    expect(result.current.isDirty).toBe(true);
  });

  it('test_markClean_清isDirty标记', async () => {
    const { yamlDump } = await mocks();
    vi.mocked(yamlDump).mockReturnValue('d');
    const { result } = renderHook(() => useProcessEditorState(GUID));
    await act(async () => {
      result.current.handleDefinitionChange(DEF);
      await vi.runAllTimersAsync();
    });
    expect(result.current.isDirty).toBe(true);
    act(() => result.current.markClean());
    expect(result.current.isDirty).toBe(false);
  });

  it('test_setSelectedNodeId_回写选中节点', async () => {
    const { result } = renderHook(() => useProcessEditorState(GUID));
    // setSelectedNodeId 前，先排空 mount 的 loadDetail（rejected）微任务，
    // 否则其游离的 setLoading(false) 落在 act 之外触发警告。
    await act(async () => {
      result.current.setSelectedNodeId('node-9');
      await vi.runAllTimersAsync();
    });
    expect(result.current.selectedNodeId).toBe('node-9');
  });
});
