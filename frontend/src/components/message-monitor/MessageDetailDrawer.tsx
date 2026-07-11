import { Drawer, Tag, Typography, Descriptions } from 'antd';
import type { FeishuHistoryMessage } from '@/types';
import { chatTypeMeta, historyMeta } from './messageMeta';

const { Text } = Typography;

const processedTypeLabel = (type: string | null): string => {
  const map: Record<string, string> = {
    'default_response': '默认响应-事项',
    'default_response_executor': '默认响应-执行器',
    'default_response_loop': '默认响应-环路',
    'slash_command': '斜杠命令-事项',
    'slash_command_loop': '斜杠命令-环路',
    'feishu_project_bind': '项目绑定-事项',
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
  // 复用列表卡片的标签映射，保证详情与列表的会话类型/来源时机展示一致。
  const chatMeta = chatTypeMeta(message.chat_type);
  const histMeta = historyMeta(message.is_history);

  return (
    <Drawer
      title="消息详情"
      open={open}
      onClose={onClose}
      width={560}
    >
      <Descriptions size="small" column={1} bordered style={{ marginBottom: 16 }}>
        <Descriptions.Item label="消息ID">{message.message_id}</Descriptions.Item>
        {/* 群聊ID 仅在群聊时展示：私聊消息虽也带 chat_id，但那是飞书 1:1 会话内部 ID，
            对配置推送目标无用（私聊推送用发送者 open_id，已在下方「发送者ID」展示）。 */}
        {message.chat_type === 'group' && (
          <Descriptions.Item label="群聊ID">{message.chat_id}</Descriptions.Item>
        )}
        <Descriptions.Item label="会话类型">
          <Tag color={chatMeta.color}>{chatMeta.label}</Tag>
        </Descriptions.Item>
        <Descriptions.Item label="发送者ID">{message.sender_open_id}</Descriptions.Item>
        <Descriptions.Item label="发送者昵称">{message.sender_nickname || '-'}</Descriptions.Item>
        <Descriptions.Item label="发送者类型">
          <Tag color={message.sender_type === 'app' ? 'blue' : 'green'}>
            {message.sender_type === 'app' ? '智能体' : '用户'}
          </Tag>
        </Descriptions.Item>
        <Descriptions.Item label="消息类型">{message.msg_type}</Descriptions.Item>
        <Descriptions.Item label="来源时机">
          <Tag color={histMeta.color}>{histMeta.label}</Tag>
        </Descriptions.Item>
        <Descriptions.Item label="处理状态">
          <Tag color={message.processed ? (message.error ? 'volcano' : 'green') : 'orange'}>
            {message.processed ? (message.error ? `已处理(${message.error})` : '已处理') : '未处理'}
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
