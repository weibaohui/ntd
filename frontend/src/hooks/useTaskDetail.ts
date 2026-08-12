// 任务详情数据 + 动作 hook（从 TaskDetailPanel 抽出，需求 093 重构）。
//
// 为什么单独抽 hook：
// TaskDetailPanel 原本把「拉任务详情 / 拉环路 / 删除 / 再次执行 / 调接力上限」等数据与
// 副作用逻辑全揉在组件里，既无法脱离组件单测，也让渲染层臃肿。抽成 hook 后：
// - 数据层可 renderHook + mock 单测（见 useTaskDetail.test.tsx）；
// - 组件回归声明式渲染，只消费 state + 调动作。
//
// 职责边界：本 hook 只管「任务数据 + 对任务的变更」；Modal 开关 / 输入文案等纯 UI 态
// 留在组件，故再次执行以 handleNewExec(requirement) 入参形式接收需求、返回是否成功，
// 由组件决定关 Modal 与清输入。refresh 仅内部复用，不对外暴露（YAGNI）。
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
  // —— 再次执行：提交 loading 态 ——
  triggering: boolean;
  // —— 动作 ——
  /** 提交新执行：接收需求文本，成功返回 true（组件据此关 Modal），空/失败返回 false。 */
  handleNewExec: (requirement: string) => Promise<boolean>;
  /** 删除任务（NTD-014-A）：删除成功后通知宿主跳回列表。 */
  handleDelete: () => Promise<void>;
  /** 内联调整接力上限覆盖（updateTask → 刷新详情；失败向上抛错让 Popover 不关）。 */
  handleUpdateMax: (max: number | null) => Promise<void>;
}

/** useTaskDetail 的宿主回调（来自 TaskDetailPanel 的 props，透传进来）。 */
interface UseTaskDetailCallbacks {
  /** 任务标题加载完成后回调，供外层 PageCard 动态更新标题。 */
  onTitleReady?: (title: string) => void;
  /** 再次执行 / 调上限成功后回调，让宿主重拉列表。 */
  onTriggered?: () => void;
  /** 任务删除成功后回调，让宿主跳回任务列表（NTD-014-A）。 */
  onDeleted?: () => void;
}

/**
 * 任务详情数据 + 动作。
 *
 * 数据流：先拉任务详情（get_task_detail）→ 若 task.loop_id 存在再拉完整 LoopDetail。
 * 两个 effect 各自用 `alive` 闭包标志丢弃 unmount / 切换后晚到的响应，防竞态写脏 state。
 *
 * 【函数长度豁免说明】本 hook 体量超 50 行（5 个 state + 2 effect + 4 useCallback），
 * 不符合 CLAUDE.md 四类豁免场景，但刻意不拆，理由命中豁免总原则「强行拆分将导致数据
 * 碎片化」：detail / loopDetail / refreshing 等状态构成一个紧耦合的「任务详情状态机」，
 * 删除 / 再次执行 / 调上限都读写同一份 detail，拆成多个子 hook 须在线程间传递共享
 * setter 与回调，反而增加阅读与传参成本。与既有 src/hooks/useRunningBoard.ts 同形态
 * （同为数据+动作聚合 hook，亦超 50 行），保持一致。行为零变更：竞态守卫、effect 依赖、
 * 错误处理（含 handleUpdateMax 的 throw e）全部自原组件逐项保留。
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
  const { onTitleReady, onTriggered, onDeleted } = cb;

  const [loading, setLoading] = useState(false);
  const [detail, setDetail] = useState<TaskDetailData | null>(null);
  const [loopDetail, setLoopDetail] = useState<LoopDetail | null>(null);
  const [loopLoading, setLoopLoading] = useState(false);
  const [triggering, setTriggering] = useState(false);

  // 重拉任务详情：create/update 成功后复用，保证 detail 与后端同步。
  // 刻意不调 onTitleReady：标题仅在首次拉取时上报，避免重复触发外层标题更新。
  // 仅内部使用，不进 TaskDetailState（无外部消费者，避免无谓的公开面）。
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

  // 删除任务（NTD-014-A）。原实现误删关联环路（deleteLoop），导致任务悬空 + 环路丢失；
  // 现改为调任务删除接口，成功后通知宿主跳回列表；失败透传后端错误信息。
  const handleDelete = useCallback(async () => {
    try {
      await bundledApi.deleteTask(workspaceId, taskId);
      message.success('任务已删除');
      onDeleted?.();
    } catch (e) {
      message.error(e instanceof Error ? e.message : '删除任务失败');
    }
  }, [workspaceId, taskId, onDeleted]);

  // 提交新执行：接收需求文本（Modal 输入态由组件持有），成功返回 true。
  // 成功后重拉详情 + 通知宿主刷新；Modal 关闭 / 输入清空交由组件在 true 时处理。
  const handleNewExec = useCallback(async (requirement: string): Promise<boolean> => {
    if (!requirement.trim()) { message.warning('请输入需求'); return false; }
    setTriggering(true);
    try {
      await bundledApi.createTaskExecution(workspaceId, taskId, requirement);
      message.success('新执行已创建');
      await refresh();
      onTriggered?.();
      return true;
    } catch {
      message.error('创建失败');
      return false;
    } finally {
      setTriggering(false);
    }
  }, [workspaceId, taskId, refresh, onTriggered]);

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
    loading, detail, loopDetail, loopLoading, triggering,
    handleNewExec, handleDelete, handleUpdateMax,
  };
}
