// 讨论帖数据 hook：主楼层分页拉取 + running 帖纯事件驱动实时刷新。
// 从 DiscussionTab 抽出（S2：组件不超 150 行），副作用集中于此，组件只管渲染与输入。

import { useCallback, useEffect, useRef, useState } from 'react';
import bundledApi from '@/api/bundled';
import type { TaskPost } from '@/types';
// 091：WS 重连全量 Sync 信号——替代被移除的 4s 兜底轮询。
import { EXECUTION_SYNC_EVENT } from '@/hooks/useExecutionEvents';

/** 主楼层每页条数（与后端默认 limit 对齐，足够覆盖常见讨论长度）。 */
export const PAGE_SIZE = 20;

/**
 * 管理某任务讨论帖的分页列表与实时刷新。
 * - 挂载/翻页时拉当前页主楼层（含楼中楼 replies）。
 * - 实时刷新改为纯事件驱动（091：移除原 4s 定时轮询）：
 *   · executionFinished（WS）：按 source_todo_id 精确匹配 running 帖，执行结束即时刷新。
 *   · EXECUTION_SYNC_EVENT（WS 重连全量快照）：补刷一次，纠正断线期间漏掉的终态。
 *   · visibilitychange（用户切回标签页）：补刷一次，覆盖后台挂起 WS、重连未就绪的窗口。
 * 返回 setPosts/setTotal 供组件做乐观更新（发帖/删帖）。
 */
export function useDiscussionPosts(taskId: number, workspaceId: number) {
  const [posts, setPosts] = useState<TaskPost[]>([]);
  const [page, setPage] = useState(1);
  const [total, setTotal] = useState(0);
  // 任务级 running 总数（跨页，后端 running_total）：用于 Tab 角标，避免只算当前页。
  const [runningTotal, setRunningTotal] = useState(0);
  const [loading, setLoading] = useState(false);

  const fetchPosts = useCallback(async () => {
    setLoading(true);
    try {
      const res = await bundledApi.listTaskPosts(workspaceId, taskId, page, PAGE_SIZE);
      setPosts(res.items);
      setTotal(res.total);
      setRunningTotal(res.running_total);
    } catch (e) {
      // Tab 切换频繁，不打扰用户（不弹 toast），但记 console 便于排查。
      console.warn('讨论帖加载失败', e);
    } finally {
      setLoading(false);
    }
  }, [workspaceId, taskId, page]);

  // 挂载/翻页/切任务后拉当前页。
  useEffect(() => {
    void fetchPosts();
  }, [fetchPosts]);
  // 切换任务时回到第 1 页，避免停留在不存在的页码上。
  useEffect(() => {
    setPage(1);
  }, [taskId]);

  // 091：实时刷新改为纯事件驱动（移除原 4s 定时轮询）。
  // - EXECUTION_SYNC_EVENT：WS 重连后端推全量快照——补刷当前页，纠正断线期间漏掉的终态。
  // - visibilitychange：用户切回标签页时刷新一次（浏览器后台会节流/挂起 WS，
  //   切回瞬间重连可能尚未完成，用可见性事件做单次纠偏，非定时轮询）。
  useEffect(() => {
    const onResync = () => { void fetchPosts(); };
    const onVisibility = () => {
      if (document.visibilityState === 'visible') void fetchPosts();
    };
    window.addEventListener(EXECUTION_SYNC_EVENT, onResync);
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      window.removeEventListener(EXECUTION_SYNC_EVENT, onResync);
      document.removeEventListener('visibilitychange', onVisibility);
    };
  }, [fetchPosts]);

  // 实时更新（M4）：监听全局 executionFinished（useExecutionEvents 的 WS 广播），
  // 按 source_todo_id（载体 todo id）精确匹配 running 帖；命中即立即刷新当前页，
  // 拿到 completion.rs 回写的结论，无需等下一轮轮询。
  const runningTodoIdsRef = useRef<Set<number>>(new Set());
  runningTodoIdsRef.current = new Set(
    posts
      .filter((p) => p.status === 'running' && p.source_todo_id != null)
      .map((p) => p.source_todo_id as number),
  );
  useEffect(() => {
    const onFinished = (e: Event) => {
      const detail = (e as CustomEvent).detail as { todoId: number };
      // 只在该任务的某条 running 帖对应的执行完成时刷新，避免无关执行触发多余拉取。
      if (runningTodoIdsRef.current.has(detail.todoId)) {
        void fetchPosts();
      }
    };
    window.addEventListener('executionFinished', onFinished);
    return () => window.removeEventListener('executionFinished', onFinished);
  }, [fetchPosts]);

  return { posts, page, total, runningTotal, loading, setPage, setPosts, setTotal };
}
