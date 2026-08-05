// 任务讨论区 Tab：帖子流（主楼层 + 楼中楼）+ 输入器。
// - 主楼层服务端分页：每条主楼层由后端附带 replies，前端直接渲染树、不再自行分组。
// - 有 running 智能体帖时每 4s 轮询当前页，直到落定（success/failed）。
//   完成回写由后端 completion.rs 的 discussion 分支负责，前端只刷新展示。

import { useCallback, useEffect, useState } from 'react';
import { Empty, Spin, Pagination, message } from 'antd';
import bundledApi from '@/api/bundled';
import { useViewState } from '@/hooks/useViewState';
import { DiscussionComposer } from './DiscussionComposer';
import { PostCard } from './PostCard';
import type { TaskPost } from '@/types';

interface DiscussionTabProps {
  taskId: number;
  workspaceId: number;
}

/** 主楼层每页条数（与后端默认 limit 对齐，足够覆盖常见讨论长度）。 */
const PAGE_SIZE = 20;
/** running 帖的轮询间隔（ms）。低于 3s 会给后端造成无谓压力。 */
const POLL_INTERVAL_MS = 4000;

/**
 * 把刚发出的帖子并入当前列表：主楼层追加到末尾，楼中楼挂到对应主楼层 replies。
 * 纯函数（乐观更新用）：避免发帖后必须翻页/刷新才看到刚发的内容。
 */
function mergeAppended(posts: TaskPost[], appended: TaskPost[]): TaskPost[] {
  const mains = appended.filter((p) => p.parent_post_id === null);
  const replies = appended.filter((p) => p.parent_post_id !== null);
  let next = posts;
  if (replies.length) {
    // 楼中楼挂到目标主楼层（按 parent 匹配）；找不到目标则丢弃（刚回复的楼层必在当前页）。
    next = next.map((p) => {
      const mine = replies.filter((r) => r.parent_post_id === p.id);
      // 只在确有挂载时产生新对象，减少无谓渲染。
      return mine.length ? { ...p, replies: [...(p.replies ?? []), ...mine] } : p;
    });
  }
  // 主楼层（含 @ 触发的 agent 占位帖）追加到末尾，契合 id ASC 的时间顺序。
  return mains.length ? [...next, ...mains] : next;
}

/** 从列表移除一条帖子：主楼层整条剔除（含其楼中楼），楼中楼则在对应楼层 replies 内过滤。 */
function removePost(posts: TaskPost[], id: number): TaskPost[] {
  if (posts.some((p) => p.id === id)) {
    // 命中主楼层：整条移除（其 replies 随之消失，与后端 CASCADE 一致）。
    return posts.filter((p) => p.id !== id);
  }
  // 否则是楼中楼：在每条主楼层的 replies 里过滤掉它。
  return posts.map((p) => ({ ...p, replies: p.replies?.filter((r) => r.id !== id) }));
}

export function DiscussionTab({ taskId, workspaceId }: DiscussionTabProps) {
  const [posts, setPosts] = useState<TaskPost[]>([]);
  const [page, setPage] = useState(1);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [sending, setSending] = useState(false);
  const [value, setValue] = useState('');
  const [replyTo, setReplyTo] = useState<{ id: number; author: string } | null>(null);

  const { pushUrl } = useViewState();
  // 点击「执行明细」→ 跳转到该执行记录的帖子详情页（事项侧执行对话流），由 PostCard 调用。
  const handleOpenExecution = useCallback(
    (todoId: number, recordId: number) => pushUrl('todos', { id: todoId, recordId }),
    [pushUrl],
  );

  const fetchPosts = useCallback(async () => {
    setLoading(true);
    try {
      const res = await bundledApi.listTaskPosts(workspaceId, taskId, page, PAGE_SIZE);
      setPosts(res.items);
      setTotal(res.total);
    } catch {
      // Tab 切换频繁，加载失败静默处理，不打扰用户。
    } finally {
      setLoading(false);
    }
  }, [workspaceId, taskId, page]);

  useEffect(() => {
    void fetchPosts();
  }, [fetchPosts]);

  // 切换任务时回到第 1 页，避免停留在不存在的页码上。
  useEffect(() => {
    setPage(1);
  }, [taskId]);

  // 有 running 帖才轮询当前页：避免常驻定时器，落定后自动停止。
  // agent 占位帖都是主楼层，故只查顶层 posts 的 running 状态即可。
  const hasRunning = posts.some((p) => p.status === 'running');
  useEffect(() => {
    if (!hasRunning) return;
    const timer = setInterval(() => { void fetchPosts(); }, POLL_INTERVAL_MS);
    return () => clearInterval(timer);
  }, [hasRunning, fetchPosts]);

  const handleSend = async () => {
    if (!value.trim()) return;
    setSending(true);
    try {
      const res = await bundledApi.createTaskPost(workspaceId, taskId, value, replyTo?.id ?? null);
      // 乐观并入：主楼层追加末尾、楼中楼挂到对应楼层；total 相应增加。下次轮询/翻页会与服务端对齐。
      const appended = [res.human_post, res.agent_post].filter(Boolean) as TaskPost[];
      setPosts((prev) => mergeAppended(prev, appended));
      setTotal((t) => t + appended.length);
      setValue('');
      setReplyTo(null);
    } catch (e) {
      message.error(e instanceof Error ? e.message : '发送失败');
    } finally {
      setSending(false);
    }
  };

  const handleDelete = async (id: number) => {
    try {
      await bundledApi.deleteTaskPost(workspaceId, taskId, id);
      // 删主楼层连同其楼中楼（后端 CASCADE）；删楼中楼则只移除该回复。total 同步减 1。
      setPosts((prev) => removePost(prev, id));
      setTotal((t) => Math.max(0, t - 1));
    } catch (e) {
      message.error(e instanceof Error ? e.message : '删除失败');
    }
  };

  return (
    <div>
      {loading && posts.length === 0 ? (
        <Spin />
      ) : posts.length === 0 ? (
        <Empty description="还没有讨论，发第一条吧" style={{ marginTop: 32 }} />
      ) : (
        <div>
          {posts.map((p) => (
            <PostCard
              key={p.id}
              post={p}
              replies={p.replies ?? []}
              onReply={(id, author) => setReplyTo({ id, author })}
              onDelete={handleDelete}
              onOpenExecution={handleOpenExecution}
            />
          ))}
          <Pagination
            current={page}
            pageSize={PAGE_SIZE}
            total={total}
            onChange={(p) => setPage(p)}
            size="small"
            showSizeChanger={false}
            style={{ marginTop: 12, textAlign: 'right' }}
          />
        </div>
      )}

      <div style={{ marginTop: 12 }}>
        <DiscussionComposer
          value={value}
          onChange={setValue}
          onSend={handleSend}
          sending={sending}
          replyTo={replyTo ? `#${replyTo.id} ${replyTo.author}` : null}
          onCancelReply={() => setReplyTo(null)}
        />
      </div>
    </div>
  );
}
