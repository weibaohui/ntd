// useBatchActions — 事项 / 环路列表共享的批量操作 hook。
//
// 设计要点（028-列表详情独立路由-设计 §5.2）：
// 1. 把原 TodoList 内部散落的 4 类批量 Modal（更换执行器 / 强停 / 工作空间复制移动 / 暂停恢复周期执行）
//    统一抽离，避免 TodoListView 与 LoopListView 重复实现。
// 2. mode 参数区分调用方：'item' = 事项列表，'loop' = 环路列表。
// 3. selectedIds 受控：由调用方持有，本 hook 只暴露 open* 触发函数。
// 4. modals 作为 JSX 返回，调用方在组件末尾一次性渲染。
// 5. 单函数 ≤ 30 行：每个 Modal 抽为独立组件，hook 主体仅负责状态编排。

import { useState, useCallback, useMemo, type Key, type ReactNode } from 'react';
import { Modal, App as AntApp } from 'antd';
import {
  SwapOutlined,
  CopyOutlined,
  DragOutlined,
  StopOutlined,
  PauseCircleOutlined,
  PlayCircleOutlined,
  DeleteOutlined,
} from '@ant-design/icons';
import * as db from '@/utils/database';
import * as dbLoops from '@/utils/database/loops';
import { EXECUTORS_FOR_PICKER } from '@/types/execution';
import { ExecutorPicker } from '@/components/todo-drawer/ExecutorPicker';
import { WorkspaceSwitcher } from '@/components/shell/WorkspaceSwitcher';
// BatchActionItem 原住在 common/ActionToolbar.tsx；该组件确认全仓无渲染方（PR #1073 评审修复删除），
// 类型随唯一消费方 hook 迁入，避免为保一个类型留下整份死组件文件。
/** 单个批量操作菜单项。渲染方不感知业务语义，只负责点击时回传当前已选 id 列表。
 *  仅本文件（UseBatchActionsResult/batchActions）消费，故不导出——外部按结构化类型接入。 */
interface BatchActionItem<TId extends Key = number> {
  key: string;
  label: string;
  icon?: ReactNode;
  danger?: boolean;
  /** 收到当前 selectedIds，由父组件决定如何执行（弹 Modal / 调 API 等）。 */
  onClick: (selectedIds: TId[]) => void;
}

/** 列表模式：决定调 db 还是 dbLoops，以及哪些批量操作可见。 */
type BatchMode = 'item' | 'loop';

export interface UseBatchActionsOptions {
  mode: BatchMode;
  selectedWorkspace: number | null;
  onRefreshItems?: () => void;
  onRefreshLoops?: () => void;
  onClearSelection?: () => void;
}

export interface UseBatchActionsResult {
  batchActions: BatchActionItem<number>[];
  modals: ReactNode;
}

// ─── Modal 子组件（提取让 hook 主体 ≤30 行） ──────────────────

/** 批量更换执行器 Modal（仅事项模式）。 */
function ExecutorChangeModal({
  open, ids, onConfirm, onCancel,
}: {
  open: boolean; ids: number[]; onConfirm: (executor: string) => void; onCancel: () => void;
}) {
  return (
    <Modal title={`更换执行器（${ids.length} 项）`} open={open} onCancel={onCancel} footer={null} destroyOnHidden>
      <ExecutorPicker executor="" executorOptions={EXECUTORS_FOR_PICKER} onChange={(v: string) => onConfirm(v)} />
    </Modal>
  );
}

/** 强停环路确认 Modal（仅环路模式）。 */
function ForceStopModal({
  open, ids, onConfirm, onCancel,
}: {
  open: boolean; ids: number[]; onConfirm: () => void; onCancel: () => void;
}) {
  return (
    <Modal
      title="强停环路" open={open} onOk={onConfirm} onCancel={onCancel}
      okText="强停" cancelText="取消" okButtonProps={{ danger: true }} destroyOnHidden
    >
      <p>将停止 <strong>{ids.length}</strong> 个环路关联的所有正在运行的执行。</p>
      <p style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>
        （强停功能开发中，详见 utils/database/loops.ts 的 forceStopLoops 注释。）
      </p>
    </Modal>
  );
}

