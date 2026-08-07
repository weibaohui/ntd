import { SettingOutlined } from '@ant-design/icons';
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
      icon={<SettingOutlined />}
      // 062：标题统一「模块名: 具体名称」格式，功能名在前、工作空间名在后
      title={`环路配置: ${workspace.name}`}
      // 062：返回按钮移交 PageCard 统一渲染（extra 最右端）
      onBack={onBack}
    >
      <div className="workspace-loop-config-page">
        <ReviewTemplatesPanel workspaceId={workspace.id} />
      </div>
    </PageCard>
  );
}
