import { Card, Tag, Typography, Button, Space } from 'antd';
import { EyeOutlined, ReloadOutlined } from '@ant-design/icons';
import type { FeishuHistoryMessage } from '@/types';
import { chatTypeMeta, historyMeta, copyableIdForPush } from './messageMeta';
import { CopyButton } from '@/components/CopyButton';

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
  onViewLoopExecution: () => void;
}

// 判断是否为环路类型
const isLoopType = (type: string | null): boolean =>
  type === 'slash_command_loop' || type === 'default_response_loop';

export function MessageCard({ message, botName, onViewDetail, onViewExecution, onViewLoopExecution }: MessageCardProps) {
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
  // 会话类型(群聊/私聊)与来源时机(历史/实时)只依赖原始字段，提前算好供标签复用。
  const chatMeta = chatTypeMeta(message.chat_type);
  const histMeta = historyMeta(message.is_history);
  // 群聊取 chat_id、私聊取 open_id，作为可复制到推送目标配置的 ID（见 copyableIdForPush 注释）。
  const copyId = copyableIdForPush(message.chat_type, message.chat_id, message.sender_open_id);

  return (
    <Card size="small" style={{ marginBottom: 12, borderRadius: 8 }} hoverable onClick={onViewDetail}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
        <Space size={8}>
          <Text style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>{formatTime(message.created_at)}</Text>
          {botName && (
            <Tag color="blue" style={{ fontSize: 11 }}>🤖 {botName}</Tag>
          )}
          {/* 会话类型 + 来源时机：群聊/私聊 与 历史/实时 是两个正交维度，
              组合起来即可一眼区分「实时群聊」「历史私聊」等，无需点开详情逐条查看。 */}
          <Tag color={chatMeta.color} style={{ fontSize: 11 }}>{chatMeta.label}</Tag>
          <Tag color={histMeta.color} style={{ fontSize: 11 }}>{histMeta.label}</Tag>
          <Tag color={processedTypeColor(message.processed_type)} style={{ fontSize: 11 }}>
            {processedTypeLabel(message.processed_type)}
          </Tag>
          <Tag color={message.processed ? (message.error ? 'volcano' : 'green') : 'orange'} style={{ fontSize: 11 }}>
            {message.processed ? (message.error ? `已处理(${message.error})` : '已处理') : '未处理'}
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
                // 环路类型显示黑板详情，其他类型显示执行记录详情
                if (isLoopType(message.processed_type) && message.processed_id) {
                  onViewLoopExecution();
                } else {
                  onViewExecution(message.execution_record_id!);
                }
              }}
            >
              执行记录
            </Button>
          )}
          {/* 内容复制：改用 CopyButton(内部走 execCommand，比 navigator.clipboard 更可靠，
              不会因非安全上下文/失焦静默失败)，并自带对钩反馈，与下方 ID 复制按钮体验一致。
              外层 span 拦截点击冒泡，避免触发卡片 onClick 打开详情抽屉。 */}
          <span onClick={(e) => e.stopPropagation()}>
            <CopyButton
              type="text"
              size="small"
              text={userContent !== '-' ? userContent : ''}
              children={null}
            />
          </span>
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

      {/* 可复制到推送目标的 ID：群聊=chat_id(贴到「群聊接收 ID」)，私聊=open_id(贴到「单聊接收 ID」)。
          code 块用 flex:0 1 auto 不占满整行，超长 ID 内部省略，确保图标复制按钮紧跟 ID 而非被推到最右。 */}
      {copyId && (
        <div style={{ display: 'flex', alignItems: 'center', gap: 4, marginBottom: 8 }}>
          <Text style={{ fontSize: 11, color: 'var(--color-text-tertiary)', flexShrink: 0 }}>
            {copyId.label}
          </Text>
          <Text
            code
            style={{
              fontSize: 11,
              flex: '0 1 auto',
              minWidth: 0,
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
              padding: '0 6px',
            }}
          >
            {copyId.value}
          </Text>
          {/* children={null} 覆盖 CopyButton 默认的「复制」文字，只保留图标，紧贴 ID 显示。
              外层 span 拦截点击冒泡，否则会触发整张卡片的 onClick(打开消息详情抽屉)。 */}
          <span onClick={(e) => e.stopPropagation()}>
            <CopyButton type="text" size="small" text={copyId.value} children={null} />
          </span>
        </div>
      )}

      {/* 关联 ID 仅在有真实 processed_id 时显示：用三元而非 `processed_id && (...)`，
          因为后者在 processed_id=0 时会把数字 0 当作 React 节点渲染成文本"0"
          (default_response_executor 类消息 processed_id 恰为 0)。 */}
      {message.processed_id ? (
        <div style={{ padding: 8, backgroundColor: 'var(--color-bg-secondary)', borderRadius: 4 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
            <ReloadOutlined style={{ fontSize: 12, color: 'var(--color-primary)' }} />
            <Text style={{ fontSize: 12, color: 'var(--color-primary)' }}>
              关联 #{message.processed_id}
            </Text>
          </div>
        </div>
      ) : null}
    </Card>
  );
}
