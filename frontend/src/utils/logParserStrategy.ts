/**
 * Log Parser Strategy Pattern 实现
 *
 * 将 `parseLogsToMessages` 的 switch-case 逻辑重构为策略模式，
 * 每种日志类型对应一个策略，新增类型只需添加新策略，无需修改现有代码。
 *
 * ## 策略接口
 * 每个策略负责识别并处理一种或多种日志类型，返回处理后的消息片段。
 * 上下文 (ParsingContext) 携带累积状态（thinking、tool 等跨日志累积的数据）。
 */

import type { LogEntry, ChatMessage } from '@/types';

/**
 * 解析上下文 — 携带跨日志累积的状态
 */
class ParsingContext {
  messages: ChatMessage[] = [];
  currentThinking: string = '';
  currentToolName: string = '';
  currentToolInput: string = '';
  isCollectingTool: boolean = false;

  /**
   * _flushThinking 将累积的 thinking 内容作为消息推入，并清空状态
   */
  flushThinking(timestamp?: string): void {
    if (this.currentThinking) {
      this.messages.push({
        role: 'thinking',
        content: this.currentThinking,
        timestamp,
        isCollapsed: true,
      });
      this.currentThinking = '';
    }
  }

  /**
   * _flushTool 将累积的 tool 调用作为消息推入，并清空状态
   */
  flushTool(timestamp?: string): void {
    if (this.isCollectingTool && (this.currentToolName || this.currentToolInput)) {
      this.messages.push({
        role: 'tool',
        content: '',
        timestamp,
        toolName: this.currentToolName || '工具调用',
        toolInput: this.currentToolInput,
        isCollapsed: true,
      });
      this.currentToolName = '';
      this.currentToolInput = '';
      this.isCollectingTool = false;
    }
  }

  /**
   * 结束解析时调用，清理所有剩余状态
   */
  finalize(timestamp?: string): void {
    this.flushThinking(timestamp);
    this.flushTool(timestamp);
  }

  /**
   * 推送一条消息
   */
  push(message: ChatMessage): void {
    this.messages.push(message);
  }
}

/**
 * 日志解析策略接口
 */
interface LogParserStrategy {
  /**
   * 判断此策略是否能处理该日志
   */
  canHandle(log: LogEntry): boolean;

  /**
   * 解析日志并更新上下文
   * @param log 日志条目
   * @param ctx 解析上下文
   */
  parse(log: LogEntry, ctx: ParsingContext): void;
}

// ============================================================================
// 具体策略实现
// ============================================================================

/**
 * user 日志解析 — 用户输入
 */
const UserLogStrategy: LogParserStrategy = {
  canHandle: (log) => log.type === 'user',
  parse: (log, ctx) => {
    ctx.flushThinking(log.timestamp);
    ctx.flushTool(log.timestamp);
    ctx.push({ role: 'user', content: log.content, timestamp: log.timestamp });
  },
};

/**
 * assistant 日志解析 — AI 响应
 */
const AssistantLogStrategy: LogParserStrategy = {
  canHandle: (log) => log.type === 'assistant',
  parse: (log, ctx) => {
    // assistant 出现时，先 flush 之前累积的 thinking 和 tool
    ctx.flushThinking(log.timestamp);
    ctx.flushTool(log.timestamp);
    ctx.push({ role: 'assistant', content: log.content, timestamp: log.timestamp });
  },
};

/**
 * thinking 日志解析 — 思考内容（跨多条日志累积）
 */
const ThinkingLogStrategy: LogParserStrategy = {
  canHandle: (log) => log.type === 'thinking',
  parse: (log, ctx) => {
    ctx.currentThinking += log.content + '\n';
  },
};

/**
 * tool_call 系列日志解析 — 工具调用开始（跨多条日志累积）
 */
const ToolCallLogStrategy: LogParserStrategy = {
  canHandle: (log) => log.type === 'tool' || log.type === 'tool_use' || log.type === 'tool_call',
  parse: (log, ctx) => {
    // tool_call 出现时，先 flush 之前的 thinking 和 tool
    ctx.flushThinking(log.timestamp);
    ctx.flushTool(log.timestamp);

    // 解析工具名称和输入
    try {
      const toolData = JSON.parse(log.content);
      ctx.currentToolName = toolData.name || toolData.tool || '工具调用';
      // 使用严格的 null/undefined 检查而非 truthy 判断，
      // 避免将合法的假值（0、false、""）误判为缺失输入而丢弃。
      ctx.currentToolInput = toolData.input != null
        ? JSON.stringify(toolData.input, null, 2)
        : log.content;
    } catch {
      ctx.currentToolName = '工具调用';
      ctx.currentToolInput = log.content;
    }
    ctx.isCollectingTool = true;
  },
};

