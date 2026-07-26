// useBatchActions — 事项 / 环路列表共享的批量操作 hook。
//
// 设计要点（028-列表详情独立路由-设计 §5.2）：
// 1. 把原 TodoList 内部散落的 4 类批量 Modal（更换执行器 / 强停 / 工作空间复制移动 / 暂停恢复周期执行）
//    统一抽离，避免 TodoListView 与 LoopListView 重复实现。
// 2. mode 参数区分调用方：'item' = 事项列表，'loop' = 环路列表，
//    内部根据 mode 选择调 db（事项）还是 dbLoops（环路）。
// 3. selectedIds 受控：由调用方持有，本 hook 只暴露 open* 触发函数；
//    操作完成后通过 onClearSelection 通知调用方清空选中，避免对已操作 id 重复执行。
// 4. modals 作为 JSX 返回，调用方在组件末尾一次性渲染，避免重复代码。
// 5. 单函数 ≤ 30 行：所有 confirm handler 拆分为「调 API / 提示用户 / 刷新数据 / 关闭 Modal」四步，
//    并通过辅助函数复用消息提示逻辑。

import { useState, useCallback, useMemo, type ReactNode } from 'react';
import { Modal, App as AntApp } from 'antd';
import {
  SwapOutlined,
  CopyOutlined,
  DragOutlined,
  StopOutlined,
  PauseCircleOutlined,
  PlayCircleOutlined,
} from '@ant-design/icons';
import * as db from '@/utils/database';
import * as dbLoops from '@/utils/database/loops';
import { EXECUTORS_FOR_PICKER } from '@/types/execution';
import { ExecutorPicker } from '@/components/todo-drawer/ExecutorPicker';
import { WorkspaceSwitcher } from '@/components/shell/WorkspaceSwitcher';
import type { BatchActionItem } from '@/components/common/ActionToolbar';

/** 列表模式：决定调 db 还是 dbLoops，以及哪些批量操作可见。 */
export type BatchMode = 'item' | 'loop';

export interface UseBatchActionsOptions {
  /** 列表模式：'item' = 事项, 'loop' = 环路。决定调 db 还是 dbLoops。 */
  mode: BatchMode;
  /** 当前选中的工作空间 id（操作源空间，body 中 workspace_id 字段）。 */
  selectedWorkspace: number | null;
  /** 操作成功后刷新事项列表（仅 mode='item' 时调用）。 */
  onRefreshItems?: () => void;
  /** 操作成功后刷新环路列表（仅 mode='loop' 时调用）。 */
  onRefreshLoops?: () => void;
  /** 操作完成后清空选中（多数场景都需要，避免对已操作的 id 重复执行）。 */
  onClearSelection?: () => void;
}

export interface UseBatchActionsResult {
  /** 顶部 ActionToolbar 用的批量菜单项。 */
  batchActions: BatchActionItem<number>[];
  /** 所有 Modal 的 JSX，调用方在组件末尾一次性渲染。 */
  modals: ReactNode;
}

/**
 * 事项 / 环路列表共享的批量操作 hook。
 *
 * 整体处理思路：
 * 1. 内部维护 4 类批量 Modal 的开关与待操作 id 列表（受控于本 hook）。
 * 2. 暴露 batchActions（菜单项）给 ActionToolbar，暴露 modals（JSX）给调用方渲染。
 * 3. 每个 confirm handler 负责：调 API → 提示用户 → 刷新数据 → 关闭 Modal → 清空选中。
 */
