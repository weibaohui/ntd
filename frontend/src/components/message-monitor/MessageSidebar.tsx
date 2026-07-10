import { Typography } from 'antd';
import { RobotOutlined } from '@ant-design/icons';
import type { AgentBot } from '@/utils/database';

const { Text } = Typography;

interface BotListItemProps {
  bot: AgentBot;
  isActive: boolean;
  onClick: () => void;
}

function BotListItem({ bot, isActive, onClick }: BotListItemProps) {
  return (
    <div
      onClick={onClick}
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        padding: '8px 12px',
        borderRadius: 6,
        cursor: 'pointer',
        transition: 'background-color 0.2s',
        backgroundColor: isActive ? 'var(--color-primary-bg)' : 'transparent',
      }}
      className={isActive ? 'active' : ''}
    >
      <div style={{ position: 'relative' }}>
        <RobotOutlined style={{ fontSize: 18, color: bot.enabled ? 'var(--color-primary)' : 'var(--color-text-tertiary)' }} />
        <div
          style={{
            position: 'absolute',
            bottom: -2,
            right: -2,
            width: 8,
            height: 8,
            borderRadius: '50%',
            backgroundColor: bot.enabled ? '#52c41a' : '#d9d9d9',
            border: '2px solid var(--color-bg-container)',
          }}
        />
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <Text style={{ fontSize: 13 }}>{bot.bot_name}</Text>
        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>
          {bot.bot_type}
        </div>
      </div>
    </div>
  );
}

interface MessageSidebarProps {
  bots: AgentBot[];
  activeBotId: number | null;
  onSelectBot: (botId: number | null) => void;
}

export function MessageSidebar({ bots, activeBotId, onSelectBot }: MessageSidebarProps) {
  return (
    <div style={{ width: 200, borderRight: '1px solid var(--color-border-secondary)', paddingRight: 12 }}>
      <div style={{ padding: '8px 12px', fontSize: 12, fontWeight: 500, color: 'var(--color-text-secondary)' }}>
        Bot 列表 ({bots.length})
      </div>

      <div
        onClick={() => onSelectBot(null)}
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '8px 12px',
          borderRadius: 6,
          cursor: 'pointer',
          transition: 'background-color 0.2s',
          backgroundColor: activeBotId === null ? 'var(--color-primary-bg)' : 'transparent',
        }}
      >
        <div style={{ width: 18, height: 18, borderRadius: '50%', backgroundColor: 'var(--color-primary)', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
          <Text style={{ color: 'white', fontSize: 10, fontWeight: 'bold' }}>✦</Text>
        </div>
        <Text style={{ fontSize: 13 }}>全部消息</Text>
      </div>

      <div style={{ marginTop: 8 }}>
        {bots.map(bot => (
          <BotListItem
            key={bot.id}
            bot={bot}
            isActive={activeBotId === bot.id}
            onClick={() => onSelectBot(bot.id)}
          />
        ))}
      </div>

      {bots.length === 0 && (
        <div style={{ padding: '16px 12px', textAlign: 'center', color: 'var(--color-text-tertiary)', fontSize: 13 }}>
          当前工作空间暂无 Bot
        </div>
      )}
    </div>
  );
}
