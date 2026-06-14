import { useState } from 'react';
import {
  RobotOutlined,
  UserOutlined,
  ToolOutlined,
  BulbOutlined,
  InfoCircleOutlined,
  LoadingOutlined,
  DownOutlined,
  RightOutlined,
  MessageOutlined,
} from '@ant-design/icons';
import XMarkdown from '@ant-design/x-markdown';
import type { LogEntry, ChatMessage } from '@/types';

export { type ChatMessage };

interface ChatViewProps {
  logs: LogEntry[];
  isRunning?: boolean;
}

export function parseLogsToMessages(logs: LogEntry[]): ChatMessage[] {
  const messages: ChatMessage[] = [];
  let currentThinking = '';
  let currentToolName = '';
  let currentToolInput = '';
  let isCollectingTool = false;

  for (const log of logs) {
    // Skip logs with null/undefined content to prevent crashes
    if (log.content == null) continue;

    switch (log.type) {
      case 'user':
        messages.push({ role: 'user', content: log.content, timestamp: log.timestamp });
        break;
      case 'assistant':
        if (currentThinking) {
          messages.push({ role: 'thinking', content: currentThinking, timestamp: log.timestamp, isCollapsed: true });
          currentThinking = '';
        }
        if (isCollectingTool && currentToolName) {
          messages.push({ role: 'tool', content: '', timestamp: log.timestamp, toolName: currentToolName, toolInput: currentToolInput, isCollapsed: true });
          currentToolName = '';
          currentToolInput = '';
          isCollectingTool = false;
        }
        messages.push({ role: 'assistant', content: log.content, timestamp: log.timestamp });
        break;
      case 'thinking':
        currentThinking += log.content + '\n';
        break;
      case 'tool':
      case 'tool_use':
      case 'tool_call':
        if (isCollectingTool && (currentToolName || currentToolInput)) {
          messages.push({ role: 'tool', content: '', timestamp: log.timestamp, toolName: currentToolName || '工具调用', toolInput: currentToolInput, isCollapsed: true });
          currentToolName = '';
          currentToolInput = '';
          isCollectingTool = false;
        }
        if (currentThinking) {
          messages.push({ role: 'thinking', content: currentThinking, timestamp: log.timestamp, isCollapsed: true });
          currentThinking = '';
        }
        try {
          const toolData = JSON.parse(log.content);
          currentToolName = toolData.name || toolData.tool || '工具调用';
          currentToolInput = toolData.input ? JSON.stringify(toolData.input, null, 2) : log.content;
          isCollectingTool = true;
        } catch {
          currentToolName = '工具调用';
          currentToolInput = log.content;
          isCollectingTool = true;
        }
        break;
      case 'tool_result':
        if (isCollectingTool && (currentToolName || currentToolInput)) {
          messages.push({ role: 'tool', content: '', timestamp: log.timestamp, toolName: currentToolName || '工具调用', toolInput: currentToolInput, toolResult: log.content, isCollapsed: true });
          currentToolName = '';
          currentToolInput = '';
          isCollectingTool = false;
        } else {
          messages.push({ role: 'tool', content: '', timestamp: log.timestamp, toolName: '工具调用', toolResult: log.content, isCollapsed: true });
        }
        break;
      case 'result':
        if (currentThinking) {
          messages.push({ role: 'thinking', content: currentThinking, timestamp: log.timestamp, isCollapsed: true });
          currentThinking = '';
        }
        if (isCollectingTool && (currentToolName || currentToolInput)) {
          messages.push({ role: 'tool', content: '', timestamp: log.timestamp, toolName: currentToolName || '工具调用', toolInput: currentToolInput, isCollapsed: true });
          currentToolName = '';
          currentToolInput = '';
          isCollectingTool = false;
        }
        messages.push({ role: 'result', content: log.content, timestamp: log.timestamp });
        break;
      case 'info':
      case 'system':
      case 'stdout':
      case 'stderr':
      case 'error':
      case 'text':
      case 'step_start':
      case 'step_finish':
      case 'tokens':
        messages.push({ role: 'system', content: log.content, timestamp: log.timestamp });
        break;
    }
  }

  if (currentThinking) {
    messages.push({ role: 'thinking', content: currentThinking });
  }
  if (isCollectingTool && (currentToolName || currentToolInput)) {
    messages.push({ role: 'tool', content: '', toolName: currentToolName || '工具调用', toolInput: currentToolInput });
  }

  return messages;
}

import { formatTimeFull } from '@/utils/format';

// 卡片类型配置
const CARD_CONFIG = {
  thinking: {
    icon: BulbOutlined,
    label: '思考',
    color: '#f59e0b',
    bgColor: 'rgba(245, 158, 11, 0.1)',
    borderColor: 'rgba(245, 158, 11, 0.3)',
  },
  tool: {
    icon: ToolOutlined,
    label: '工具',
    color: '#8b5cf6',
    bgColor: 'rgba(139, 92, 246, 0.1)',
    borderColor: 'rgba(139, 92, 246, 0.3)',
  },
  output: {
    icon: MessageOutlined,
    label: '输出',
    color: '#3b82f6',
    bgColor: 'rgba(59, 130, 246, 0.1)',
    borderColor: 'rgba(59, 130, 246, 0.3)',
  },
  system: {
    icon: InfoCircleOutlined,
    label: '系统',
    color: '#94a3b8',
    bgColor: 'rgba(148, 163, 184, 0.1)',
    borderColor: 'rgba(148, 163, 184, 0.3)',
  },
} as const;

