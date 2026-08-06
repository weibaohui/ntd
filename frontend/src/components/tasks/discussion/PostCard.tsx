// 讨论帖卡片：渲染一条帖子（人帖 / 智能体帖）+ 其楼中楼回复。
// - 人帖：作者 + Markdown 正文 + 回复/删除。
// - 智能体帖：执行器/专家徽标 + 状态 Tag + 结论（running 时显示「正在干活…」+ Spin）。

import { Card, Tag, Space, Button, Spin, Typography, Popconfirm, theme } from 'antd';
import { DeleteOutlined, MessageOutlined } from '@ant-design/icons';
import XMarkdown from '@ant-design/x-markdown';
import type { TaskPost } from '@/types';

const { Text } = Typography;

interface PostCardProps {
  post: TaskPost;
  replies: TaskPost[];
  onReply: (id: number, author: string) => void;
  onDelete: (id: number) => void;
  /** 点击「执行明细」跳转：由父组件用路由 pushUrl 注入，保持本组件与路由解耦。 */
  onOpenExecution?: (todoId: number, recordId: number) => void;
}

/** 智能体帖状态 → Tag。人帖（sent）不显示状态 Tag。 */
function statusTag(status: TaskPost['status']) {
  switch (status) {
    case 'running': return <Tag color="processing">执行中</Tag>;
    case 'success': return <Tag color="success">成功</Tag>;
    case 'failed': return <Tag color="error">失败</Tag>;
    default: return null;
  }
}

export function PostCard({ post, replies, onReply, onDelete, onOpenExecution }: PostCardProps) {
  const { token } = theme.useToken();
  const isAgent = post.kind === 'agent';
  const isMain = post.parent_post_id === null;

  return (
    <Card size="small" style={{ marginBottom: 8 }}>
      <Space style={{ width: '100%', justifyContent: 'space-between', flexWrap: 'wrap' }}>
        <Space size={6} wrap>
          <Text strong>{post.author_name}</Text>
          {isAgent ? <Tag color="purple">智能体</Tag> : null}
          {post.expert_name ? <Tag color="geekblue">专家·{post.expert_name}</Tag> : null}
          {post.executor ? <Tag color="blue">{post.executor}</Tag> : null}
          {statusTag(post.status)}
          {post.created_at ? (
            <Text type="secondary" style={{ fontSize: 12 }}>{post.created_at}</Text>
          ) : null}
        </Space>
        <Space size={4}>
          {/* 主楼层（人帖或智能体帖）均可被回复（楼中楼深度 ≤1）。 */}
          {isMain ? (
            <Button
              size="small"
              type="text"
              icon={<MessageOutlined />}
              onClick={() => onReply(post.id, post.author_name)}
            >
              回复
            </Button>
          ) : null}
          <Popconfirm title="删除该帖？" onConfirm={() => onDelete(post.id)}>
            <Button size="small" type="text" danger icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      </Space>

      <div style={{ marginTop: 8 }}>
        {post.status === 'running' ? (
          // 占位帖：执行中，正文是「X 正在干活…」。
          <Space>
            <Spin size="small" />
            <Text type="secondary">{post.content}</Text>
          </Space>
        ) : (
          <XMarkdown content={post.content || '（空）'} />
        )}
        {post.source_execution_id != null && post.source_todo_id != null ? (
          // 跳转到该执行记录的帖子详情页（/#/todos/:载体todoId/posts/:recordId），
          // 复用事项侧执行对话流页查看完整输出/日志。载体 todo 已隐藏软删，不影响按 recordId 加载。
          <div>
            <Button
              size="small"
              type="link"
              style={{ padding: 0, height: 'auto', fontSize: 12 }}
              onClick={() => onOpenExecution?.(post.source_todo_id!, post.source_execution_id!)}
            >
              执行明细 #{post.source_execution_id}
            </Button>
          </div>
        ) : null}
      </div>

      {/* 楼中楼：回复挂在主楼层下，缩进 + 左边框区隔。深度 ≤1，回复不再有回复。 */}
      {replies.length > 0 ? (
        <div style={{ marginTop: 8, paddingLeft: 12, borderLeft: `2px solid ${token.colorBorderSecondary}` }}>
          {replies.map((r) => (
            <PostCard
              key={r.id}
              post={r}
              replies={[]}
              onReply={() => undefined}
              onDelete={onDelete}
              onOpenExecution={onOpenExecution}
            />
          ))}
        </div>
      ) : null}
    </Card>
  );
}
