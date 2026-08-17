/**
 * useRunningRecords — 「正在运行」标签页的运行监控数据族 hook（096-W4-4-3 产物）。
 *
 * 承接原 ExecutorsPanel 主组件的运行监控整块：
 * - 面板级 Tab 当前页（runningTab，控制执行器/API Key/正在运行/会话四个页签）
 * - 运行记录族（runningRecords / recordTodos / selectedRecordIds / stoppingRecords）
 * - loader（loadRunningRecords：拉记录 + 按 todo_id 集反查 brief 标题）
 * - 批量停止（handleBatchStop）与执行器名→展示名映射（executorDisplayNames）
 * - 切到「正在运行」时初始加载 + 事件驱动刷新（useAutoRefreshRunningBoard 订阅）
 *
 * 设计取舍：
 * - runningTab 保留原联合类型 `'executors' | 'running' | 'sessions'`（原实现即如此，
 *   'api-key' 经 onChange 的 cast 写入），逐字保留以零行为风险。
 * - recordTodos 的反查用「按 id 集轻量拉 brief」（056），替代全量桶，失败时保留旧映射避免闪空。
 */

import { useCallback, useEffect, useMemo, useState } from 'react';
import { App } from 'antd';
import type { ExecutionRecord, ExecutorConfig, TodoBrief } from '@/types';
import * as db from '@/utils/database';
// 093：本 hook 只消费 todo 域的 selectedWorkspace（用于运行记录的 workspace 归属查询）。
import { useTodos } from '@/hooks/useTodoContext';
// 091：执行态变化（Started/Finished/ReviewStatusChanged）+ WS 重连 + 切回标签页 → 事件驱动刷新。
import { useAutoRefreshRunningBoard } from '@/hooks/useRunningBoard';

/** 面板 Tab 当前页（原联合类型逐字保留；'api-key' 经 setRunningTab 的 cast 写入）。 */
type ExecutorPanelTab = 'executors' | 'running' | 'sessions';

export interface UseRunningRecordsReturn {
  /** 面板当前 Tab。 */
  runningTab: ExecutorPanelTab;
  setRunningTab: React.Dispatch<React.SetStateAction<ExecutorPanelTab>>;
  /** 运行中的执行记录。 */
  runningRecords: ExecutionRecord[];
  /** 记录的 todo 标题反查结果（按 todo_id 集，056 轻量反查）。 */
  recordTodos: TodoBrief[];
  /** 运行记录表勾选的记录 id（批量停止用）。 */
  selectedRecordIds: number[];
  setSelectedRecordIds: React.Dispatch<React.SetStateAction<number[]>>;
  /** 批量停止进行中。 */
  stoppingRecords: boolean;
  /** 加载运行记录（切到 running tab 的初始加载 + 事件驱动刷新都调它）。 */
  loadRunningRecords: () => Promise<void>;
  /** 批量停止勾选的记录。 */
  handleBatchStop: () => Promise<void>;
  /** 单条停止（运行记录表操作列的 Popconfirm 用）。 */
  stopRecord: (recordId: number) => Promise<void>;
  /** 执行器 name → display_name 映射（运行记录表的执行器列展示用）。 */
  executorDisplayNames: Record<string, string>;
}

/**
 * @param executors 执行器列表（用于派生 executorDisplayNames；来自 useExecutorAdmin）。
 */