export function useBatchActions(opts: UseBatchActionsOptions): UseBatchActionsResult {
  const { mode, selectedWorkspace, onRefreshItems, onRefreshLoops, onClearSelection } = opts;
  const { message } = AntApp.useApp();

  // —— 批量更换执行器（仅 item 模式） ——
  const [executorModalOpen, setExecutorModalOpen] = useState(false);
  const [pendingExecutorChangeIds, setPendingExecutorChangeIds] = useState<number[]>([]);

  // —— 强停环路确认（仅 loop 模式） ——
  const [forceStopModalOpen, setForceStopModalOpen] = useState(false);
  const [pendingForceStopIds, setPendingForceStopIds] = useState<number[]>([]);

  // —— 批量工作空间复制/移动（item/loop 共用） ——
  const [workspaceBatchModalOpen, setWorkspaceBatchModalOpen] = useState(false);
  const [workspaceBatchMode, setWorkspaceBatchMode] = useState<'copy' | 'move'>('copy');
  const [workspaceBatchTarget, setWorkspaceBatchTarget] = useState<number | null>(null);
  const [pendingWorkspaceBatchIds, setPendingWorkspaceBatchIds] = useState<number[]>([]);
  const [workspaceBatchProcessing, setWorkspaceBatchProcessing] = useState(false);

  // —— 批量暂停/恢复周期执行（仅 item 模式） ——
  const [schedulerBatchModalOpen, setSchedulerBatchModalOpen] = useState(false);
  const [schedulerBatchMode, setSchedulerBatchMode] = useState<'pause' | 'resume'>('pause');
  const [pendingSchedulerBatchIds, setPendingSchedulerBatchIds] = useState<number[]>([]);
  const [schedulerBatchProcessing, setSchedulerBatchProcessing] = useState(false);

  // ─── 触发函数：打开对应 Modal ─────────────────────────────────
  // 每个 open 函数仅做 state 赋值，业务逻辑在 confirm handler 里。
  const openChangeExecutor = useCallback((ids: number[]) => {
    setPendingExecutorChangeIds(ids);
    setExecutorModalOpen(true);
  }, []);

  const openForceStop = useCallback((ids: number[]) => {
    setPendingForceStopIds(ids);
    setForceStopModalOpen(true);
  }, []);

  const openCopyWorkspace = useCallback((ids: number[]) => {
    setWorkspaceBatchMode('copy');
    setPendingWorkspaceBatchIds(ids);
    setWorkspaceBatchTarget(null);
    setWorkspaceBatchModalOpen(true);
  }, []);

  const openMoveWorkspace = useCallback((ids: number[]) => {
    setWorkspaceBatchMode('move');
    setPendingWorkspaceBatchIds(ids);
    setWorkspaceBatchTarget(null);
    setWorkspaceBatchModalOpen(true);
  }, []);

  const openPauseScheduler = useCallback((ids: number[]) => {
    setSchedulerBatchMode('pause');
    setPendingSchedulerBatchIds(ids);
    setSchedulerBatchModalOpen(true);
  }, []);

  const openResumeScheduler = useCallback((ids: number[]) => {
    setSchedulerBatchMode('resume');
    setPendingSchedulerBatchIds(ids);
    setSchedulerBatchModalOpen(true);
  }, []);

  // ─── 确认回调：调 API + 刷新 + 清空选中 ────────────────────────

  /** 把 {updated_count, total} 渲染为成功/警告消息。 */
  const reportBatchResult = useCallback((result: { updated_count: number; total: number }, actionLabel: string) => {
    if (result.updated_count === result.total) {
      message.success(`已${actionLabel} ${result.updated_count} 项`);
    } else {
      message.warning(`${actionLabel}成功 ${result.updated_count} 条，失败 ${result.total - result.updated_count} 条`);
    }
  }, [message]);

  /** 确认更换执行器：调 batchUpdateTodosExecutor，刷新事项列表。 */
  const handleConfirmChangeExecutor = useCallback(async (executor: string) => {
    const ids = pendingExecutorChangeIds;
    if (ids.length === 0) return;
    // 工作空间未选时不发请求，避免向 /workspaces/0/... 发出无效调用
    if (selectedWorkspace == null) {
      message.warning('请先选择工作空间');
      return;
    }
    setExecutorModalOpen(false);
    setPendingExecutorChangeIds([]);
    try {
      const result = await db.batchUpdateTodosExecutor(selectedWorkspace, ids, executor);
      if (result.failed.length === 0) {
        message.success(`已为 ${result.updated.length} 项更换执行器为「${executor}」`);
      } else {
        message.warning(`成功 ${result.updated.length} 条，失败 ${result.failed.length} 条`);
      }
      onRefreshItems?.();
    } catch {
      // axios 拦截器已弹错，这里静默
    } finally {
      onClearSelection?.();
    }
  }, [pendingExecutorChangeIds, selectedWorkspace, message, onRefreshItems, onClearSelection]);

  /** 确认强停环路：调 forceStopLoops（占位实现，后端待补）。 */
  const handleConfirmForceStop = useCallback(async () => {
    const ids = pendingForceStopIds;
    if (ids.length === 0) return;
    setForceStopModalOpen(false);
    setPendingForceStopIds([]);
    try {
      const result = await dbLoops.forceStopLoops(ids);
      if (result.stopped.length > 0) {
        message.success(`已强停 ${result.stopped.length} 个环路`);
      } else {
        message.warning(`环路强停功能开发中（已选 ${ids.length} 个）`);
      }
    } finally {
      onClearSelection?.();
    }
  }, [pendingForceStopIds, message, onClearSelection]);

  /** 确认工作空间复制/移动：根据 mode 调对应 API，刷新对应列表。 */
  const handleConfirmWorkspaceBatch = useCallback(async () => {
    const ids = pendingWorkspaceBatchIds;
    const target = workspaceBatchTarget;
    if (ids.length === 0 || target == null) return;
    // 工作空间未选时不发请求，避免 srcWs=0 的无效调用
    if (selectedWorkspace == null) {
      message.warning('请先选择工作空间');
      return;
    }
    setWorkspaceBatchProcessing(true);
    try {
      const srcWs = selectedWorkspace;
      const isCopy = workspaceBatchMode === 'copy';
      // mode 区分调 db 还是 dbLoops；返回结构相同 {updated_count, total}
      const result = mode === 'item'
        ? (isCopy
          ? await db.batchCopyTodosWorkspace(srcWs, ids, target)
          : await db.batchMoveTodosWorkspace(srcWs, ids, target))
        : (isCopy
          ? await dbLoops.batchCopyLoopsWorkspace(srcWs, ids, target)
          : await dbLoops.batchMoveLoopsWorkspace(srcWs, ids, target));
      reportBatchResult(result, isCopy ? '复制' : '移动');
      // 刷新对应列表：事项模式刷新 items，环路模式刷新 loops
      if (mode === 'item') {
        onRefreshItems?.();
      } else {
        onRefreshLoops?.();
      }
      setWorkspaceBatchModalOpen(false);
      onClearSelection?.();
    } catch (err) {
      message.error(`操作失败: ${err instanceof Error ? err.message : '未知错误'}`);
    } finally {
      setWorkspaceBatchProcessing(false);
    }
  }, [pendingWorkspaceBatchIds, workspaceBatchTarget, workspaceBatchMode, mode, selectedWorkspace, message, onRefreshItems, onRefreshLoops, onClearSelection, reportBatchResult]);

  /** 确认暂停/恢复周期执行：调 batchPauseScheduler/batchResumeScheduler。 */
  const handleConfirmSchedulerBatch = useCallback(async () => {
    const ids = pendingSchedulerBatchIds;
    if (ids.length === 0) return;
    // 工作空间未选时不发请求，避免向 /workspaces/0/... 发出无效调用
    if (selectedWorkspace == null) {
      message.warning('请先选择工作空间');
      return;
    }
    setSchedulerBatchProcessing(true);
    try {
      const isPause = schedulerBatchMode === 'pause';
      const result = isPause
        ? await db.batchPauseScheduler(selectedWorkspace, ids)
        : await db.batchResumeScheduler(selectedWorkspace, ids);
      reportBatchResult(result, isPause ? '暂停' : '恢复');
      onRefreshItems?.();
      setSchedulerBatchModalOpen(false);
      onClearSelection?.();
    } catch (err) {
      message.error(`操作失败: ${err instanceof Error ? err.message : '未知错误'}`);
    } finally {
      setSchedulerBatchProcessing(false);
    }
  }, [pendingSchedulerBatchIds, schedulerBatchMode, selectedWorkspace, message, onRefreshItems, onClearSelection, reportBatchResult]);

  // ─── 顶部 ActionToolbar 的批量菜单项（按 mode 区分） ─────────
  const batchActions = useMemo<BatchActionItem<number>[]>(() => {
    if (mode === 'item') {
      return [
        { key: 'change-executor', label: '更换执行器', icon: <SwapOutlined />, onClick: openChangeExecutor },
        { key: 'copy-workspace', label: '复制到', icon: <CopyOutlined />, onClick: openCopyWorkspace },
        { key: 'move-workspace', label: '移动到', icon: <DragOutlined />, onClick: openMoveWorkspace },
        { key: 'pause-scheduler', label: '暂停周期执行', icon: <PauseCircleOutlined />, onClick: openPauseScheduler },
        { key: 'resume-scheduler', label: '恢复周期执行', icon: <PlayCircleOutlined />, onClick: openResumeScheduler },
      ];
    }
    return [
      { key: 'copy-workspace', label: '复制到', icon: <CopyOutlined />, onClick: openCopyWorkspace },
      { key: 'move-workspace', label: '移动到', icon: <DragOutlined />, onClick: openMoveWorkspace },
      { key: 'force-stop', label: '强停', icon: <StopOutlined />, danger: true, onClick: openForceStop },
    ];
  }, [mode, openChangeExecutor, openCopyWorkspace, openMoveWorkspace, openPauseScheduler, openResumeScheduler, openForceStop]);

  // ─── Modal JSX 一次性返回 ────────────────────────────────────
  const modals = (
    <>
      {/* 批量更换执行器 Modal（仅事项模式） */}
      {mode === 'item' && (
        <Modal
          title={`更换执行器（${pendingExecutorChangeIds.length} 项）`}
          open={executorModalOpen}
          onCancel={() => { setExecutorModalOpen(false); setPendingExecutorChangeIds([]); }}
          footer={null}
          destroyOnHidden
        >
          <ExecutorPicker
            executor=""
            executorOptions={EXECUTORS_FOR_PICKER}
            onChange={(v: string) => handleConfirmChangeExecutor(v)}
          />
        </Modal>
      )}

      {/* 强停环路确认 Modal（仅环路模式） */}
      {mode === 'loop' && (
        <Modal
          title="强停环路"
          open={forceStopModalOpen}
          onOk={handleConfirmForceStop}
          onCancel={() => { setForceStopModalOpen(false); setPendingForceStopIds([]); }}
          okText="强停"
          cancelText="取消"
          okButtonProps={{ danger: true }}
          destroyOnHidden
        >
          <p>将停止 <strong>{pendingForceStopIds.length}</strong> 个环路关联的所有正在运行的执行。</p>
          <p style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>
            （强停功能开发中，详见 utils/database/loops.ts 的 forceStopLoops 注释。）
          </p>
        </Modal>
      )}

      {/* 批量工作空间复制/移动 Modal（共用） */}
      <Modal
        title={workspaceBatchMode === 'copy' ? '复制到工作空间' : '移动到工作空间'}
        open={workspaceBatchModalOpen}
        onOk={handleConfirmWorkspaceBatch}
        onCancel={() => { setWorkspaceBatchModalOpen(false); setPendingWorkspaceBatchIds([]); }}
        okText={workspaceBatchMode === 'copy' ? '确认复制' : '确认移动'}
        cancelText="取消"
        confirmLoading={workspaceBatchProcessing}
        okButtonProps={{ disabled: workspaceBatchTarget == null }}
        destroyOnHidden
      >
        <p>
          {workspaceBatchMode === 'copy' ? '复制' : '移动'} <strong>{pendingWorkspaceBatchIds.length}</strong> 项到目标工作空间：
        </p>
        <div style={{ marginTop: 12 }}>
          <WorkspaceSwitcher
            value={workspaceBatchTarget}
            showAddOption={false}
            onChange={(v) => setWorkspaceBatchTarget(v)}
          />
        </div>
        {workspaceBatchMode === 'copy' && (
          <p style={{ color: 'var(--color-text-tertiary)', fontSize: 12, marginTop: 8 }}>
            复制后，原工作空间和目标工作空间中各有一份相同的条目。
          </p>
        )}
      </Modal>

      {/* 批量暂停/恢复周期执行 Modal（仅事项模式） */}
      {mode === 'item' && (
        <Modal
          title={schedulerBatchMode === 'pause' ? '暂停周期执行' : '恢复周期执行'}
          open={schedulerBatchModalOpen}
          onOk={handleConfirmSchedulerBatch}
          onCancel={() => { setSchedulerBatchModalOpen(false); setPendingSchedulerBatchIds([]); }}
          okText="确认"
          cancelText="取消"
          confirmLoading={schedulerBatchProcessing}
          destroyOnHidden
        >
          <p>
            {schedulerBatchMode === 'pause'
              ? <>确定暂停 <strong>{pendingSchedulerBatchIds.length}</strong> 项的周期执行吗？暂停后定时调度将不再触发。</>
              : <>确定恢复 <strong>{pendingSchedulerBatchIds.length}</strong> 项的周期执行吗？将使用原有的调度配置继续运行。</>
            }
          </p>
        </Modal>
      )}
    </>
  );

  return { batchActions, modals };
}
