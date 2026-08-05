// 讨论帖数据 hook：主楼层分页拉取 + running 帖轮询/WS 实时刷新。
// 从 DiscussionTab 抽出（S2：组件不超 150 行），副作用集中于此，组件只管渲染与输入。

import { useCallback, useEffect, useRef, useState } from 'react';
import bundledApi from '@/api/bundled';
import type { TaskPost } from '@/types';

/** 主楼层每页条数（与后端默认 limit 对齐，足够覆盖常见讨论长度）。 */
export const PAGE_SIZE = 20;
/** running 帖的轮询间隔（ms）。低于 3s 会给后端造成无谓压力。 */
const POLL_INTERVAL_MS = 4000;

/**
 * 管理某任务讨论帖的分页列表与实时刷新。
 * - 挂载/翻页时拉当前页主楼层（含楼中楼 replies）。
 * - 有 running 帖时每 POLL_INTERVAL_MS 轮询当前页（WS 断线兜底）。
 * - 监听全局 executionFinished（WS），按 source_todo_id 精确匹配 running 帖即时刷新。
 * 返回 setPosts/setTotal 供组件做乐观更新（发帖/删帖）。
 */
export function useDiscussionPosts(taskId: number, workspaceId: number) {
  const [posts, setPosts] = useState<TaskPost[]>([]);
  const [page, setPage] = useState(1);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);

  const fetchPosts = useCallback(async () => {
    setLoading(true);
    try {
      const res = await bundledApi.listTaskPosts(workspaceId, taskId, page, PAGE_SIZE);
      setPosts(res.items);
      setTotal(res.total);
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

  // 有 running 帖才轮询（轮询为兜底：WS 断线/丢事件时仍能收敛到终态）。
  const hasRunning = posts.some((p) => p.status === 'running');
  useEffect(() => {
    if (!hasRunning) return;
    const timer = setInterval(() => { void fetchPosts(); }, POLL_INTERVAL_MS);
    return () => clearInterval(timer);
  }, [hasRunning, fetchPosts]);

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

  return { posts, page, total, loading, setPage, setPosts, setTotal };
}