/** 批量工作空间复制/移动 Modal（共用）。 */
function WorkspaceBatchModal({
  open, ids, mode, processing, target, onTargetChange, onConfirm, onCancel,
}: {
  open: boolean; ids: number[]; mode: 'copy' | 'move'; processing: boolean;
  target: number | null; onTargetChange: (v: number | null) => void;
  onConfirm: () => void; onCancel: () => void;
}) {
  return (
    <Modal
      title={mode === 'copy' ? '复制到工作空间' : '移动到工作空间'}
      open={open} onOk={onConfirm} onCancel={onCancel}
      okText={mode === 'copy' ? '确认复制' : '确认移动'} cancelText="取消"
      confirmLoading={processing} okButtonProps={{ disabled: target == null }} destroyOnHidden
    >
      <p>{mode === 'copy' ? '复制' : '移动'} <strong>{ids.length}</strong> 项到目标工作空间：</p>
      <div style={{ marginTop: 12 }}>
        <WorkspaceSwitcher value={target} showAddOption={false} onChange={onTargetChange} />
      </div>
      {mode === 'copy' && (
        <p style={{ color: 'var(--color-text-tertiary)', fontSize: 12, marginTop: 8 }}>
          复制后，原工作空间和目标工作空间中各有一份相同的条目。
        </p>
      )}
    </Modal>
  );
}

/** 批量暂停/恢复周期执行 Modal（仅事项模式）。 */
function SchedulerBatchModal({
  open, ids, mode, processing, onConfirm, onCancel,
}: {
  open: boolean; ids: number[]; mode: 'pause' | 'resume'; processing: boolean;
  onConfirm: () => void; onCancel: () => void;
}) {
  return (
    <Modal
      title={mode === 'pause' ? '暂停周期执行' : '恢复周期执行'}
      open={open} onOk={onConfirm} onCancel={onCancel}
      okText="确认" cancelText="取消" confirmLoading={processing} destroyOnHidden
    >
      <p>
        {mode === 'pause'
          ? <>确定暂停 <strong>{ids.length}</strong> 项的周期执行吗？暂停后定时调度将不再触发。</>
          : <>确定恢复 <strong>{ids.length}</strong> 项的周期执行吗？将使用原有的调度配置继续运行。</>
        }
      </p>
    </Modal>
  );
}

/** 批量删除确认 Modal（事项/环路共用）。 */
function BatchDeleteConfirmModal({
  open, ids, processing, mode, onConfirm, onCancel,
}: {
  open: boolean; ids: number[]; processing: boolean; mode: string;
  onConfirm: () => void; onCancel: () => void;
}) {
  return (
    <Modal
      title={mode === 'loop' ? '确认删除环路' : '确认删除事项'}
      open={open} onOk={onConfirm} onCancel={onCancel}
      okText="删除" cancelText="取消" okButtonProps={{ danger: true, loading: processing }} destroyOnHidden
    >
      <p>确定删除选中的 <strong>{ids.length}</strong> 个{mode === 'loop' ? '环路' : '事项'}吗？</p>
      <p style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>
        {mode === 'loop' ? '删除后将级联删除 triggers、steps 及关联数据。' : '事项将被标记为已删除。'}
      </p>
    </Modal>
  );
}

/** 把 {updated_count, total} 渲染为成功/警告消息。 */
function reportBatchResult(result: { updated_count: number; total: number }, actionLabel: string, message: ReturnType<typeof AntApp.useApp>['message']) {
  if (result.updated_count === result.total) {
    message.success(`已${actionLabel} ${result.updated_count} 项`);
  } else {
    message.warning(`${actionLabel}成功 ${result.updated_count} 条，失败 ${result.total - result.updated_count} 条`);
  }
}

/**
 * 事项 / 环路列表共享的批量操作 hook。
 *
 * 整体处理思路：
 * 1. 内部维护 4 类批量 Modal 的开关与待操作 id 列表。
 * 2. 暴露 batchActions（菜单项）给调用方，暴露 modals（JSX）给调用方渲染。
 * 3. 每个 confirm handler 负责：调 API → 提示用户 → 刷新数据 → 关闭 Modal → 清空选中。
 */
