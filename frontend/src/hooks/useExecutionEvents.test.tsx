// 094：useExecutionEvents 单元测试（CodeRabbit #1011 评审项）。
//
// 核心守卫：workspace 切换时的 onclose 竞态——旧连接的 onclose 回调晚于新连接建立
// 触发时，不得冲掉 sharedWs 中的新连接引用、不得安排多余重连（重复连接会导致
// 事件重复 dispatch，issue #720 的回归形态）。
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useExecutionEvents } from './useExecutionEvents';

// ─── 假 WebSocket：记录每次连接的 URL，close() 异步触发 onclose（贴近真实浏览器行为）──
class FakeWebSocket {
  static instances: FakeWebSocket[] = [];
  readonly url: string;
  onopen: (() => void) | null = null;
  onmessage: ((ev: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  closed = false;
  constructor(url: string) {
    this.url = url;
    FakeWebSocket.instances.push(this);
  }
  close() {
    this.closed = true;
    // 真实浏览器里 onclose 在后续事件循环触发——这正是竞态的成因，测试必须复现该时序
    setTimeout(() => this.onclose?.(), 0);
  }
}

// useApp mock：可控的 selectedWorkspace + 空 dispatch（本测试不断言状态分发）
let mockWorkspace: number | null = null;
// 093 批次2：组件已拆为 useAppDispatch（dispatch-only）+ useTodos（workspace），
// mock 同步拆到两个模块；本测试不断言状态分发，dispatch 给空实现即可。
vi.mock('./useApp', () => ({
  useAppDispatch: () => vi.fn(),
}));
vi.mock('./useTodoContext', () => ({
  useTodos: () => ({
    state: { selectedWorkspace: mockWorkspace },
  }),
}));

/** 刷新宏任务队列：让 FakeWebSocket 的异步 onclose 有机会触发 */
async function flushTimers() {
  await new Promise(resolve => setTimeout(resolve, 10));
}

describe('useExecutionEvents（094 workspace 订阅范围）', () => {
  beforeEach(() => {
    FakeWebSocket.instances = [];
    mockWorkspace = null;
    vi.stubGlobal('WebSocket', FakeWebSocket);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('test_useExecutionEvents_initial_connection_includes_workspace_id', async () => {
    mockWorkspace = 1;
    const { unmount } = renderHook(() => useExecutionEvents());
    expect(FakeWebSocket.instances).toHaveLength(1);
    expect(FakeWebSocket.instances[0].url).toContain('workspace_id=1');
    unmount();
    await flushTimers();
  });

  it('test_useExecutionEvents_null_workspace_uses_unscoped_connection', async () => {
    mockWorkspace = null;
    const { unmount } = renderHook(() => useExecutionEvents());
    expect(FakeWebSocket.instances[0].url).not.toContain('workspace_id');
    unmount();
    await flushTimers();
  });

  it('test_useExecutionEvents_workspace_change_ignores_stale_onclose', async () => {
    mockWorkspace = 1;
    const { unmount, rerender } = renderHook(() => useExecutionEvents());
    expect(FakeWebSocket.instances[0].url).toContain('workspace_id=1');

    // 切换到 workspace 2：应关闭旧连接并立即以新参数重建
    mockWorkspace = 2;
    rerender();
    expect(FakeWebSocket.instances).toHaveLength(2);
    expect(FakeWebSocket.instances[0].closed).toBe(true);
    expect(FakeWebSocket.instances[1].url).toContain('workspace_id=2');

    // 关键断言：旧连接的 onclose 异步触发后，不得再建第三个连接。
    // 竞态未防护时：旧 onclose 把 sharedWs 置空并安排重连 → 产生 workspace_id=2 的重复连接
    await flushTimers();
    expect(FakeWebSocket.instances, '旧 onclose 不得触发额外连接').toHaveLength(2);
    unmount();
    await flushTimers();
  });
});
