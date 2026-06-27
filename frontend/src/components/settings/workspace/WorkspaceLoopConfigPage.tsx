import { Button } from 'antd';
import { ArrowLeftOutlined, SettingOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import type { ProjectDirectory } from '@/utils/database';
import { ReviewTemplatesPanel } from '../ReviewTemplatesPanel';

interface WorkspaceLoopConfigPageProps {
  workspace: ProjectDirectory;
  onBack: () => void;
}

/**
 * 工作空间 Loop 配置页：评审模板管理
 * 原 WorkspaceDetailPage 中的「Loop设置」tab 内容
 */
export function WorkspaceLoopConfigPage({ workspace, onBack }: WorkspaceLoopConfigPageProps) {
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
          <SettingOutlined />
        </div>
      }
      title={`${workspace.name} - 环路配置`}
    >
      <div className="workspace-loop-config-page">
        <ReviewTemplatesPanel workspaceId={workspace.id} />
      </div>
    </PageCard>
  );
}
