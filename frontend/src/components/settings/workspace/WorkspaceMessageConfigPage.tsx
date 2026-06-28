import { useState } from 'react';
import { Button } from 'antd';
import { ArrowLeftOutlined, RobotOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import type { AgentBot, ProjectDirectory } from '@/utils/database';
import { WorkspaceAgentPanel } from './WorkspaceAgentPanel';
import { WorkspaceSlashCommandsPanel } from './WorkspaceSlashCommandsPanel';
import { WorkspaceSettingsPanel } from './WorkspaceSettingsPanel';

interface WorkspaceMessageConfigPageProps {
  workspace: ProjectDirectory;
  onBack: () => void;
}

/**
 * 工作空间消息配置页：整合智能体管理、斜杠命令、工作空间设置
 * 原 WorkspaceDetailPage 中的「智能体」tab 内容
 */
export function WorkspaceMessageConfigPage({ workspace, onBack }: WorkspaceMessageConfigPageProps) {
  // 选中的 bot（详情页或消息记录），提升到父层统一控制
  const [activeBot, setActiveBot] = useState<AgentBot | null>(null);
  const [activeBotForHistory, setActiveBotForHistory] = useState<AgentBot | null>(null);

  const handleBack = () => {
    if (activeBot || activeBotForHistory) {
      setActiveBot(null);
      setActiveBotForHistory(null);
      return;
    }
    onBack();
  };

  // 如果有选中的 bot（查看详情或消息记录），只显示 BotDetailPage
  const viewingBot = activeBot || activeBotForHistory;
  if (viewingBot) {
    // BotDetailPage 会在内部处理返回逻辑，这里提前渲染
    // 但我们需要通过 WorkspaceAgentPanel 来渲染，因为它有完整的 BotDetailPage
    return (
      <PageCard
        icon={
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <Button
              type="text"
              size="small"
              icon={<ArrowLeftOutlined />}
              onClick={handleBack}
              style={{ marginLeft: -8 }}
            />
            <RobotOutlined />
          </div>
        }
        title={`${workspace.name} - 消息配置`}
      >
        <WorkspaceAgentPanel
          workspaceId={workspace.id}
          activeBot={viewingBot}
          onBotBack={() => { setActiveBot(null); setActiveBotForHistory(null); }}
          autoShowHistory={!!activeBotForHistory}
        />
      </PageCard>
    );
  }

  return (
    <PageCard
      icon={
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <Button
            type="text"
            size="small"
            icon={<ArrowLeftOutlined />}
            onClick={onBack}
            style={{ marginLeft: -8 }}
          />
          <RobotOutlined />
        </div>
      }
      title={`${workspace.name} - 消息配置`}
    >
      <div className="workspace-message-config-page">
        <WorkspaceAgentPanel
          workspaceId={workspace.id}
          onSelectBot={(bot) => setActiveBot(bot)}
          onSelectBotForHistory={(bot) => setActiveBotForHistory(bot)}
        />
        <div style={{ marginTop: 24 }}>
          <WorkspaceSlashCommandsPanel workspaceId={workspace.id} />
        </div>
        <div style={{ marginTop: 24 }}>
          <WorkspaceSettingsPanel workspaceId={workspace.id} />
        </div>
      </div>
    </PageCard>
  );
}
