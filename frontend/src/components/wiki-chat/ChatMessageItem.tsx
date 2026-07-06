/**
 * ChatMessageItem — Wiki 对话单条消息渲染组件。
 *
 * 支持三种消息类型：用户消息、执行器日志、最终结果。
 * 根据 mobile 参数调整样式（字号、内边距等）。
 */

import { MessageOutlined } from '@ant-design/icons';
import { XMarkdown } from '@ant-design/x-markdown';
import { LOG_TYPE_COLORS_LIGHT, LOG_TYPE_COLORS_DARK, LOG_TYPE_LABELS } from '@/constants';
import type { LogEntry } from '@/types';

/** 对话消息类型定义 */
export type ChatMessage =
  | {
      id: string;
      role: 'user';
      content: string;
    }
  | {
      id: string;
      role: 'log';
      entry: LogEntry;
      taskId: string;
    }
  | {
      id: string;
      role: 'result';
      content: string;
      taskId: string;
      success: boolean;
      durationSecs?: number;
    };

interface ChatMessageItemProps {
  /** 消息数据 */
  message: ChatMessage;
  /** 是否移动端布局 */
  mobile?: boolean;
  /** 是否暗色主题 */
  isDark: boolean;
}

/** 格式化时间戳为 HH:MM:SS 格式 */
export function formatChatTime(timestamp?: string): string {
  if (!timestamp) return '';
  try {
    const d = new Date(timestamp);
    if (Number.isNaN(d.getTime())) return '';
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  } catch {
    return '';
  }
}

/** 颜色常量：根据主题返回 */
export function getChatColors(isDark: boolean) {
  return {
    panelBg: isDark ? '#1a1a1a' : '#ffffff',
    panelBorder: isDark ? '#333' : '#e8e8e8',
    userMsgBg: isDark ? '#1d3950' : '#e6f4ff',
    textColor: isDark ? '#e0e0e0' : '#333',
    hintColor: isDark ? '#666' : '#999',
    headerBg: isDark ? '#222' : '#fafafa',
    logTypeColors: isDark ? LOG_TYPE_COLORS_DARK : LOG_TYPE_COLORS_LIGHT,
  };
}

/** 单条消息渲染组件 */
export function ChatMessageItem({ message, mobile = false, isDark }: ChatMessageItemProps) {
  const colors = getChatColors(isDark);

  // 用户消息：右对齐蓝色气泡
  if (message.role === 'user') {
    return (
      <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
        <div
          style={{
            maxWidth: mobile ? '85%' : '80%',
            padding: mobile ? '12px 16px' : '10px 14px',
            borderRadius: 12,
            background: colors.userMsgBg,
            color: colors.textColor,
            fontSize: mobile ? 16 : 14,
            lineHeight: 1.6,
            whiteSpace: 'pre-wrap',
            wordBreak: 'break-word',
          }}
        >
          {message.content}
        </div>
      </div>
    );
  }

  // 执行器日志：左对齐，时间戳 + 类型标签 + 内容
  if (message.role === 'log') {
    const typeColor = colors.logTypeColors[message.entry.type] || colors.logTypeColors.info;
    const typeLabel = LOG_TYPE_LABELS[message.entry.type] || message.entry.type;
    return (
      <div style={{ display: 'flex', justifyContent: 'flex-start' }}>
        <div
          style={{
            maxWidth: '100%',
            fontSize: mobile ? 13 : 12,
            lineHeight: 1.6,
            fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
            color: colors.textColor,
            wordBreak: 'break-word',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 2 }}>
            {message.entry.timestamp && (
              <span style={{ color: colors.hintColor, fontSize: mobile ? 12 : 11 }}>
                {formatChatTime(message.entry.timestamp)}
              </span>
            )}
            <span
              style={{
                padding: '1px 6px',
                borderRadius: 3,
                fontSize: mobile ? 11 : 10,
                fontWeight: 600,
                background: typeColor,
                color: '#fff',
                textTransform: 'uppercase',
              }}
            >
              {typeLabel}
            </span>
          </div>
          <div style={{ whiteSpace: 'pre-wrap', paddingLeft: 4 }}>
            {message.entry.content}
          </div>
        </div>
      </div>
    );
  }

  // 最终结果：左对齐，Markdown渲染，成功/失败边框颜色区分
  return (
    <div style={{ display: 'flex', justifyContent: 'flex-start' }}>
      <div
        style={{
          maxWidth: mobile ? '92%' : '90%',
          padding: mobile ? '14px 16px' : '12px 14px',
          borderRadius: 12,
          background: isDark ? '#2a2a2a' : '#fff',
          color: colors.textColor,
          fontSize: mobile ? 15 : 14,
          lineHeight: 1.6,
          border: `2px solid ${message.success
            ? isDark ? '#3d7a3d' : '#52c41a'
            : isDark ? '#7a3d3d' : '#ff4d4f'}`,
          wordBreak: 'break-word',
        }}
      >
        <XMarkdown>{message.content}</XMarkdown>
        <div style={{ marginTop: 8, fontSize: mobile ? 12 : 11, color: colors.hintColor, display: 'flex', justifyContent: 'space-between' }}>
          <span>{message.success ? '✅ 执行成功' : '❌ 执行失败'}</span>
          {message.durationSecs != null && <span>用时 {message.durationSecs.toFixed(1)}s</span>}
        </div>
      </div>
    </div>
  );
}

/** 空状态占位组件 */
export function ChatEmptyPlaceholder({ mobile = false, isDark }: { mobile?: boolean; isDark: boolean }) {
  const colors = getChatColors(isDark);
  return (
    <div style={{ textAlign: 'center', color: colors.hintColor, fontSize: mobile ? 14 : 13, padding: '32px 0' }}>
      <MessageOutlined style={{ fontSize: 36, marginBottom: 12, opacity: 0.3 }} />
      <div>还没有对话记录</div>
      <div style={{ marginTop: 6, fontSize: mobile ? 13 : 12 }}>输入问题开始与 Wiki 交互</div>
    </div>
  );
}