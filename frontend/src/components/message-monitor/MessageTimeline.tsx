import { Input, Select, Empty, Spin, Pagination } from 'antd';
import { SearchOutlined } from '@ant-design/icons';
import type { FeishuHistoryMessage, FeishuHistoryChat } from '@/types';
import type { AgentBot } from '@/utils/database';
import { MessageCard } from './MessageCard';

interface MessageTimelineProps {
  messages: FeishuHistoryMessage[];
  chats: FeishuHistoryChat[];
  bots: AgentBot[];
  loading: boolean;
  total: number;
  page: number;
  pageSize: number;
  selectedChatId: string | undefined;
  isHistory: boolean | undefined;
  searchText: string;
  onSearchChange: (text: string) => void;
  onChatChange: (chatId: string | undefined) => void;
  onHistoryChange: (isHistory: boolean | undefined) => void;
  onPageChange: (page: number, pageSize: number) => void;
  onViewDetail: (message: FeishuHistoryMessage) => void;
  onViewExecution: (recordId: number) => void;
  onViewLoopExecution: (message: FeishuHistoryMessage) => void;
}

export function MessageTimeline({
  messages,
  chats,
  bots,
  loading,
  total,
  page,
  pageSize,
  selectedChatId,
  isHistory,
  searchText,
  onSearchChange,
  onChatChange,
  onHistoryChange,
  onPageChange,
  onViewDetail,
  onViewExecution,
  onViewLoopExecution,
}: MessageTimelineProps) {
  const getBotName = (chatId: string) => {
    const chat = chats.find(c => c.chat_id === chatId);
    if (!chat) return undefined;
    return bots.find(b => b.id === chat.bot_id)?.bot_name;
  };

  const handleCopy = (content: string) => {
    try {
      const parsed = JSON.parse(content);
      navigator.clipboard.writeText(parsed.text || content);
    } catch {
      navigator.clipboard.writeText(content);
    }
  };

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
      <div style={{ display: 'flex', gap: 12, marginBottom: 16, flexWrap: 'wrap', alignItems: 'center' }}>
        <Input
          size="small"
          placeholder="搜索消息内容..."
          prefix={<SearchOutlined />}
          value={searchText}
          onChange={(e) => onSearchChange(e.target.value)}
          style={{ width: 200 }}
        />

        <Select
          size="small"
          placeholder="筛选群聊"
          style={{ width: 150 }}
          value={selectedChatId ?? 'all'}
          onChange={(v: string) => { onChatChange(v === 'all' ? undefined : v); onPageChange(1, pageSize); }}
          options={[
            { value: 'all', label: '全部群聊' },
            ...chats.map(c => ({ value: c.chat_id, label: c.chat_name || c.chat_id })),
          ]}
        />

        <Select
          size="small"
          placeholder="消息类型"
          style={{ width: 120 }}
          value={isHistory === undefined ? 'all' : isHistory}
          onChange={(v: string | boolean) => { onHistoryChange(v === 'all' ? undefined : (v as boolean)); onPageChange(1, pageSize); }}
          options={[
            { value: 'all', label: '全部' },
            { value: true, label: '历史消息' },
            { value: false, label: '实时消息' },
          ]}
        />
      </div>

      <div style={{ flex: 1, overflowY: 'auto', paddingRight: 8 }}>
        {loading ? (
          <div style={{ display: 'flex', justifyContent: 'center', padding: 48 }}>
            <Spin size="large" />
          </div>
        ) : messages.length === 0 ? (
          <Empty description="暂无消息记录" />
        ) : (
          messages.map(message => (
            <MessageCard
              key={message.id}
              message={message}
              botName={getBotName(message.chat_id)}
              onViewDetail={() => onViewDetail(message)}
              onViewExecution={onViewExecution}
              onViewLoopExecution={() => onViewLoopExecution(message)}
              onCopy={() => handleCopy(message.content || '')}
            />
          ))
        )}
      </div>

      {total > 0 && (
        <div style={{ display: 'flex', justifyContent: 'center', marginTop: 16 }}>
          <Pagination
            current={page}
            pageSize={pageSize}
            total={total}
            showSizeChanger
            showQuickJumper
            showTotal={(t) => `共 ${t} 条`}
            onChange={onPageChange}
            size="small"
          />
        </div>
      )}
    </div>
  );
}
