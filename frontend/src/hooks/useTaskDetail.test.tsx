// useTaskDetail 单元测试。
// 覆盖数据层：初次拉详情（含 onTitleReady 上报）、有 loop_id 拉环路、再次执行、调接力上限（含失败抛错）、删除环路。
// 参照 src/components/todo-list/useBatchActions.test.tsx 的 renderHook + vi.hoisted/vi.mock 模式。
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';

// antd message 用 hoisted mock：避免 vi.mock 工厂内的 hoisting 顺序问题。
const mockMessage = vi.hoisted(() => ({ success: vi.fn(), warning: vi.fn(), error: vi.fn() }));
vi.mock('antd', () => ({ message: mockMessage }));

// 数据层依赖全部桩化：getTaskDetail / createTaskExecution / updateTask。
const mockApi = vi.hoisted(() => ({
  getTaskDetail: vi.fn(),
  createTaskExecution: vi.fn(),
  updateTask: vi.fn(),
}));
vi.mock('@/api/bundled', () => ({ default: mockApi }));

// 环路依赖：getLoop / deleteLoop。
const mockLoops = vi.hoisted(() => ({
  getLoop: vi.fn(),
  deleteLoop: vi.fn(),
}));
vi.mock('@/utils/database/loops', () => mockLoops);

import { useTaskDetail } from './useTaskDetail';
import type { TaskDetailData } from '@/types/task';

// 工艺环路任务（带 loop_id）：用于验证拉环路、删除、再次执行。
const loopTask: TaskDetailData = {
  task: { id: 1, title: '环路任务', status: 'running', execution_mode: 'loop', loop_id: 7, workspace_id: 2 },
  template: { display_name: '工艺A', version: '1', complexity: 'low' },
  steps: [],
  executions: [],
  loop: { id: 7, workspace_id: 2 },
};
// getLoop 返回的最小 LoopDetail（仅 handleDelete 用到 id / workspace_id）。
const mockLoopDetail = { id: 7, workspace_id: 2, name: '环路7' };

