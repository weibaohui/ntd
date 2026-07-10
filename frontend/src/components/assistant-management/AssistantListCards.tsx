import { Card, Tag, Button, Space, Typography } from 'antd';
import { RobotOutlined, SettingOutlined, PoweroffOutlined, DeleteOutlined } from '@ant-design/icons';
import type { AgentBot, ProjectDirectory } from '@/utils/database';

const { Text } = Typography;

interface AssistantListCardsProps {
  bots: AgentBot[];
  workspaces: ProjectDirectory[];
  onOpenConfig: (bot: AgentBot) => void;
  onToggleEnabled: (bot: AgentBot) => void;
  onDelete: (bot: AgentBot) => void;
}

export function AssistantListCards({ bots, workspaces, onOpenConfig, onToggleEnabled, onDelete }: AssistantListCardsProps) {
  const getWorkspaceName = (workspaceId: number) => {
    return workspaces.find(w => w.id === workspaceId)?.name || '-';
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      {bots.map(bot => (
        <Card key={bot.id} size="small" hoverable style={{ borderRadius: 8 }}>
          <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', marginBottom: 12 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <div style={{
                width: 40,
                height: 40,
                borderRadius: 8,
                backgroundColor: 'var(--color-bg-secondary)',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
              }}>
                <RobotOutlined style={{ fontSize: 18, color: 'var(--color-primary)' }} />
              </div>
              <div>
                <Text strong style={{ fontSize: 15 }}>{bot.bot_name}</Text>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 4 }}>
                  <Tag color={bot.bot_type === 'feishu' ? 'blue' : 'default'} style={{ fontSize: 11 }}>
                    {bot.bot_type === 'feishu' ? '飞书' : bot.bot_type}
                  </Tag>
                  <Tag color={bot.enabled ? 'green' : 'red'} style={{ fontSize: 11 }}>
                    {bot.enabled ? '运行中' : '已停用'}
                  </Tag>
                </div>
              </div>
            </div>
          </div>

          <div style={{ marginBottom: 12 }}>
            <Text style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>当前服务：</Text>
            <Tag color={bot.workspace_id ? 'green' : 'gray'} style={{ fontSize: 11, marginLeft: 4 }}>
              {bot.workspace_id ? getWorkspaceName(bot.workspace_id) : '未分配'}
            </Tag>
          </div>

          <div style={{ marginBottom: 12, padding: 8, backgroundColor: 'var(--color-bg-secondary)', borderRadius: 4 }}>
            <Text style={{ fontSize: 11, color: 'var(--color-text-secondary)' }}>App ID：</Text>
            <code style={{ fontSize: 11, color: 'var(--color-text-primary)' }}>{bot.app_id}</code>
          </div>

          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <Text style={{ fontSize: 11, color: 'var(--color-text-secondary)' }}>
              {new Date(bot.created_at).toLocaleDateString('zh-CN')}
            </Text>
            <Space>
              <Button type="text" size="small" icon={<SettingOutlined />} onClick={() => onOpenConfig(bot)}>
                配置
              </Button>
              <Button
                type="text"
                size="small"
                icon={<PoweroffOutlined />}
                onClick={() => onToggleEnabled(bot)}
                danger={bot.enabled}
              >
                {bot.enabled ? '停用' : '启用'}
              </Button>
              <Button type="text" size="small" icon={<DeleteOutlined />} danger onClick={() => onDelete(bot)}>
                删除
              </Button>
            </Space>
          </div>
        </Card>
      ))}
    </div>
  );
}