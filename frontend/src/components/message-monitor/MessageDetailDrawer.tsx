import { Drawer, Tag, Typography, Descriptions } from 'antd';
import type { FeishuHistoryMessage } from '@/types';

const { Text } = Typography;

const processedTypeLabel = (type: string | null): string => {
  const map: Record<string, string> = {
    'default_response': '默认响应-Todo',
    'default_response_executor': '默认响应-执行器',
    'default_response_loop': '默认响应-环路',
    'slash_command': '斜杠命令',
    'slash_command_loop': '斜杠命令-环路',
    'feishu_project_bind': '项目绑定-Todo',
  };
  return map[type || ''] || type || '未分类';
};

interface MessageDetailDrawerProps {
  open: boolean;
  message: FeishuHistoryMessage | null;
  onClose: () => void;
}

export function MessageDetailDrawer({ open, message, onClose }: MessageDetailDrawerProps) {
  if (!message) return null;

  const formatTime = (dateStr: string | null) => {
    if (!dateStr) return '-';
    const d = new Date(dateStr);
    return isNaN(d.getTime()) ? dateStr : d.toLocaleString('zh-CN');
  };

  const parseContent = (content: string | null) => {
    if (!content) return '-';
    if (message.msg_type === 'text') {
      try {
        const parsed = JSON.parse(content);
        return parsed.text || content;
      } catch {
        return content;
      }
    }
    return message.msg_type;
  };

  const content = parseContent(message.content);

  return (
    <Drawer
      title="消息详情"
      open={open}
      onClose={onClose}
      width={560}
    >
      <Descriptions size="small" column={1} bordered style={{ marginBottom: 16 }}>
        <Descriptions.Item label="消息ID">{message.message_id}</Descriptions.Item>
        <Descriptions.Item label="群聊ID">{message.chat_id}</Descriptions.Item>
        <Descriptions.Item label="群聊类型">{message.chat_type}</Descriptions.Item>
        <Descriptions.Item label="发送者ID">{message.sender_open_id}</Descriptions.Item>
        <Descriptions.Item label="发送者昵称">{message.sender_nickname || '-'}</Descriptions.Item>
        <Descriptions.Item label="发送者类型">
          <Tag color={message.sender_type === 'app' ? 'blue' : 'green'}>
            {message.sender_type === 'app' ? '智能体' : '用户'}
          </Tag>
        </Descriptions.Item>
        <Descriptions.Item label="消息类型">{message.msg_type}</Descriptions.Item>
        <Descriptions.Item label="是否历史消息">
          <Tag color={message.is_history ? 'orange' : 'cyan'}>
            {message.is_history ? '是' : '否'}
          </Tag>
        </Descriptions.Item>
        <Descriptions.Item label="处理状态">
          <Tag color={message.processed ? 'green' : 'orange'}>
            {message.processed ? '已处理' : '未处理'}
          </Tag>
        </Descriptions.Item>
        <Descriptions.Item label="处理类型">{processedTypeLabel(message.processed_type)}</Descriptions.Item>
        <Descriptions.Item label="处理ID">{message.processed_id || '-'}</Descriptions.Item>
        <Descriptions.Item label="执行记录ID">{message.execution_record_id || '-'}</Descriptions.Item>
        <Descriptions.Item label="工作空间ID">{message.workspace_id || '-'}</Descriptions.Item>
        <Descriptions.Item label="创建时间">{formatTime(message.created_at)}</Descriptions.Item>
      </Descriptions>

      <div style={{ padding: 12, backgroundColor: 'var(--color-bg-secondary)', borderRadius: 8 }}>
        <Text style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>消息内容</Text>
        <p style={{ margin: '8px 0 0', fontSize: 14, lineHeight: 1.8, whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>
          {content}
        </p>
      </div>
    </Drawer>
  );
}
