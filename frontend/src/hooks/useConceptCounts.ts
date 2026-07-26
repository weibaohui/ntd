// 拉取 6 个概念当前数量 + 快速开始 5 步完成状态。
// 并行请求（Promise.all）降低首屏延迟。
// 任一 API 失败对应字段设为 null，不阻塞页面。

import { useState, useEffect, useCallback } from 'react';
import bundledApi from '@/api/bundled';
import * as db from '@/utils/database';
import * as dbLoops from '@/utils/database/loops';
import type { ConceptNode } from '@/components/onboarding/concepts';

/** 6 个概念当前数量映射，null 表示拉取失败/未拉取。 */
export type ConceptCounts = Record<ConceptNode['id'], number | null>;

/** 快速开始 5 步完成状态，按序号 1-5 索引。 */
export type QuickStartStatus = Record<number, boolean | null>;

interface UseConceptCountsResult {
  counts: ConceptCounts | null;
  quickStart: QuickStartStatus | null;
  loading: boolean;
}

/**
 * 拉取概念数量 + 快速开始完成状态。
 *
 * 并行请求 6 个 API：
 *   bundledApi.getProcesses()      → 工艺数量
 *   dbLoops.listLoops(wsId)        → 环路数量 + 触发器判断（步骤 2）
 *   db.getAllTodos(wsId)           → 事项数量
 *   bundledApi.listTasks(wsId)     → 任务数量（步骤 3）
 *   db.getExecutors()              → 执行器数量
 *   db.getAllExperts()             → 专家数量
 *
 * 快速开始完成判断（简化，只取首个判断，足够给新用户「是否做过」信号）：
 *   步骤 1（安装工艺）：processes 非空
 *   步骤 2（配置触发器）：任一 loop 有非 manual 触发器（getLoop 详情含 triggers）
 *   步骤 3（创建任务）：tasks 非空
 *   步骤 4（监控执行）：db.getExecutionRecords 非空
 *   步骤 5（验收产物）：暂用步骤 4 同源判断（完整实现需扫审计 API，YAGNI 阶段先简化）
 */
export function useConceptCounts(workspaceId: number | null): UseConceptCountsResult {
  const [counts, setCounts] = useState<ConceptCounts | null>(null);
  const [quickStart, setQuickStart] = useState<QuickStartStatus | null>(null);
  const [loading, setLoading] = useState(false);

  // null workspace 回退到 1（开发环境默认空间），与 TasksPage 一致。
  const wsId = workspaceId ?? 1;

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      // 并行拉取 6 个 API，任一失败对应字段 null。
      // 不用 Promise.allSettled：失败字段在 catch 里置 null，不影响其他。
      const processes = await bundledApi.getProcesses().catch(() => null);
      const loops = await dbLoops.listLoops(wsId).catch(() => null);
      const todos = await db.getAllTodos(wsId).catch(() => null);
      const tasks = await bundledApi.listTasks(wsId).catch(() => null);
      const executors = await db.getExecutors().catch(() => null);
      const experts = await db.getAllExperts().catch(() => null);

      // 数量映射：null 表示拉取失败，徽标不显示。
      const nextCounts: ConceptCounts = {
        process: processes ? processes.length : null,
        loop: loops ? loops.length : null,
        todo: todos ? todos.length : null,
        task: tasks ? tasks.length : null,
        executor: executors ? executors.length : null,
        expert: experts ? experts.length : null,
      };
      setCounts(nextCounts);

      // 快速开始完成判断。
      // 步骤 1：工艺非空。
      const step1Done = processes ? processes.length > 0 : false;
      // 步骤 2：任一 loop 有非 manual 触发器。
      // 简化：只查首个 loop 的详情（含 triggers），避免 N+1 请求。
      const step2Done = await checkTriggersConfigured(loops, wsId);
      // 步骤 3：任务非空。
      const step3Done = tasks ? tasks.length > 0 : false;
      // 步骤 4：有执行记录。
      // 简化：用首个 todo 查执行记录，避免 N+1。
      const step4Done = await checkExecutionsExist(todos, wsId);
      // 步骤 5：有产物。
      // 简化：与步骤 4 同源判断（完整实现需扫审计 API，YAGNI 先简化）。
      const step5Done = step4Done;

      setQuickStart({
        1: step1Done,
        2: step2Done,
        3: step3Done,
        4: step4Done,
        5: step5Done,
      });
    } finally {
      setLoading(false);
    }
  }, [wsId]);

  // workspace 变化时重拉。
  useEffect(() => {
    reload();
  }, [reload]);

  return { counts, quickStart, loading };
}

/**
 * 检查任一 loop 有非 manual 触发器（步骤 2）。
 *
 * 简化：只查首个 loop 的详情（getLoop 返回含 triggers 列表）。
 * 完整实现应扫所有 loop，但首屏性能优先，且对新用户「是否配置过触发器」的信号足够。
 */
async function checkTriggersConfigured(
  loops: Array<{ id: number }> | null,
  wsId: number,
): Promise<boolean> {
  if (!loops || loops.length === 0) return false;
  try {
    // getLoop 返回 LoopDetail，含 triggers 数组。
    const detail = await dbLoops.getLoop(wsId, loops[0].id);
    const triggers = detail.triggers ?? [];
    return triggers.some((t) => t.trigger_type !== 'manual');
  } catch {
    return false;
  }
}

/**
 * 检查有执行记录（步骤 4）。
 *
 * 简化：只查首个 todo 的执行记录分页 total。
 */
async function checkExecutionsExist(
  todos: Array<{ id: number }> | null,
  wsId: number,
): Promise<boolean> {
  if (!todos || todos.length === 0) return false;
  try {
    const page = await db.getExecutionRecords(todos[0].id, 1, 1, undefined, undefined, wsId);
    return page.total > 0;
  } catch {
    return false;
  }
}
