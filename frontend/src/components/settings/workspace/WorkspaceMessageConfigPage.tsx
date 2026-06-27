import { Button } from 'antd';
import { ArrowLeftOutlined, RobotOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import type { ProjectDirectory } from '@/utils/database';
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
        <WorkspaceAgentPanel workspaceId={workspace.id} />
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
