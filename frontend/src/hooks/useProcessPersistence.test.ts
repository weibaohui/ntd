// useProcessPersistence 单测：保存（成功回刷+清dirty / 失败提示）、删除（确认框 title +
// onOk 删除跳列表）、返回、复制系统工艺（成功跳副本 / 失败提示）。
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import type { ProcessTemplateDetail } from '@/api/bundled';
import { useProcessPersistence } from './useProcessPersistence';

// 稳定的 message / Modal.confirm mock。
const mockMessage = vi.hoisted(() => ({
  error: vi.fn(),
  success: vi.fn(),
  warning: vi.fn(),
  info: vi.fn(),
}));
const mockConfirm = vi.hoisted(() => vi.fn());
vi.mock('antd', async (importOriginal) => {
  const actual = await importOriginal<typeof import('antd')>();
  return {
    ...actual,
    message: mockMessage,
    Modal: { ...actual.Modal, confirm: mockConfirm },
  };
});

vi.mock('@/api/bundled', () => ({
  bundledApi: { putProcess: vi.fn(), deleteProcess: vi.fn(), copyProcessToUser: vi.fn() },
}));

async function bundled() {
  return (await import('@/api/bundled')).bundledApi;
}

const GUID = 'proc-1';
// 注入桩化的编辑器状态回写出口。
function makeDeps(over: { yamlText?: string; detail?: ProcessTemplateDetail | null } = {}) {
  return {
    detail: (over.detail ?? { name: 'n', display_name: '显示名' }) as ProcessTemplateDetail,
    yamlText: over.yamlText ?? 'yaml-current',
    setYamlText: vi.fn(),
    markClean: vi.fn(),
  };
}

// location.hash 在 jsdom 下「赋值可写、读回可得」，但 setter 属性不可 spy（non-configurable），
// 故采用「前置清空 + 调用后读回」的姿势捕获跳转目标，避免 redefine 报错。
function resetHash() {
  window.location.hash = '';
}

describe('useProcessPersistence', () => {
  beforeEach(() => vi.clearAllMocks());

  it('test_handleSave_成功PUT当前yamlText回刷Monaco并清dirty', async () => {
    const api = await bundled();
    vi.mocked(api.putProcess).mockResolvedValue({ definition: 'yaml-bumped' } as never);
    const deps = makeDeps({ yamlText: 'yaml-current' });
    const { result } = renderHook(() => useProcessPersistence(GUID, deps));

    await act(async () => {
      await result.current.handleSave();
    });

    expect(api.putProcess).toHaveBeenCalledWith(GUID, 'yaml-current');
    expect(deps.setYamlText).toHaveBeenCalledWith('yaml-bumped');
    expect(deps.markClean).toHaveBeenCalled();
    expect(mockMessage.success).toHaveBeenCalledWith('工艺已保存');
    expect(result.current.isSaving).toBe(false);
  });

  it('test_handleSave_失败message_error且不清dirty', async () => {
    const api = await bundled();
    vi.mocked(api.putProcess).mockRejectedValue(new Error('boom'));
    const deps = makeDeps();
    const { result } = renderHook(() => useProcessPersistence(GUID, deps));

    await act(async () => {
      await result.current.handleSave();
    });

    expect(mockMessage.error).toHaveBeenCalledWith('保存失败：boom');
    expect(deps.markClean).not.toHaveBeenCalled();
    expect(result.current.isSaving).toBe(false);
  });

  it('test_handleDelete_确认框title优先显示名_onOk删除清dirty跳列表', async () => {
    const api = await bundled();
    vi.mocked(api.deleteProcess).mockResolvedValue(undefined as never);
    resetHash();
    const deps = makeDeps({ detail: { name: 'n', display_name: '我的工艺' } as ProcessTemplateDetail });
    const { result } = renderHook(() => useProcessPersistence(GUID, deps));

    act(() => result.current.handleDelete());
    expect(mockConfirm).toHaveBeenCalledTimes(1);
    // title 优先取 display_name（而非回退 GUID）。
    const opts = mockConfirm.mock.calls[0][0] as { title: string; onOk: () => Promise<void> };
    expect(opts.title).toContain('我的工艺');

    await act(async () => {
      await opts.onOk();
    });
    expect(api.deleteProcess).toHaveBeenCalledWith(GUID);
    expect(deps.markClean).toHaveBeenCalled();
    expect(mockMessage.success).toHaveBeenCalledWith('工艺已删除');
    expect(window.location.hash).toBe('#/processes');
  });

  it('test_handleBack_仅置hash回列表', () => {
    resetHash();
    const { result } = renderHook(() => useProcessPersistence(GUID, makeDeps()));
    act(() => result.current.handleBack());
    expect(window.location.hash).toBe('#/processes');
  });

  it('test_handleCopyToUser_成功跳副本编辑器', async () => {
    const api = await bundled();
    vi.mocked(api.copyProcessToUser).mockResolvedValue({ guid: 'copy-9' } as never);
    resetHash();
    const { result } = renderHook(() => useProcessPersistence(GUID, makeDeps()));

    await act(async () => {
      await result.current.handleCopyToUser();
    });
    expect(mockMessage.success).toHaveBeenCalledWith('已复制为我的工艺，正在打开副本…');
    expect(window.location.hash).toContain('guid=copy-9');
    expect(window.location.hash).toContain('processMode=edit');
  });

  it('test_handleCopyToUser_失败message_error', async () => {
    const api = await bundled();
    vi.mocked(api.copyProcessToUser).mockRejectedValue(new Error('x'));
    const { result } = renderHook(() => useProcessPersistence(GUID, makeDeps()));

    await act(async () => {
      await result.current.handleCopyToUser();
    });
    expect(mockMessage.error).toHaveBeenCalledWith('复制到用户层失败');
  });
});
