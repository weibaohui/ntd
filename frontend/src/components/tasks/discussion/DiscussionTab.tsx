// 任务讨论区 Tab：帖子流（主楼层 + 楼中楼）+ 输入器。
// 数据/分页/轮询/SSE 副作用抽到 useDiscussionPosts；本组件只管输入态、发送/删除与渲染。

import { useCallback, useEffect, useState } from 'react';
import { Empty, Spin, Pagination, message } from 'antd';
import bundledApi from '@/api/bundled';
import { useViewState } from '@/hooks/useViewState';
import { DiscussionComposer } from './DiscussionComposer';
import { PostCard } from './PostCard';
import { mergeAppended, removePost } from './utils';
import { useDiscussionPosts, PAGE_SIZE } from './useDiscussionPosts';
import type { TaskPost } from '@/types';

interface DiscussionTabProps {
  taskId: number;
  workspaceId: number;
  /** running 帖数量变化时通知父组件（用于「讨论」Tab 角标）。 */
  onRunningCountChange?: (count: number) => void;
}

export function DiscussionTab({ taskId, workspaceId, onRunningCountChange }: DiscussionTabProps) {
  const { posts, page, total, loading, setPage, setPosts, setTotal } = useDiscussionPosts(taskId, workspaceId);
  const [sending, setSending] = useState(false);
  const [value, setValue] = useState('');
  const [replyTo, setReplyTo] = useState<{ id: number; author: string } | null>(null);

  const { pushUrl } = useViewState();
  // 点击「执行明细」→ 跳转到该执行记录的帖子详情页（事项侧执行对话流），由 PostCard 调用。
  const handleOpenExecution = useCallback(
    (todoId: number, recordId: number) => pushUrl('todos', { id: todoId, recordId }),
    [pushUrl],
  );

  // running 帖数量上报父组件（TaskDetailPanel 用于「讨论」Tab 角标，M4）。
  const runningCount = posts.filter((p) => p.status === 'running').length;
  useEffect(() => {
    onRunningCountChange?.(runningCount);
  }, [runningCount, onRunningCountChange]);

  const handleSend = async () => {
    if (!value.trim()) return;
    setSending(true);
    try {
      const res = await bundledApi.createTaskPost(workspaceId, taskId, value, replyTo?.id ?? null);
      // 乐观并入：主楼层追加末尾、楼中楼挂到对应楼层；total 相应增加。下次轮询/SSE 会与服务端对齐。
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
          workspaceId={workspaceId}
          replyTo={replyTo ? `#${replyTo.id} ${replyTo.author}` : null}
          onCancelReply={() => setReplyTo(null)}
        />
      </div>
    </div>
  );
}
