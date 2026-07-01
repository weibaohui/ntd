// 飞书指令配置表单：选择 bot 和命令。
// 存储格式：{"bot_id":1,"command":"/run"}

import { useState, useEffect, useMemo } from 'react';
import { Form, Input, Select } from 'antd';
import * as db from '@/utils/database';

interface FeishuCommandConfigFormProps {
  value: string;
  onChange: (json: string) => void;
}

export function FeishuCommandConfigForm({ value, onChange }: FeishuCommandConfigFormProps) {
  const [bots, setBots] = useState<{ id: number; name: string }[]>([]);
  const [loading, setLoading] = useState(false);

  const parsed = useMemo(() => {
    try {
      const v = JSON.parse(value || '{}');
      return { bot_id: v.bot_id ?? null, command: v.command ?? '' };
    } catch {
      return { bot_id: null, command: '' };
    }
  }, [value]);

  const [botId, setBotId] = useState<number | null>(parsed.bot_id);
  const [command, setCommand] = useState(parsed.command);

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
    if (command) config.command = command;
    onChange(JSON.stringify(config));
  }, [botId, command, onChange]);

  return (
    <div>
      <Form.Item label="飞书机器人" tooltip="接收指令的飞书机器人">
        <Select
          value={botId}
          onChange={(v) => setBotId(v)}
          loading={loading}
          allowClear
          placeholder="选择机器人"
          options={bots.map((b) => ({ value: b.id, label: b.name }))}
        />
      </Form.Item>
      <Form.Item
        label="指令名称"
        tooltip="用户在飞书中发送的 slash 命令，例如 /run"
        rules={[{ required: false }]}
      >
        <Input
          value={command}
          onChange={(e) => setCommand(e.target.value)}
          placeholder="例如: /run, /review, /deploy"
        />
      </Form.Item>
    </div>
  );
}
