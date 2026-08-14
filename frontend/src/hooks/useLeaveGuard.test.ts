// useLeaveGuard 单测：isDirty 时 hashchange 弹确认、非 dirty 放行、确认离开清标记+跳转、
// 取消离开 replaceState 回退、beforeunload 阻止默认、卸载移除监听。
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useLeaveGuard } from './useLeaveGuard';

// Modal.confirm 桩：捕获调用以断言 title 与 onOk/onCancel 行为。
const mockConfirm = vi.hoisted(() => vi.fn());
vi.mock('antd', async (importOriginal) => {
  const actual = await importOriginal<typeof import('antd')>();
  return { ...actual, Modal: { ...actual.Modal, confirm: mockConfirm } };
});

// 构造带 oldURL/newURL 的 hashchange 事件（兼容 jsdom）。
function hashEvent(oldURL: string, newURL: string): HashChangeEvent {
  const evt = new Event('hashchange') as HashChangeEvent;
  Object.defineProperty(evt, 'oldURL', { value: oldURL, configurable: true });
  Object.defineProperty(evt, 'newURL', { value: newURL, configurable: true });
  return evt;
}

describe('useLeaveGuard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('test_isDirty时hashchange弹Modal_confirm', () => {
    renderHook(() => useLeaveGuard(true, vi.fn()));
    window.dispatchEvent(hashEvent('http://localhost/#/editor', 'http://localhost/#/processes'));
    expect(mockConfirm).toHaveBeenCalledTimes(1);
  });

  it('test_非dirty时hashchange不弹框放行', () => {
    renderHook(() => useLeaveGuard(false, vi.fn()));
    window.dispatchEvent(hashEvent('http://localhost/#/editor', 'http://localhost/#/processes'));
    expect(mockConfirm).not.toHaveBeenCalled();
  });

  it('test_确认离开调markClean并跳目标hash', () => {
    const markClean = vi.fn();
    window.location.hash = '';
    renderHook(() => useLeaveGuard(true, markClean));
    window.dispatchEvent(hashEvent('http://localhost/#/editor', 'http://localhost/#/processes'));
    const opts = mockConfirm.mock.calls[0][0] as { onOk: () => void };
    opts.onOk();
    expect(markClean).toHaveBeenCalled();
    // 跳目标 hash（取 newURL 的 # 段）：onOk 内 markClean 在前，故二次 hashchange 不再弹框。
    expect(window.location.hash).toBe('#/processes');
  });

  it('test_取消离开replaceState回退旧URL', () => {
    // jsdom 对跨完整 URL 的 replaceState 抛 SecurityError，桩掉实现只验入参。
    const replaceSpy = vi.spyOn(history, 'replaceState').mockImplementation(() => null);
    renderHook(() => useLeaveGuard(true, vi.fn()));
    window.dispatchEvent(hashEvent('http://localhost/#/editor', 'http://localhost/#/processes'));
    const opts = mockConfirm.mock.calls[0][0] as { onCancel: () => void };
    opts.onCancel();
    // 用 history.replaceState 回退旧 hash，避免再触发 hashchange。
    expect(replaceSpy).toHaveBeenCalledWith(null, '', 'http://localhost/#/editor');
    replaceSpy.mockRestore();
  });

  it('test_isDirty时beforeunload阻止默认触发原生提示', () => {
    renderHook(() => useLeaveGuard(true, vi.fn()));
    const evt = new Event('beforeunload');
    const pdSpy = vi.spyOn(evt, 'preventDefault');
    window.dispatchEvent(evt);
    // preventDefault 是标准阻止机制；returnValue='' 是 Firefox 兼容写法，
    // jsdom 对其归一化（读回恒 true），故只断言 preventDefault。
    expect(pdSpy).toHaveBeenCalled();
  });

  it('test_非dirty时beforeunload不阻止', () => {
    renderHook(() => useLeaveGuard(false, vi.fn()));
    const evt = new Event('beforeunload');
    const pdSpy = vi.spyOn(evt, 'preventDefault');
    window.dispatchEvent(evt);
    expect(pdSpy).not.toHaveBeenCalled();
  });

  it('test_卸载后移除监听_hashchange不再弹框', () => {
    const { unmount } = renderHook(() => useLeaveGuard(true, vi.fn()));
    unmount();
    window.dispatchEvent(hashEvent('http://localhost/#/editor', 'http://localhost/#/processes'));
    expect(mockConfirm).not.toHaveBeenCalled();
  });
});
