import { Card, Tag, Typography, Button, Space } from 'antd';
import { EyeOutlined, CopyOutlined, ReloadOutlined } from '@ant-design/icons';
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

const processedTypeColor = (type: string | null): string => {
  const map: Record<string, string> = {
    'default_response': 'default',
    'default_response_executor': 'purple',
    'default_response_loop': 'cyan',
    'slash_command': 'blue',
    'slash_command_loop': 'orange',
    'feishu_project_bind': 'green',
  };
  return map[type || ''] || 'default';
};

interface MessageCardProps {
  message: FeishuHistoryMessage;
  botName?: string;
  onViewDetail: () => void;
  onViewExecution: (recordId: number) => void;
  onCopy: () => void;
}

export function MessageCard({ message, botName, onViewDetail, onViewExecution, onCopy }: MessageCardProps) {
  const formatTime = (dateStr: string | null) => {
    if (!dateStr) return '-';
    const d = new Date(dateStr);
    return isNaN(d.getTime()) ? dateStr : d.toLocaleString('zh-CN', {
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    });
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

  const userContent = parseContent(message.content);
  const isUser = message.sender_type !== 'app';

  return (
    <Card size="small" style={{ marginBottom: 12, borderRadius: 8 }} hoverable onClick={onViewDetail}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
        <Space size={8}>
          <Text style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>{formatTime(message.created_at)}</Text>
          {botName && (
            <Tag color="blue" style={{ fontSize: 11 }}>🤖 {botName}</Tag>
          )}
          <Tag color={processedTypeColor(message.processed_type)} style={{ fontSize: 11 }}>
            {processedTypeLabel(message.processed_type)}
          </Tag>
          <Tag color={message.processed ? 'green' : (message.error ? 'volcano' : 'orange')} style={{ fontSize: 11 }}>
            {message.processed ? '已处理' : (message.error === 'loop_paused' ? '环路暂停' : '未处理')}
          </Tag>
        </Space>
        <Space>
          {message.execution_record_id && (
            <Button
              type="text"
              size="small"
              icon={<EyeOutlined />}
              onClick={(e) => {
                e.stopPropagation();
                onViewExecution(message.execution_record_id!);
              }}
            >
              执行记录
            </Button>
          )}
          <Button
            type="text"
            size="small"
            icon={<CopyOutlined />}
            onClick={(e) => {
              e.stopPropagation();
              onCopy();
            }}
          >
            复制
          </Button>
        </Space>
      </div>

      <div style={{ marginBottom: 8 }}>
        <div style={{ display: 'flex', alignItems: 'flex-start', gap: 8 }}>
          <Tag color={isUser ? 'green' : 'blue'} style={{ fontSize: 11, marginTop: 2 }}>
            {isUser ? '用户' : '智能体'}
          </Tag>
          <div>
            <Text style={{ fontSize: 13 }}>{message.sender_nickname || message.sender_open_id?.slice(0, 8) || '-'}</Text>
          </div>
        </div>
        <p style={{ margin: '4px 0 0', fontSize: 14, lineHeight: 1.6, color: 'var(--color-text-primary)' }}>
          {userContent.length > 200 ? userContent.slice(0, 200) + '...' : userContent}
        </p>
      </div>

      {message.processed_id && (
        <div style={{ padding: 8, backgroundColor: 'var(--color-bg-secondary)', borderRadius: 4 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
            <ReloadOutlined style={{ fontSize: 12, color: 'var(--color-primary)' }} />
            <Text style={{ fontSize: 12, color: 'var(--color-primary)' }}>
              关联 #{message.processed_id}
            </Text>
          </div>
        </div>
      )}
    </Card>
  );
}