export function useBatchActions(opts: UseBatchActionsOptions): UseBatchActionsResult {
  const { mode, selectedWorkspace, onRefreshItems, onRefreshLoops, onClearSelection } = opts;
  const { message } = AntApp.useApp();

  // 4 类批量 Modal 的受控状态
  const [executorModalOpen, setExecutorModalOpen] = useState(false);
  const [pendingExecutorChangeIds, setPendingExecutorChangeIds] = useState<number[]>([]);
  const [forceStopModalOpen, setForceStopModalOpen] = useState(false);
  const [pendingForceStopIds, setPendingForceStopIds] = useState<number[]>([]);
  const [workspaceBatchModalOpen, setWorkspaceBatchModalOpen] = useState(false);
  const [workspaceBatchMode, setWorkspaceBatchMode] = useState<'copy' | 'move'>('copy');
  const [workspaceBatchTarget, setWorkspaceBatchTarget] = useState<number | null>(null);
  const [pendingWorkspaceBatchIds, setPendingWorkspaceBatchIds] = useState<number[]>([]);
  const [workspaceBatchProcessing, setWorkspaceBatchProcessing] = useState(false);
  const [schedulerBatchModalOpen, setSchedulerBatchModalOpen] = useState(false);
  const [schedulerBatchMode, setSchedulerBatchMode] = useState<'pause' | 'resume'>('pause');
  const [pendingSchedulerBatchIds, setPendingSchedulerBatchIds] = useState<number[]>([]);
  const [schedulerBatchProcessing, setSchedulerBatchProcessing] = useState(false);
  // 批量删除 Modal 状态
  const [deleteModalOpen, setDeleteModalOpen] = useState(false);
  const [pendingDeleteIds, setPendingDeleteIds] = useState<number[]>([]);
  const [deleteProcessing, setDeleteProcessing] = useState(false);

  // ─── 触发函数：仅 state 赋值 ─────────────────────────────────
  const openChangeExecutor = useCallback((ids: number[]) => { setPendingExecutorChangeIds(ids); setExecutorModalOpen(true); }, []);
  const openForceStop = useCallback((ids: number[]) => { setPendingForceStopIds(ids); setForceStopModalOpen(true); }, []);
  const openCopyWorkspace = useCallback((ids: number[]) => { setWorkspaceBatchMode('copy'); setPendingWorkspaceBatchIds(ids); setWorkspaceBatchTarget(null); setWorkspaceBatchModalOpen(true); }, []);
  const openMoveWorkspace = useCallback((ids: number[]) => { setWorkspaceBatchMode('move'); setPendingWorkspaceBatchIds(ids); setWorkspaceBatchTarget(null); setWorkspaceBatchModalOpen(true); }, []);
  const openPauseScheduler = useCallback((ids: number[]) => { setSchedulerBatchMode('pause'); setPendingSchedulerBatchIds(ids); setSchedulerBatchModalOpen(true); }, []);
  const openResumeScheduler = useCallback((ids: number[]) => { setSchedulerBatchMode('resume'); setPendingSchedulerBatchIds(ids); setSchedulerBatchModalOpen(true); }, []);
  // 批量删除：打开确认 Modal
  const openBatchDelete = useCallback((ids: number[]) => { setPendingDeleteIds(ids); setDeleteModalOpen(true); }, []);

  // ─── 确认回调 ────────────────────────────────────────────────

  const handleConfirmChangeExecutor = useCallback(async (executor: string) => {
    if (pendingExecutorChangeIds.length === 0 || selectedWorkspace == null) {
      if (selectedWorkspace == null) message.warning('请先选择工作空间');
      return;
    }
    setExecutorModalOpen(false);
    setPendingExecutorChangeIds([]);
    try {
      const result = await db.batchUpdateTodosExecutor(selectedWorkspace, pendingExecutorChangeIds, executor);
      if (result.failed.length === 0) {
        message.success(`已为 ${result.updated.length} 项更换执行器为「${executor}」`);
      } else {
        message.warning(`成功 ${result.updated.length} 条，失败 ${result.failed.length} 条`);
      }
      onRefreshItems?.();
    } catch { /* axios 拦截器已弹错 */ } finally { onClearSelection?.(); }
  }, [pendingExecutorChangeIds, selectedWorkspace, message, onRefreshItems, onClearSelection]);

  const handleConfirmForceStop = useCallback(async () => {
    if (pendingForceStopIds.length === 0) return;
    setForceStopModalOpen(false);
    setPendingForceStopIds([]);
    try {
      const result = await dbLoops.forceStopLoops(pendingForceStopIds);
      message.success(result.stopped.length > 0 ? `已强停 ${result.stopped.length} 个环路` : `环路强停功能开发中（已选 ${pendingForceStopIds.length} 个）`);
    } finally { onClearSelection?.(); }
  }, [pendingForceStopIds, message, onClearSelection]);

  const handleConfirmWorkspaceBatch = useCallback(async () => {
    if (pendingWorkspaceBatchIds.length === 0 || workspaceBatchTarget == null) return;
    if (selectedWorkspace == null) { message.warning('请先选择工作空间'); return; }
    setWorkspaceBatchProcessing(true);
    try {
      const isCopy = workspaceBatchMode === 'copy';
      const result = mode === 'item'
        ? (isCopy ? await db.batchCopyTodosWorkspace(selectedWorkspace, pendingWorkspaceBatchIds, workspaceBatchTarget)
          : await db.batchMoveTodosWorkspace(selectedWorkspace, pendingWorkspaceBatchIds, workspaceBatchTarget))
        : (isCopy ? await dbLoops.batchCopyLoopsWorkspace(selectedWorkspace, pendingWorkspaceBatchIds, workspaceBatchTarget)
          : await dbLoops.batchMoveLoopsWorkspace(selectedWorkspace, pendingWorkspaceBatchIds, workspaceBatchTarget));
      reportBatchResult(result, isCopy ? '复制' : '移动', message);
      if (mode === 'item') onRefreshItems?.(); else onRefreshLoops?.();
      setWorkspaceBatchModalOpen(false);
      onClearSelection?.();
    } catch (err) {
      message.error(`操作失败: ${err instanceof Error ? err.message : '未知错误'}`);
    } finally { setWorkspaceBatchProcessing(false); }
  }, [pendingWorkspaceBatchIds, workspaceBatchTarget, workspaceBatchMode, mode, selectedWorkspace, message, onRefreshItems, onRefreshLoops, onClearSelection]);

  const handleConfirmSchedulerBatch = useCallback(async () => {
    if (pendingSchedulerBatchIds.length === 0 || selectedWorkspace == null) {
      if (selectedWorkspace == null) message.warning('请先选择工作空间');
      return;
    }
    setSchedulerBatchProcessing(true);
    try {
      const isPause = schedulerBatchMode === 'pause';
      const result = isPause
        ? await db.batchPauseScheduler(selectedWorkspace, pendingSchedulerBatchIds)
        : await db.batchResumeScheduler(selectedWorkspace, pendingSchedulerBatchIds);
      reportBatchResult(result, isPause ? '暂停' : '恢复', message);
      onRefreshItems?.();
      setSchedulerBatchModalOpen(false);
      onClearSelection?.();
    } catch (err) {
      message.error(`操作失败: ${err instanceof Error ? err.message : '未知错误'}`);
    } finally { setSchedulerBatchProcessing(false); }
  }, [pendingSchedulerBatchIds, schedulerBatchMode, selectedWorkspace, message, onRefreshItems, onClearSelection]);

  // 批量删除确认回调
  const handleConfirmBatchDelete = useCallback(async () => {
    if (pendingDeleteIds.length === 0) return;
    if (selectedWorkspace == null && mode === 'item') { message.warning('请先选择工作空间'); return; }
    setDeleteProcessing(true);
    try {
      if (mode === 'item') {
        const result = await db.batchDeleteTodos(selectedWorkspace!, pendingDeleteIds);
        if (result.errors.length === 0) {
          message.success(`已删除 ${result.deleted} 项`);
        } else {
          message.warning(`删除成功 ${result.deleted} 条，失败 ${result.errors.length} 条`);
        }
        onRefreshItems?.();
      } else {
        // loop 模式：批量删除（需 workspace 隔离，与单删一致走 v1 路径）
        if (selectedWorkspace == null) { message.warning('请先选择工作空间'); return; }
        const result = await dbLoops.batchDeleteLoops(selectedWorkspace, pendingDeleteIds);
        message.success(`已删除 ${result.deleted} 个环路`);
        onRefreshLoops?.();
      }
      setDeleteModalOpen(false);
      onClearSelection?.();
    } catch (err) {
      message.error(`删除失败: ${err instanceof Error ? err.message : '未知错误'}`);
    } finally {
      setDeleteProcessing(false);
      setPendingDeleteIds([]);
    }
  }, [pendingDeleteIds, mode, selectedWorkspace, message, onRefreshItems, onRefreshLoops, onClearSelection]);

  // ─── 顶部批量操作菜单项（渲染方为 TodoListView 的 BatchButton） ──
  const batchActions = useMemo<BatchActionItem<number>[]>(() => {
    if (mode === 'item') {
      return [
        { key: 'change-executor', label: '更换执行器', icon: <SwapOutlined />, onClick: openChangeExecutor },
        { key: 'copy-workspace', label: '复制到', icon: <CopyOutlined />, onClick: openCopyWorkspace },
        { key: 'move-workspace', label: '移动到', icon: <DragOutlined />, onClick: openMoveWorkspace },
        { key: 'pause-scheduler', label: '暂停周期执行', icon: <PauseCircleOutlined />, onClick: openPauseScheduler },
        { key: 'resume-scheduler', label: '恢复周期执行', icon: <PlayCircleOutlined />, onClick: openResumeScheduler },
        { key: 'delete', label: '删除', icon: <DeleteOutlined />, danger: true, onClick: openBatchDelete },
      ];
    }
    return [
      { key: 'copy-workspace', label: '复制到', icon: <CopyOutlined />, onClick: openCopyWorkspace },
      { key: 'move-workspace', label: '移动到', icon: <DragOutlined />, onClick: openMoveWorkspace },
      { key: 'force-stop', label: '强停', icon: <StopOutlined />, danger: true, onClick: openForceStop },
      { key: 'delete', label: '删除', icon: <DeleteOutlined />, danger: true, onClick: openBatchDelete },
    ];
  }, [mode, openChangeExecutor, openCopyWorkspace, openMoveWorkspace, openPauseScheduler, openResumeScheduler, openForceStop, openBatchDelete]);

  // ─── Modal JSX ──────────────────────────────────────────────
  const modals = (
    <>
      {mode === 'item' && (
        <ExecutorChangeModal
          open={executorModalOpen} ids={pendingExecutorChangeIds}
          onConfirm={handleConfirmChangeExecutor}
          onCancel={() => { setExecutorModalOpen(false); setPendingExecutorChangeIds([]); }}
        />
      )}
      {mode === 'loop' && (
        <ForceStopModal
          open={forceStopModalOpen} ids={pendingForceStopIds}
          onConfirm={handleConfirmForceStop}
          onCancel={() => { setForceStopModalOpen(false); setPendingForceStopIds([]); }}
        />
      )}
      <WorkspaceBatchModal
        open={workspaceBatchModalOpen} ids={pendingWorkspaceBatchIds}
        mode={workspaceBatchMode} processing={workspaceBatchProcessing}
        target={workspaceBatchTarget}
        onTargetChange={setWorkspaceBatchTarget}
        onConfirm={handleConfirmWorkspaceBatch}
        onCancel={() => { setWorkspaceBatchModalOpen(false); setPendingWorkspaceBatchIds([]); }}
      />
      {mode === 'item' && (
        <SchedulerBatchModal
          open={schedulerBatchModalOpen} ids={pendingSchedulerBatchIds}
          mode={schedulerBatchMode} processing={schedulerBatchProcessing}
          onConfirm={handleConfirmSchedulerBatch}
          onCancel={() => { setSchedulerBatchModalOpen(false); setPendingSchedulerBatchIds([]); }}
        />
      )}
      <BatchDeleteConfirmModal
        open={deleteModalOpen} ids={pendingDeleteIds}
        processing={deleteProcessing} mode={mode}
        onConfirm={handleConfirmBatchDelete}
        onCancel={() => { setDeleteModalOpen(false); setPendingDeleteIds([]); }}
      />
    </>
  );

  return { batchActions, modals };
}
