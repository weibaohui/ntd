// 任务详情数据 + 动作 hook（从 TaskDetailPanel 抽出，需求 093 重构）。
//
// 为什么单独抽 hook：
// TaskDetailPanel 原本把「拉任务详情 / 拉环路 / 删除 / 再次执行 / 调接力上限」等数据与
// 副作用逻辑全揉在组件里，既无法脱离组件单测，也让渲染层臃肿。抽成 hook 后：
// - 数据层可 renderHook + mock 单测（见 useTaskDetail.test.tsx）；
// - 组件回归声明式渲染，只消费 state + 调动作。
//
// 行为与原组件逐行等价：竞态守卫（alive 标志）、错误处理、副作用依赖、eslint-disable
// 注释全部原样保留，本次为纯结构抽取，不改逻辑。
// 复用既有模式：参照 src/hooks/useRunningBoard.ts（alive 竞态守卫 + 导出 state 接口）。

import { useState, useEffect, useCallback } from 'react';
import { message } from 'antd';
import bundledApi from '@/api/bundled';
import * as dbLoops from '@/utils/database/loops';
import type { LoopDetail } from '@/types/loop';
import type { TaskDetailData } from '@/types/task';

/** hook 对外暴露的状态与动作集合（参照 useRunningBoard 的 RunningBoardState 模式）。 */
export interface TaskDetailState {
  // —— 数据态 ——
  loading: boolean;
  detail: TaskDetailData | null;
  loopDetail: LoopDetail | null;
  loopLoading: boolean;
  // —— 再次执行 Modal 态：开关 / 输入文案 / 提交中 ——
  reqModalOpen: boolean;
  newRequirement: string;
  triggering: boolean;
  setNewRequirement: (v: string) => void;
  openReqModal: () => void;
  closeReqModal: () => void;
  // —— 动作 ——
  /** 提交新执行（createTaskExecution → 刷新详情 → 通知宿主）。 */
  handleNewExec: () => Promise<void>;
  /** 删除环路（deleteLoop → 通知宿主刷新）。 */
  handleDelete: () => Promise<void>;
  /** 内联调整接力上限覆盖（updateTask → 刷新详情；失败向上抛错让 Popover 不关）。 */
  handleUpdateMax: (max: number | null) => Promise<void>;
  /** 重拉任务详情（内部由 create/update 复用，亦供外部按需刷新）。 */
  refresh: () => Promise<void>;
}

/** useTaskDetail 的宿主回调（来自 TaskDetailPanel 的 props，透传进来）。 */
interface UseTaskDetailCallbacks {
  /** 任务标题加载完成后回调，供外层 PageCard 动态更新标题。 */
  onTitleReady?: (title: string) => void;
  /** 再次执行 / 调上限成功后回调，让宿主重拉列表。 */
  onTriggered?: () => void;
  /** 环路删除后通知宿主刷新列表。 */
  onLoopChanged?: () => void;
}

/**
 * 任务详情数据 + 动作。
 *
 * 数据流：先拉任务详情（get_task_detail）→ 若 task.loop_id 存在再拉完整 LoopDetail。
 * 两个 effect 各自用 `alive` 闭包标志丢弃 unmount / 切换后晚到的响应，防竞态写脏 state。
 *
 * @param taskId      任务 id
 * @param workspaceId 工作空间 id（拉详情 / 拉环路 / 删除均需）
 * @param cb          宿主回调（标题上报 / 刷新列表 / 删除通知）
 */
