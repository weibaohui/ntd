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
  processedFilter: boolean | undefined;
  chatTypeFilter: string | undefined;
  processedTypeFilter: string | undefined;
  searchText: string;
  onSearchChange: (text: string) => void;
  onChatChange: (chatId: string | undefined) => void;
  onHistoryChange: (isHistory: boolean | undefined) => void;
  onProcessedChange: (processed: boolean | undefined) => void;
  onChatTypeChange: (chatType: string | undefined) => void;
  onProcessedTypeChange: (processedType: string | undefined) => void;
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
  processedFilter,
  chatTypeFilter,
  processedTypeFilter,
  searchText,
  onSearchChange,
  onChatChange,
  onHistoryChange,
  onProcessedChange,
  onChatTypeChange,
  onProcessedTypeChange,
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

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
      {/* 筛选区顺序：搜索 → 会话类型 → 消息类型 → 处理状态 → 处理类型 → 具体群聊。
          按「文本 → 消息维度 → 处理维度 → 具体会话」由宽到窄排列，便于从大类逐步缩小范围。 */}
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
          placeholder="会话类型"
          style={{ width: 120 }}
          value={chatTypeFilter ?? 'all'}
          onChange={(v: string) => { onChatTypeChange(v === 'all' ? undefined : v); onPageChange(1, pageSize); }}
          options={[
            { value: 'all', label: '全部会话' },
            { value: 'group', label: '群聊' },
            { value: 'p2p', label: '私聊' },
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

        <Select
          size="small"
          placeholder="处理状态"
          style={{ width: 120 }}
          value={processedFilter === undefined ? 'all' : processedFilter}
          onChange={(v: string | boolean) => { onProcessedChange(v === 'all' ? undefined : (v as boolean)); onPageChange(1, pageSize); }}
          options={[
            { value: 'all', label: '全部状态' },
            { value: true, label: '已处理' },
            { value: false, label: '未处理' },
          ]}
        />

        {/* 处理类型：value 是语义关键字，后端用 processed_type LIKE '%关键字%' 落到具体类型。
            slash→斜杠命令(slash_command/slash_command_loop)、executor→执行器、loop→环路(*_loop)。 */}
        <Select
          size="small"
          placeholder="处理类型"
          style={{ width: 130 }}
          value={processedTypeFilter ?? 'all'}
          onChange={(v: string) => { onProcessedTypeChange(v === 'all' ? undefined : v); onPageChange(1, pageSize); }}
          options={[
            { value: 'all', label: '全部类型' },
            { value: 'slash', label: '斜杠命令' },
            { value: 'executor', label: '执行器' },
            { value: 'loop', label: '环路' },
          ]}
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
