// 飞书消息配置表单：选择 bot、chat_id、匹配方式和关键词。
// 存储格式：{"bot_id":1,"chat_id":"oc_xxx","match_type":"contains","pattern":"hello"}

import { useState, useEffect, useMemo } from 'react';
import { Form, Input, Select } from 'antd';
import * as db from '@/utils/database';

interface FeishuMessageConfigFormProps {
  value: string;
  onChange: (json: string) => void;
}

export function FeishuMessageConfigForm({ value, onChange }: FeishuMessageConfigFormProps) {
  const [bots, setBots] = useState<{ id: number; name: string }[]>([]);
  const [loading, setLoading] = useState(false);

  const parsed = useMemo(() => {
    try {
      const v = JSON.parse(value || '{}');
      return {
        bot_id: v.bot_id ?? null,
        chat_id: v.chat_id ?? '',
        match_type: v.match_type ?? 'contains',
        pattern: v.pattern ?? '',
      };
    } catch {
      return { bot_id: null, chat_id: '', match_type: 'contains', pattern: '' };
    }
  }, [value]);

  const [botId, setBotId] = useState<number | null>(parsed.bot_id);
  const [chatId, setChatId] = useState(parsed.chat_id);
  const [matchType, setMatchType] = useState(parsed.match_type);
  const [pattern, setPattern] = useState(parsed.pattern);

  // 加载 bot 列表
  useEffect(() => {
    setLoading(true);
    db.getAgentBots()
      .then((list) => setBots(list.map((b: any) => ({ id: b.id, name: b.bot_name || b.bot_type }))))
      .catch(() => { /* 静默 */ })
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    const config: Record<string, any> = {};
    if (botId) config.bot_id = botId;
    if (chatId) config.chat_id = chatId;
    if (matchType) config.match_type = matchType;
    if (pattern) config.pattern = pattern;
    onChange(JSON.stringify(config));
  }, [botId, chatId, matchType, pattern, onChange]);

  return (
    <div>
      <Form.Item label="飞书机器人" tooltip="接收消息的飞书机器人">
        <Select
          value={botId}
          onChange={(v) => setBotId(v)}
          loading={loading}
          allowClear
          placeholder="选择机器人"
          options={bots.map((b) => ({ value: b.id, label: b.name }))}
        />
      </Form.Item>
      <Form.Item label="群聊/会话 ID" tooltip="留空则匹配任意群聊或会话（仅限已配置的机器人）">
        <Input
          value={chatId}
          onChange={(e) => setChatId(e.target.value)}
          placeholder="留空=任意会话，示例：oc_xxxxxx"
        />
      </Form.Item>
      <Form.Item label="匹配方式" tooltip="选择如何匹配消息内容">
        <Select
          value={matchType}
          onChange={(v) => setMatchType(v)}
          options={[
            { value: 'contains', label: '包含（contains）' },
            { value: 'exact', label: '精确匹配（exact）' },
            { value: 'regex', label: '正则匹配（regex）' },
          ]}
        />
      </Form.Item>
      <Form.Item label="关键词/正则" tooltip="匹配的消息内容关键词或正则表达式（留空=全部命中）">
        <Input
          value={pattern}
          onChange={(e) => setPattern(e.target.value)}
          placeholder={matchType === 'regex' ? '例如：^bug\\s+\\d+' : '输入关键词'}
        />
      </Form.Item>
    </div>
  );
}
