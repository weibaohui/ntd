import { useState, useEffect, useCallback } from 'react';
import { Spin, Empty, Button, Space, Typography } from 'antd';
import { RobotOutlined, PlusOutlined, EyeOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { useIsMobile } from '@/hooks/useIsMobile';
import * as db from '@/utils/database';
import type { AgentBot, ProjectDirectory } from '@/utils/database';
import { BotConfigDrawer } from './BotConfigDrawer';
import { BotListTable } from './BotListTable';
import { BotListCards } from './BotListCards';

const { Title } = Typography;

interface BotManagementPageProps {}

export function BotManagementPage({}: BotManagementPageProps) {
  const isMobile = useIsMobile();
  const [loading, setLoading] = useState(true);
  const [bots, setBots] = useState<AgentBot[]>([]);
  const [workspaces, setWorkspaces] = useState<ProjectDirectory[]>([]);
  const [configDrawerOpen, setConfigDrawerOpen] = useState(false);
  const [selectedBot, setSelectedBot] = useState<AgentBot | null>(null);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const [botList, workspaceList] = await Promise.all([
        db.getAgentBots(),
        db.getProjectDirectories(),
      ]);
      setBots(botList);
      setWorkspaces(workspaceList);
    } catch {}
    setLoading(false);
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleOpenConfig = (bot: AgentBot) => {
    setSelectedBot(bot);
    setConfigDrawerOpen(true);
  };

  const handleToggleEnabled = async (bot: AgentBot) => {
    try {
      const newConfig = { ...JSON.parse(bot.config), enabled: !bot.enabled };
      await db.updateAgentBotConfig(bot.id, JSON.stringify(newConfig));
      loadData();
    } catch {}
  };

  const handleDelete = async (bot: AgentBot) => {
    try {
      await db.deleteAgentBot(bot.id);
      loadData();
    } catch {}
  };

  const handleRefresh = () => {
    loadData();
  };

  const handleConfigChanged = () => {
    loadData();
    setConfigDrawerOpen(false);
  };

  return (
    <PageCard icon={<RobotOutlined />} title="智能体管理中心">
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 20 }}>
        <div>
          <Title level={4} style={{ margin: 0 }}>智能体管理</Title>
          <p style={{ margin: '4px 0 0', color: 'var(--color-text-secondary)', fontSize: 13 }}>
            统一管理所有智能体，配置推送规则和白名单
          </p>
        </div>
        <Space>
          <Button type="text" size="small" icon={<EyeOutlined />} onClick={handleRefresh}>刷新</Button>
          <Button type="primary" size="small" icon={<PlusOutlined />}>
            绑定智能体
          </Button>
        </Space>
      </div>

      {loading ? (
        <div style={{ display: 'flex', justifyContent: 'center', padding: 48 }}>
          <Spin />
        </div>
      ) : bots.length === 0 ? (
        <Empty description="暂无智能体，点击上方按钮绑定" />
      ) : isMobile ? (
        <BotListCards
          bots={bots}
          workspaces={workspaces}
          onOpenConfig={handleOpenConfig}
          onToggleEnabled={handleToggleEnabled}
          onDelete={handleDelete}
        />
      ) : (
        <BotListTable
          bots={bots}
          workspaces={workspaces}
          onOpenConfig={handleOpenConfig}
          onToggleEnabled={handleToggleEnabled}
          onDelete={handleDelete}
        />
      )}

      <BotConfigDrawer
        open={configDrawerOpen}
        bot={selectedBot}
        workspaces={workspaces}
        onClose={() => setConfigDrawerOpen(false)}
        onChanged={handleConfigChanged}
      />
    </PageCard>
  );
}