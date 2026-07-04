import yaml from 'js-yaml';
import type { ChatMessage } from '@/types';

/**
 * 归一化黑板 Markdown 内容：剥掉 LLM 误包的最外层 fenced code block。
 *
 * 规则（尽量保守，与后端 normalize_blackboard_markdown 同语义）：
 * - 仅当整段内容以 ```markdown / ```md / ``` 开头，且以匹配的 ``` 结尾时才剥离
 * - 内部代码块不受影响
 * - 剥离后 trim() 一次
 * - 不满足条件时原样返回
 *
 * 这是"双保险"的前端侧：兼容兜底，保证历史脏数据也能正常渲染。
 */
export function normalizeBlackboardMarkdown(content: string): string {
  const trimmed = content.trim();
  // 快速失败：太短的内容不可能包着 fenced code block
  if (trimmed.length < 5) {
    return content;
  }
  // 匹配开头的 fenced code block：```markdown, ```md, 或纯 ```
  const startMarker = trimmed
    .startsWith('```markdown')
      ? trimmed.slice(11)
    : trimmed.startsWith('```md')
      ? trimmed.slice(7)
      : trimmed.startsWith('```')
        ? trimmed.slice(3)
        : null;
  if (startMarker === undefined || startMarker === null) {
    // 不是以 ``` 开头，原样返回
    return content;
  }
  // 跳过开头的换行
  const inner = startMarker.replace(/^\n+/, '');
  // 检查末尾是否有匹配的 ```
  if (!(inner.endsWith('\n```') || inner === '```')) {
    // 末尾没有 ```，说明不是完整的外层包裹，原样返回
    return content;
  }
  // 剥掉外层，trim 后返回
  const cleaned = inner.replace(/\n*```$/, '').trim();
  // 剥掉后为空则返回原始内容（保护已有内容不被清空）
  if (cleaned.length === 0) {
    return content;
  }
  return cleaned;
}

const STATUS_MAP: Record<string, string> = {
  success: '成功',
  failed: '失败',
  running: '运行中',
};

export function conversationToYaml(
  messages: ChatMessage[],
  meta?: {
    title?: string;
    executor?: string;
    model?: string;
    startedAt?: string;
    status?: string;
  },
): string {
  const header: Record<string, string> = {};
  if (meta?.title) header['任务'] = meta.title;
  if (meta?.executor) header['执行器'] = meta.executor;
  if (meta?.model) header['模型'] = meta.model;
  if (meta?.startedAt) header['开始时间'] = meta.startedAt;
  if (meta?.status) header['状态'] = STATUS_MAP[meta.status] || meta.status;

  const items = messages.map(msg => {
    const item: Record<string, unknown> = { role: msg.role };
    if (msg.timestamp) item['timestamp'] = msg.timestamp;
    switch (msg.role) {
      case 'user':
      case 'assistant':
      case 'thinking':
      case 'result':
        item['content'] = msg.content;
        break;
      case 'tool':
        item['name'] = msg.toolName || '工具';
        if (msg.toolInput) item['input'] = msg.toolInput;
        if (msg.toolResult) item['result'] = truncate(msg.toolResult, 5000);
        break;
      case 'system':
        item['content'] = msg.content;
        break;
    }
    return item;
  });

  const doc = {
    ...header,
    '导出时间': new Date().toLocaleString(),
    messages: items,
  };

  return yaml.dump(doc, { lineWidth: -1, forceQuotes: true, quotingType: "'" });
}

function truncate(text: string, maxLen: number): string {
  if (text.length <= maxLen) return text;
  return text.slice(0, maxLen) + '\n... (已截断)';
}
