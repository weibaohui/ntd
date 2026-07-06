/**
 * ChatMessageList — Wiki 对话消息列表组件。
 *
 * 渲染消息列表，支持自动滚动到底部、加载骨架屏。
 */

import { useRef, useEffect } from 'react';
import { Skeleton } from 'antd';
import { ChatMessageItem, ChatEmptyPlaceholder, ChatMessage, getChatColors } from './ChatMessageItem';

interface ChatMessageListProps {
  /** 消息列表 */
  messages: ChatMessage[];
  /** 是否正在加载 */
  loading: boolean;
  /** 是否移动端布局 */
  mobile?: boolean;
  /** 是否暗色主题 */
  isDark: boolean;
}

/** 消息列表组件：自动滚动到底部、空状态、加载骨架屏 */
export function ChatMessageList({ messages, loading, mobile = false, isDark }: ChatMessageListProps) {
  const listRef = useRef<HTMLDivElement>(null);
  const colors = getChatColors(isDark);

  // 消息更新时自动滚动到底部
  useEffect(() => {
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [messages, loading]);

  return (
    <div
      ref={listRef}
      style={{
        flex: 1,
        overflowY: 'auto',
        padding: mobile ? '12px 14px' : '16px',
        display: 'flex',
        flexDirection: 'column',
        gap: mobile ? 14 : 12,
        minHeight: 0,
      }}
    >
      {messages.length === 0 && !loading && (
        <ChatEmptyPlaceholder mobile={mobile} isDark={isDark} />
      )}
      {messages.map((msg) => (
        <ChatMessageItem key={msg.id} message={msg} mobile={mobile} isDark={isDark} />
      ))}
      {loading && (
        <div style={{ display: 'flex', justifyContent: 'flex-start' }}>
          <div
            style={{
              padding: mobile ? '12px 16px' : '10px 14px',
              borderRadius: 12,
              background: isDark ? '#2a2a2a' : '#fff',
              border: `1px solid ${colors.panelBorder}`,
            }}
          >
            <Skeleton.Input active size={mobile ? 'default' : 'small'} style={{ width: 140 }} />
          </div>
        </div>
      )}
    </div>
  );
}