export function useRunningRecords(executors: ExecutorConfig[]): UseRunningRecordsReturn {
  const { message } = App.useApp();
  // 只取 selectedWorkspace：运行记录按 workspace 归属查询（093 细粒度订阅，避免执行态变化触发本组件多余重渲染）。
  const { state } = useTodos();

  // 面板 Tab 默认在「执行器」页。
  const [runningTab, setRunningTab] = useState<ExecutorPanelTab>('executors');
  const [runningRecords, setRunningRecords] = useState<ExecutionRecord[]>([]);
  const [recordTodos, setRecordTodos] = useState<TodoBrief[]>([]);
  const [selectedRecordIds, setSelectedRecordIds] = useState<number[]>([]);
  const [stoppingRecords, setStoppingRecords] = useState(false);

  /**
   * 加载运行中记录 + 按 todo_id 集反查标题。
   * brief 反查失败时保留旧 recordTodos（catch → null → 不 setRecordTodos），避免列表标题闪空。
   */
  const loadRunningRecords = useCallback(async () => {
    try {
      const records = await db.getRunningExecutionRecords(state.selectedWorkspace ?? 0);
      setRunningRecords(records);
      // 056：按记录的 todo_id 集合拉 brief 反查标题；失败时保留旧映射避免闪空。
      const todoIds = [...new Set(records.map((r) => r.todo_id))];
      if (todoIds.length > 0 && state.selectedWorkspace != null) {
        const briefs = await db
          .getTodoBriefs(state.selectedWorkspace, { ids: todoIds })
          .catch(() => null);
        if (briefs) setRecordTodos(briefs);
      } else {
        // 无记录或无 workspace：清空标题反查，避免残留旧数据。
        setRecordTodos([]);
      }
    } catch (err) {
      // 记录加载失败仅打日志（运行 tab 非首屏，不打扰用户）。
      console.error('加载运行中任务失败:', err);
    }
  }, [state.selectedWorkspace]);

  /**
   * 批量停止勾选的运行记录。
   * 用 allSettled 逐条尝试：部分失败不影响其余；成功/失败计数分别提示，最后清选 + 重拉。
   */
  const handleBatchStop = useCallback(async () => {
    // 未勾选直接返回（按钮本应 disabled，双保险）。
    if (selectedRecordIds.length === 0) return;
    setStoppingRecords(true);
    const results = await Promise.allSettled(
      selectedRecordIds.map(async (recordId) => {
        await db.forceFailExecution(state.selectedWorkspace ?? 0, recordId);
      }),
    );
    const successCount = results.filter((r) => r.status === 'fulfilled').length;
    const failCount = results.filter((r) => r.status === 'rejected').length;
    setSelectedRecordIds([]);
    setStoppingRecords(false);
    if (successCount > 0) message.success(`已停止 ${successCount} 个任务`);
    if (failCount > 0) message.error(`${failCount} 个任务停止失败`);
    // 重拉以反映最新停止结果（Finished 事件也会触发刷新，这里主动一次保证即时）。
    loadRunningRecords();
  }, [selectedRecordIds, state.selectedWorkspace, message, loadRunningRecords]);

  /** 单条停止：调 forceFail 后提示并重拉（运行记录表操作列的 Popconfirm 用）。 */
  const stopRecord = useCallback(
    async (recordId: number) => {
      try {
        await db.forceFailExecution(state.selectedWorkspace ?? 0, recordId);
        message.success('已停止');
        loadRunningRecords();
      } catch (err) {
        message.error(`停止失败: ${err instanceof Error ? err.message : String(err)}`);
      }
    },
    [state.selectedWorkspace, message, loadRunningRecords],
  );

  // 091：切到「正在运行」tab 时做一次初始加载；执行态变化全由事件驱动刷新
  // （useAutoRefreshRunningBoard 订阅 Started/Finished/ReviewStatusChanged + WS 重连 + 切回标签页）。
  // 彻底移除原 60s 兜底轮询，无变化时不打请求。deps 仅 runningTab（逐字保留原实现）。
  useEffect(() => {
    if (runningTab !== 'running') return;
    loadRunningRecords();
  }, [runningTab]); // eslint-disable-line react-hooks/exhaustive-deps

  // 始终订阅事件驱动刷新（refresh 内部轻量）；loadRunningRecords 引用变化时重新挂订阅。
  useAutoRefreshRunningBoard(loadRunningRecords);

  // 执行器 display_name 映射（运行记录表的执行器列用 name 反查展示名）。
  const executorDisplayNames = useMemo(() => {
    const map: Record<string, string> = {};
    for (const ec of executors) {
      map[ec.name] = ec.display_name;
    }
    return map;
  }, [executors]);

  return {
    runningTab,
    setRunningTab,
    runningRecords,
    recordTodos,
    selectedRecordIds,
    setSelectedRecordIds,
    stoppingRecords,
    loadRunningRecords,
    handleBatchStop,
    stopRecord,
    executorDisplayNames,
  };
}
