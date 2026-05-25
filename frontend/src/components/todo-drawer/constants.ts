export const DEFAULT_CRON = '0 */10 * * * *';

export const PROMPT_PARAMS = [
  { key: '{{content}}', label: 'content', desc: '消息内容（已清理格式）' },
  { key: '{{message}}', label: 'message', desc: '原始消息文本' },
  { key: '{{raw_message}}', label: 'raw_message', desc: '未处理的原始消息' },
  { key: '{{slash_command}}', label: 'slash_command', desc: '斜杠命令内容' },
];
