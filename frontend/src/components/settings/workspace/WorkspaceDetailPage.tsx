import { Button } from 'antd';
import { ArrowLeftOutlined, FolderOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import type { ProjectDirectory } from '@/utils/database';
import { WorkspaceAgentPanel } from './WorkspaceAgentPanel';
import { WorkspaceSlashCommandsPanel } from './WorkspaceSlashCommandsPanel';
import { WorkspaceSettingsPanel } from './WorkspaceSettingsPanel';
import { ReviewTemplatesPanel } from '../ReviewTemplatesPanel';

interface WorkspaceDetailPageProps {
  workspace: ProjectDirectory;
  onBack: () => void;
}

/**
 * 工作空间详情页：展示智能体、斜杠命令、工作空间设置和 Loop 评审模板。
 * 不再使用 Tab 切换，所有配置项平铺展示，一步交互完成。
 */
export function WorkspaceDetailPage({ workspace, onBack }: WorkspaceDetailPageProps) {
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
          <FolderOutlined />
        </div>
      }
      title={workspace.name}
    >
      <div className="workspace-detail-page" style={{ display: 'flex', flexDirection: 'column', gap: 24 }}>
        {/* 智能体面板 */}
        <WorkspaceAgentPanel workspaceId={workspace.id} />
        {/* 斜杠命令面板 */}
        <WorkspaceSlashCommandsPanel workspaceId={workspace.id} />
        {/* 工作空间设置面板 */}
        <WorkspaceSettingsPanel workspaceId={workspace.id} />
        {/* Loop 评审模板面板 */}
        <ReviewTemplatesPanel workspaceId={workspace.id} />
      </div>
    </PageCard>
  );
}