// 通用的可折叠卡片组件
function CollapsibleCard({
  type,
  preview,
  children,
  timestamp,
}: {
  type: 'thinking' | 'tool' | 'output' | 'system';
  preview?: string;
  children: React.ReactNode;
  timestamp?: string;
}) {
  const [expanded, setExpanded] = useState(false);
  const config = CARD_CONFIG[type];
  const Icon = config.icon;

  return (
    <div
      className="chat-collapsible-card"
      style={{
        borderLeft: `3px solid ${config.color}`,
        background: config.bgColor,
        borderColor: config.borderColor,
      }}
    >
      <button
        type="button"
        className="chat-card-header"
        onClick={() => setExpanded(!expanded)}
        aria-expanded={expanded}
      >
        <div className="chat-card-header-left">
          <span className="chat-card-icon" style={{ color: config.color }}>
            <Icon />
          </span>
          <span className="chat-card-label" style={{ color: config.color }}>
            {config.label}
          </span>
          {preview && !expanded && (
            <span className="chat-card-preview">{preview}</span>
          )}
        </div>
        <div className="chat-card-header-right">
          {timestamp && <span className="chat-card-time">{formatTimeFull(timestamp)}</span>}
          <span className="chat-card-toggle" style={{ color: config.color }}>
            {expanded ? <DownOutlined /> : <RightOutlined />}
          </span>
        </div>
      </button>
      {expanded && <div className="chat-card-content">{children}</div>}
    </div>
  );
}

// 思考块
function ThinkingBlock({ content, timestamp }: { content: string; timestamp?: string }) {
  return (
    <CollapsibleCard type="thinking" timestamp={timestamp}>
      <XMarkdown content={content} />
    </CollapsibleCard>
  );
}

// 工具块
function ToolBlock({
  toolName,
  toolInput,
  toolResult,
  timestamp,
}: {
  toolName?: string;
  toolInput?: string;
  toolResult?: string;
  timestamp?: string;
}) {
  // 生成参数预览
  const getInputPreview = () => {
    if (!toolInput) return '';
    try {
      const parsed = JSON.parse(toolInput);
      const keys = Object.keys(parsed);
      if (keys.length === 0) return '{}';
      const preview = keys
        .map((k) => {
          const val = parsed[k];
          const strVal = typeof val === 'string' ? `"${val.substring(0, 15)}${val.length > 15 ? '...' : ''}"` : String(val);
          return `${k}: ${strVal}`;
        })
        .join(', ');
      return preview.length > 50 ? preview.substring(0, 50) + '...' : preview;
    } catch {
      return toolInput.length > 50 ? toolInput.substring(0, 50) + '...' : toolInput;
    }
  };

  return (
    <CollapsibleCard type="tool" preview={`${toolName || '工具调用'} ${getInputPreview()}`} timestamp={timestamp}>
      {toolInput && (
        <div className="chat-card-section">
          <div className="chat-card-section-label">输入参数</div>
          <pre className="chat-card-code">{toolInput}</pre>
        </div>
      )}
      {toolResult && (
        <div className="chat-card-section">
          <div className="chat-card-section-label">执行结果</div>
          <pre className="chat-card-code">{toolResult}</pre>
        </div>
      )}
    </CollapsibleCard>
  );
}

// 结果块
function ResultBlock({ content, timestamp }: { content: string; timestamp?: string }) {
  return (
    <CollapsibleCard type="output" timestamp={timestamp}>
      <XMarkdown content={content} />
    </CollapsibleCard>
  );
}

// 系统消息块
function SystemBlock({ content, timestamp }: { content: string; timestamp?: string }) {
  return (
    <CollapsibleCard type="system" timestamp={timestamp}>
      <span>{content}</span>
    </CollapsibleCard>
  );
}

// 消息气泡
function ChatBubble({ message }: { message: ChatMessage }) {
  const { role, content, timestamp, toolName, toolInput, toolResult } = message;

  if (role === 'thinking') {
    return <ThinkingBlock content={content} timestamp={timestamp} />;
  }

  if (role === 'tool') {
    return <ToolBlock toolName={toolName} toolInput={toolInput} toolResult={toolResult} timestamp={timestamp} />;
  }

  if (role === 'system') {
    return <SystemBlock content={content} timestamp={timestamp} />;
  }

  if (role === 'result') {
    return <ResultBlock content={content} timestamp={timestamp} />;
  }

  const isUser = role === 'user';
  return (
    <div className={`chat-bubble-row ${isUser ? 'chat-bubble-user' : 'chat-bubble-assistant'}`}>
      <div className="chat-avatar">
        {isUser ? <UserOutlined /> : <RobotOutlined />}
      </div>
      <div className="chat-bubble">
        <div className="chat-bubble-content">
          <XMarkdown content={content} />
        </div>
        {timestamp && <div className="chat-bubble-time">{formatTimeFull(timestamp)}</div>}
      </div>
    </div>
  );
}

export function ChatView({ logs, isRunning }: ChatViewProps) {
  const messages = parseLogsToMessages(logs);

  if (messages.length === 0) {
    return (
      <div className="chat-empty">
        {isRunning ? (
          <div className="chat-loading">
            <LoadingOutlined style={{ fontSize: 24, color: 'var(--color-primary)' }} />
            <span>等待AI响应...</span>
          </div>
        ) : (
          <span>暂无对话记录</span>
        )}
      </div>
    );
  }

  return (
    <div className="chat-container">
      <div className="chat-messages">
        {messages.map((msg, idx) => (
          <ChatBubble key={idx} message={msg} />
        ))}
        {isRunning && (
          <div className="chat-typing-indicator">
            <div className="chat-typing-dot" />
            <div className="chat-typing-dot" />
            <div className="chat-typing-dot" />
          </div>
        )}
      </div>
    </div>
  );
}
