import { useCallback, useEffect, useRef, useState } from 'react';
import type { Todo } from '@/types';
import * as db from '@/utils/database';
import { TODO_LIST_REFRESH_EVENT } from '@/constants';

/**
 * 056：按 id 定点查询 Todo 的共享缓存 hook。
 *
 * 背景：全局 `state.todos` 全量桶已删除（决策 2a）。过去「从全量桶 find 一条」
 * 的消费点（运行记录抽屉补标题、纪念板、帖子页等）改为按需 by-id 查询。
 *
 * 设计要点：
 * - 模块级 Map 缓存 + 请求中去重：同一 id 并发请求只发一次 HTTP。
 * - TODO_LIST_REFRESH_EVENT 到达时清空缓存：列表保存/删除后详情数据可能已变。
 * - 返回 (todo | null)：未加载完成或不存在都为 null，调用方自行降级展示。
 */

interface CacheEntry {
  /** 已完成的查询结果；undefined 表示尚未加载 */
  value?: Todo | null;
  /** 进行中的请求 Promise，用于并发去重 */
  inflight?: Promise<Todo | null>;
}

/** key = `${workspaceId}:${todoId}`（todo 按 workspace 隔离，跨 ws 同 id 不共享） */
const cache = new Map<string, CacheEntry>();

/** CodeRabbit#12：缓存代际计数。clear() 时 +1；在途请求写回前比对代际，
 *  代际不同说明结果产生于「清空」之前，属于陈旧数据，丢弃不写回。 */
let cacheGeneration = 0;

function cacheKey(workspaceId: number, todoId: number): string {
  return `${workspaceId}:${todoId}`;
}

/** 查询单条 todo（带缓存与并发去重）。失败按 null 处理——by-id 是增强信息，缺失不应报错。 */
function fetchTodo(workspaceId: number, todoId: number): Promise<Todo | null> {
  const key = cacheKey(workspaceId, todoId);
  const entry = cache.get(key) ?? {};
  if (entry.value !== undefined) return Promise.resolve(entry.value);
  if (entry.inflight) return entry.inflight;

  const gen = cacheGeneration;
  const inflight = db
    .getTodo(workspaceId, todoId)
    .then((todo) => {
      // 代际一致才写回：clear() 之后的晚到响应属于旧世界，不复活旧数据
      if (gen === cacheGeneration) cache.set(key, { value: todo });
      return todo as Todo | null;
    })
    .catch(() => {
      // 失败也缓存 null：避免失败 id 被每个组件反复重试打爆后端；
      // TODO_LIST_REFRESH_EVENT 到达时缓存会整体失效，给后续重试机会。
      if (gen === cacheGeneration) cache.set(key, { value: null });
      return null;
    });
  cache.set(key, { inflight });
  return inflight;
}

// 列表刷新事件到达时清空缓存（保存/删除/批量操作后，旧缓存可能已过期）。
// 模块级注册一次即可——监听器只做失效标记，无组件生命周期负担。
if (typeof window !== 'undefined') {
  window.addEventListener(TODO_LIST_REFRESH_EVENT, () => {
    cacheGeneration += 1;
    cache.clear();
  });
}

/**
 * 按 id 取单条 todo（响应式）。workspaceId/todoId 为 null 时返回 null 且不发请求。
 *
 * 注意：只在「只需要一两条」的场景使用；列表场景请走分页/批量接口，
 * 避免逐条请求形成 N+1。
 */
export function useTodoById(workspaceId: number | null, todoId: number | null): Todo | null {
  const [todo, setTodo] = useState<Todo | null>(null);
  // cancelledRef 防御快速切换造成的竞态：晚返回的请求若发现参数已变，直接丢弃结果
  const requestSeq = useRef(0);

  const load = useCallback(async (wsId: number, id: number, seq: number) => {
    const result = await fetchTodo(wsId, id);
    // 只接受最新一次请求的结果，过期响应直接丢弃
    if (seq === requestSeq.current) {
      setTodo(result);
    }
  }, []);

  useEffect(() => {
    if (workspaceId == null || todoId == null) {
      setTodo(null);
      return;
    }
    // 命中缓存时同步返回，避免闪烁
    const entry = cache.get(cacheKey(workspaceId, todoId));
    if (entry?.value !== undefined) {
      setTodo(entry.value);
    } else {
      const seq = ++requestSeq.current;
      load(workspaceId, todoId, seq);
    }
    // 无论缓存命中与否都订阅刷新事件（评审 I2 修复）：
    // 命中路径之前 early return 不订阅，该 todo 被编辑后组件仍显示旧数据。
    const onRefresh = () => {
      const s = ++requestSeq.current;
      load(workspaceId, todoId, s);
    };
    window.addEventListener(TODO_LIST_REFRESH_EVENT, onRefresh);
    return () => window.removeEventListener(TODO_LIST_REFRESH_EVENT, onRefresh);
  }, [workspaceId, todoId, load]);

  return todo;
}