describe('useTaskDetail', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // 默认 happy path：拉详情返回环路任务，拉环路返回 mockLoopDetail。
    mockApi.getTaskDetail.mockResolvedValue(loopTask);
    mockLoops.getLoop.mockResolvedValue(mockLoopDetail);
    mockApi.createTaskExecution.mockResolvedValue({ id: 100 });
    mockApi.updateTask.mockResolvedValue({});
    mockLoops.deleteLoop.mockResolvedValue({});
  });

  it('test_useTaskDetail_初次挂载_拉详情并上报标题', async () => {
    const onTitleReady = vi.fn();
    const { result } = renderHook(() => useTaskDetail(1, 2, { onTitleReady }));

    // effect 异步执行：等待 detail 落位。
    await waitFor(() => expect(result.current.detail).not.toBeNull());
    // 入参顺序：(workspaceId, taskId) —— 与 hook 内 bundledApi.getTaskDetail(workspaceId, taskId) 一致。
    expect(mockApi.getTaskDetail).toHaveBeenCalledWith(2, 1);
    expect(result.current.detail?.task.title).toBe('环路任务');
    // 标题仅在初次加载上报一次。
    expect(onTitleReady).toHaveBeenCalledWith('环路任务');
  });

  it('test_useTaskDetail_详情含loop_id_并行拉环路', async () => {
    const { result } = renderHook(() => useTaskDetail(1, 2, {}));

    await waitFor(() => expect(mockLoops.getLoop).toHaveBeenCalledWith(2, 7));
    await waitFor(() => expect(result.current.loopDetail).toEqual(mockLoopDetail));
    expect(result.current.loopLoading).toBe(false);
  });

  it('test_useTaskDetail_委派任务无loop_id_不拉环路', async () => {
    // 委派任务不绑环路：覆盖 getTaskDetail 返回无 loop_id 的详情。
    const delegateTask: TaskDetailData = {
      task: { id: 2, title: '委派任务', status: 'running', execution_mode: 'delegate', workspace_id: 2 },
      steps: [], executions: [],
    };
    mockApi.getTaskDetail.mockResolvedValue(delegateTask);

    renderHook(() => useTaskDetail(2, 2, {}));
    await waitFor(() => expect(mockApi.getTaskDetail).toHaveBeenCalled());
    // 让微任务排空后再断言 getLoop 未被调用（effect 已跑完）。
    await new Promise((r) => setTimeout(r, 0));
    expect(mockLoops.getLoop).not.toHaveBeenCalled();
  });

  it('test_useTaskDetail_再次执行_建执行后刷新详情并关Modal', async () => {
    const onTriggered = vi.fn();
    const { result } = renderHook(() => useTaskDetail(1, 2, { onTriggered }));
    await waitFor(() => expect(result.current.detail).not.toBeNull());

    // 隔离：初次加载的调用清零，只观测 handleNewExec 触发的调用。
    vi.clearAllMocks();
    // 填需求（handleNewExec 校验非空）。
    await act(() => { result.current.setNewRequirement('新需求'); });
    await act(() => result.current.handleNewExec());

    // createTaskExecution 入参顺序：(workspaceId, taskId, requirement)。
    expect(mockApi.createTaskExecution).toHaveBeenCalledWith(2, 1, '新需求');
    // 刷新详情：getTaskDetail 再被调一次。
    expect(mockApi.getTaskDetail).toHaveBeenCalledWith(2, 1);
    // 成功后清空输入、通知宿主。
    expect(result.current.newRequirement).toBe('');
    expect(onTriggered).toHaveBeenCalled();
  });

  it('test_useTaskDetail_再次执行_空需求警告且不建执行', async () => {
    const { result } = renderHook(() => useTaskDetail(1, 2, {}));
    await waitFor(() => expect(result.current.detail).not.toBeNull());
    vi.clearAllMocks();

    await act(() => result.current.handleNewExec());
    // 空需求被校验拦截，不触发 createTaskExecution。
    expect(mockApi.createTaskExecution).not.toHaveBeenCalled();
    expect(mockMessage.warning).toHaveBeenCalledWith('请输入需求');
  });

  it('test_useTaskDetail_调接力上限_落库后刷新详情并回调', async () => {
    const onTriggered = vi.fn();
    const { result } = renderHook(() => useTaskDetail(1, 2, { onTriggered }));
    await waitFor(() => expect(result.current.detail).not.toBeNull());
    vi.clearAllMocks();

    await act(() => result.current.handleUpdateMax(5));
    // updateTask 入参：(workspaceId, taskId, { delegate_max_rounds })。
    expect(mockApi.updateTask).toHaveBeenCalledWith(2, 1, { delegate_max_rounds: 5 });
    expect(mockApi.getTaskDetail).toHaveBeenCalledWith(2, 1);
    expect(mockMessage.success).toHaveBeenCalledWith('上限已设为 5 轮');
    expect(onTriggered).toHaveBeenCalled();
  });

  it('test_useTaskDetail_调上限_恢复默认传null且提示恢复', async () => {
    const { result } = renderHook(() => useTaskDetail(1, 2, {}));
    await waitFor(() => expect(result.current.detail).not.toBeNull());
    vi.clearAllMocks();

    await act(() => result.current.handleUpdateMax(null));
    expect(mockApi.updateTask).toHaveBeenCalledWith(2, 1, { delegate_max_rounds: null });
    expect(mockMessage.success).toHaveBeenCalledWith('已恢复默认上限');
  });

  it('test_useTaskDetail_调上限失败_向上抛错不吞错', async () => {
    const { result } = renderHook(() => useTaskDetail(1, 2, {}));
    await waitFor(() => expect(result.current.detail).not.toBeNull());
    // updateTask 400（越界/非委派），后端中文 message 经拦截器透传。
    mockApi.updateTask.mockRejectedValueOnce(new Error('轮数越界'));

    // 失败必须向上抛错：RelayMaxEditor.submit 据此判定失败、保持 Popover 打开供重试。
    await act(async () => {
      await expect(result.current.handleUpdateMax(5)).rejects.toThrow('轮数越界');
    });
    expect(mockMessage.error).toHaveBeenCalledWith('轮数越界');
  });

  it('test_useTaskDetail_删除环路_落库并回调onLoopChanged', async () => {
    const onLoopChanged = vi.fn();
    const { result } = renderHook(() => useTaskDetail(1, 2, { onLoopChanged }));
    // 等待 loopDetail 就绪（handleDelete 依赖它）。
    await waitFor(() => expect(result.current.loopDetail).not.toBeNull());
    vi.clearAllMocks();

    await act(() => result.current.handleDelete());
    // deleteLoop 入参：(workspaceId, loopId)。
    expect(mockLoops.deleteLoop).toHaveBeenCalledWith(2, 7);
    expect(mockMessage.success).toHaveBeenCalledWith('已删除');
    expect(onLoopChanged).toHaveBeenCalled();
  });

  it('test_useTaskDetail_openReqModal_以任务描述预填', async () => {
    const { result } = renderHook(() => useTaskDetail(1, 2, {}));
    await waitFor(() => expect(result.current.detail).not.toBeNull());

    await act(() => result.current.openReqModal());
    // 预填取 description ?? title；loopTask 无 description，回退标题。
    expect(result.current.newRequirement).toBe('环路任务');
    expect(result.current.reqModalOpen).toBe(true);
    // closeReqModal 关闭。
    await act(() => result.current.closeReqModal());
    expect(result.current.reqModalOpen).toBe(false);
  });
});
