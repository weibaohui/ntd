// 任务讨论区 Tab：帖子流（人帖 + 智能体帖）+ 输入器。
// - 挂载/发送后拉取该任务全部帖子，前端按 parent_post_id 分组（主楼层 + 楼中楼）。
// - 有 running 智能体帖时每 4s 轮询一次，直到全部落定（success/failed）。
//   完成回写由后端 completion.rs 的 discussion 分支负责，前端只负责刷新展示。

import { useCallback, useEffect, useState } from 'react';
import { Empty, Spin, message } from 'antd';
import bundledApi from '@/api/bundled';
import { DiscussionComposer } from './DiscussionComposer';
import { PostCard } from './PostCard';
import type { TaskPost } from '@/types';

interface DiscussionTabProps {
  taskId: number;
  workspaceId: number;
}

/** running 帖的轮询间隔（ms）。低于 3s 会给后端造成无谓压力。 */
const POLL_INTERVAL_MS = 4000;

export function DiscussionTab({ taskId, workspaceId }: DiscussionTabProps) {
  const [posts, setPosts] = useState<TaskPost[]>([]);
  const [loading, setLoading] = useState(false);
  const [sending, setSending] = useState(false);
  const [value, setValue] = useState('');
  const [replyTo, setReplyTo] = useState<{ id: number; author: string } | null>(null);

  const fetchPosts = useCallback(async () => {
    setLoading(true);
    try {
      const res = await bundledApi.listTaskPosts(workspaceId, taskId);
      setPosts(res.items);
    } catch {
      // Tab 切换频繁，加载失败静默处理，不打扰用户。
    } finally {
      setLoading(false);
    }
  }, [workspaceId, taskId]);

  useEffect(() => {
    void fetchPosts();
  }, [fetchPosts]);

  // 有 running 帖才轮询：避免常驻定时器，落定后自动停止。
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
      // 后端返回人帖 + 可能的智能体占位帖；一次性追加到流末尾。
      const appended = [res.human_post, res.agent_post].filter(Boolean) as TaskPost[];
      setPosts((prev) => [...prev, ...appended]);
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
      // 删主楼层时连同其楼中楼回复一起移除（后端 CASCADE，前端同步过滤）。
      setPosts((prev) => prev.filter((p) => p.id !== id && p.parent_post_id !== id));
    } catch (e) {
      message.error(e instanceof Error ? e.message : '删除失败');
    }
  };

  // 分组：主楼层 + 各自楼中楼。
  const mainPosts = posts.filter((p) => p.parent_post_id === null);
  const repliesOf = (id: number) => posts.filter((p) => p.parent_post_id === id);

  return (
    <div>
      {loading && posts.length === 0 ? (
        <Spin />
      ) : mainPosts.length === 0 ? (
        <Empty description="还没有讨论，发第一条吧" style={{ marginTop: 32 }} />
      ) : (
        <div>
          {mainPosts.map((p) => (
            <PostCard
              key={p.id}
              post={p}
              replies={repliesOf(p.id)}
              onReply={(id, author) => setReplyTo({ id, author })}
              onDelete={handleDelete}
            />
          ))}
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