export function useTaskDetail(
  taskId: number,
  workspaceId: number,
  cb: UseTaskDetailCallbacks,
): TaskDetailState {
  const { onTitleReady, onTriggered, onLoopChanged } = cb;

  const [loading, setLoading] = useState(false);
  const [detail, setDetail] = useState<TaskDetailData | null>(null);
  const [loopDetail, setLoopDetail] = useState<LoopDetail | null>(null);
  const [loopLoading, setLoopLoading] = useState(false);
  const [triggering, setTriggering] = useState(false);
  const [reqModalOpen, setReqModalOpen] = useState(false);
  const [newRequirement, setNewRequirement] = useState('');

  // 重拉任务详情：create/update 成功后复用，保证 detail 与后端同步。
  // 刻意不调 onTitleReady：标题仅在首次拉取时上报，避免重复触发外层标题更新。
  const refresh = useCallback(async () => {
    const raw = await bundledApi.getTaskDetail(workspaceId, taskId) as TaskDetailData;
    setDetail(raw);
  }, [workspaceId, taskId]);

  // 拉取任务详情（含基本 loop 信息）。onTitleReady 是宿主回调，身份可能每次渲染都变，
  // 故仅依赖 taskId/workspaceId 触发拉取（与原组件一致，原标题上报只在首次拉取时发生）。
  useEffect(() => {
    let alive = true;
    setLoading(true);
    bundledApi.getTaskDetail(workspaceId, taskId)
      .then((raw) => {
        if (!alive) return;
        const d = raw as TaskDetailData;
        setDetail(d);
        if (onTitleReady && d.task?.title) onTitleReady(d.task.title);
      })
      .catch(() => { if (alive) message.error('加载任务详情失败'); })
      .finally(() => { if (alive) setLoading(false); });
    return () => { alive = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [taskId, workspaceId]);

  // 任务详情加载后，若有 loop_id 则并行拉取完整 LoopDetail。
  useEffect(() => {
    if (!detail) return;
    const lpId = detail.task.loop_id ?? detail.loop?.id;
    if (!lpId) return;
    let alive = true;
    setLoopLoading(true);
    const wsId = detail.task.workspace_id ?? detail.loop?.workspace_id ?? workspaceId;
    dbLoops.getLoop(wsId, lpId)
      .then((ld) => { if (alive) setLoopDetail(ld); })
      .catch(() => { /* 环路加载失败不影响任务展示 */ })
      .finally(() => { if (alive) setLoopLoading(false); });
    return () => { alive = false; };
  }, [detail, workspaceId]);

  // 删除环路。
  const handleDelete = useCallback(async () => {
    if (!loopDetail) return;
    const wsId = loopDetail.workspace_id ?? workspaceId;
    try {
      await dbLoops.deleteLoop(wsId, loopDetail.id);
      message.success('已删除');
      onLoopChanged?.();
    } catch {
      message.error('删除失败，环路可能正在被引用');
    }
  }, [loopDetail, workspaceId, onLoopChanged]);

  // 打开再次执行 Modal：以任务描述（或缺省标题）预填输入框。
  const openReqModal = useCallback(() => {
    if (detail) setNewRequirement(detail.task.description ?? detail.task.title);
    setReqModalOpen(true);
  }, [detail]);

  const closeReqModal = useCallback(() => setReqModalOpen(false), []);

  // 提交新执行：createTaskExecution 成功后关 Modal、清输入、重拉详情、通知宿主刷新列表。
  const handleNewExec = useCallback(async () => {
    if (!newRequirement.trim()) { message.warning('请输入需求'); return; }
    setTriggering(true);
    try {
      await bundledApi.createTaskExecution(workspaceId, taskId, newRequirement);
      message.success('新执行已创建');
      setReqModalOpen(false);
      setNewRequirement('');
      await refresh();
      onTriggered?.();
    } catch {
      message.error('创建失败');
    } finally {
      setTriggering(false);
    }
  }, [newRequirement, workspaceId, taskId, refresh, onTriggered]);

  // 内联调整某任务的接力上限覆盖（需求 092）：落库后重拉详情，effective 随之刷新。
  // 用 taskId/workspaceId（而非 detail.task），保证 detail 未就绪时也不引用空对象。
  // 失败时向上抛错：RelayMaxEditor.submit 据此判定失败、跳过关 Popover，让用户在原值上重试。
  const handleUpdateMax = useCallback(async (max: number | null) => {
    try {
      await bundledApi.updateTask(workspaceId, taskId, { delegate_max_rounds: max });
      await refresh();
      message.success(max == null ? '已恢复默认上限' : `上限已设为 ${max} 轮`);
      onTriggered?.();
    } catch (e) {
      // updateTask 400 时后端返回中文 message（越界/非委派），拦截器已透传，先弹给用户可见提示。
      message.error(e instanceof Error ? e.message : '更新接力上限失败');
      throw e;
    }
  }, [workspaceId, taskId, refresh, onTriggered]);

  return {
    loading, detail, loopDetail, loopLoading,
    reqModalOpen, newRequirement, triggering,
    setNewRequirement, openReqModal, closeReqModal,
    handleNewExec, handleDelete, handleUpdateMax, refresh,
  };
}