/**
 * tool_result 日志解析 — 工具调用结果
 */
const ToolResultLogStrategy: LogParserStrategy = {
  canHandle: (log) => log.type === 'tool_result',
  parse: (log, ctx) => {
    // 在推送 tool_result 消息之前先 flush thinking，
    // 避免 thinking 内容被推迟到 tool_result 之后，造成消息顺序错乱。
    ctx.flushThinking(log.timestamp);
    if (ctx.isCollectingTool && (ctx.currentToolName || ctx.currentToolInput)) {
      // 合并到累积的 tool 消息
      ctx.messages.push({
        role: 'tool',
        content: '',
        timestamp: log.timestamp,
        toolName: ctx.currentToolName || '工具调用',
        toolInput: ctx.currentToolInput,
        toolResult: log.content,
        isCollapsed: true,
      });
      ctx.currentToolName = '';
      ctx.currentToolInput = '';
      ctx.isCollectingTool = false;
    } else {
      // 没有累积的 tool_call，直接作为独立 tool 消息
      ctx.push({
        role: 'tool',
        content: '',
        timestamp: log.timestamp,
        toolName: '工具调用',
        toolResult: log.content,
        isCollapsed: true,
      });
    }
  },
};

/**
 * result 日志解析 — 最终结果
 */
const ResultLogStrategy: LogParserStrategy = {
  canHandle: (log) => log.type === 'result',
  parse: (log, ctx) => {
    ctx.flushThinking(log.timestamp);
    ctx.flushTool(log.timestamp);
    ctx.push({ role: 'result', content: log.content, timestamp: log.timestamp });
  },
};

/**
 * system 系列日志解析 — 系统消息（info、system、stdout、stderr、error、text、step_start、step_finish、tokens）
 */
const SystemLogStrategy: LogParserStrategy = {
  canHandle: (log) =>
    ['info', 'system', 'stdout', 'stderr', 'error', 'text', 'step_start', 'step_finish', 'tokens'].includes(
      log.type
    ),
  parse: (log, ctx) => {
    ctx.flushThinking(log.timestamp);
    ctx.flushTool(log.timestamp);
    ctx.push({ role: 'system', content: log.content, timestamp: log.timestamp });
  },
};

/**
 * 所有策略列表 — 按优先级排序
 *
 * 顺序很重要：
 * 1. tool_result 需要在 tool_call 之前处理，因为它可能 заверш 一个 tool 序列
 * 2. thinking 需要在其他类型之前累积
 * 3. system 系列放在最后，作为兜底
 */
const LOG_PARSER_STRATEGIES: LogParserStrategy[] = [
  UserLogStrategy,
  AssistantLogStrategy,
  ThinkingLogStrategy,
  ToolCallLogStrategy,
  ToolResultLogStrategy,
  ResultLogStrategy,
  SystemLogStrategy,
];

/**
 * 使用策略模式解析日志
 *
 * @param logs 日志列表
 * @returns 解析后的消息列表
 */
function parseLogsWithStrategies(logs: LogEntry[]): ChatMessage[] {
  const ctx = new ParsingContext();

  for (const log of logs) {
    // Skip logs with null/undefined content to prevent crashes
    if (log.content == null) continue;

    // 遍历策略列表，找到第一个能处理此日志的策略
    for (const parser of LOG_PARSER_STRATEGIES) {
      if (parser.canHandle(log)) {
        parser.parse(log, ctx);
        break; // 只用第一个匹配的策略
      }
    }
  }

  // 结束解析，清理剩余状态
  // 传入最后一条日志的时间戳，确保 finalize 推送的剩余消息时间顺序一致。
  ctx.finalize(logs.length > 0 ? logs[logs.length - 1].timestamp : undefined);

  return ctx.messages;
}

/**
 * 将日志列表解析为对话消息
 */
export function parseLogsToMessages(logs: LogEntry[]): ChatMessage[] {
  return parseLogsWithStrategies(logs);
}
