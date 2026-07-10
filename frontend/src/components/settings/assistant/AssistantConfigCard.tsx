// Assistant 基本配置卡片：dm/group/echo 回复开关。

import { Switch, Card } from 'antd';
import type { AgentBot } from '@/utils/database';

interface AssistantConfigCardProps {
  bot: AgentBot;
  botConfig: Record<string, boolean>;
  onConfigChange: (key: string, val: boolean) => void;
}

export function AssistantConfigCard({ bot, botConfig, onConfigChange }: AssistantConfigCardProps) {
  const isFeishu = bot.bot_type === 'feishu';

  return (
    <Card title="基本信息" size="small" style={{ marginBottom: 16 }}>
      {/* Bot 类型标识 */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 12 }}>
        <div style={{
          width: 36, height: 36, borderRadius: 8,
          background: isFeishu ? '#1976D2' : '#888',
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          color: '#fff', fontWeight: 700, fontSize: 14,
        }}>
          {isFeishu ? '飞' : '其他'}
        </div>
        <div>
          <div style={{ fontWeight: 600, fontSize: 14 }}>{bot.bot_name}</div>
          <div style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>App ID: {bot.app_id}</div>
        </div>
      </div>

      {/* 开关控制 */}
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px 16px' }}>
        <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 13 }}>
          <Switch size="small" checked={botConfig.dm_enabled !== false} onChange={v => onConfigChange('dm_enabled', v)} />
          接收单聊消息
        </span>
        <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 13 }}>
          <Switch size="small" checked={botConfig.group_enabled !== false} onChange={v => onConfigChange('group_enabled', v)} />
          接收群聊消息
        </span>
        <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 13 }}>
          <Switch size="small" checked={botConfig.group_require_mention !== false} onChange={v => onConfigChange('group_require_mention', v)} />
          群聊仅处理@
        </span>
        <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 13 }}>
          <Switch size="small" checked={botConfig.echo_reply !== false} onChange={v => onConfigChange('echo_reply', v)} />
          Echo 回复
        </span>
      </div>
    </Card>
  );
}
