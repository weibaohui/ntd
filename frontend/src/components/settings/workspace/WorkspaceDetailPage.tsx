import { useState } from 'react';
import { Tabs, Select, Button } from 'antd';
import { ArrowLeftOutlined, RobotOutlined, SettingOutlined, FolderOutlined } from '@ant-design/icons';
import { useIsMobile } from '@/hooks/useIsMobile';
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

type TabKey = 'agents' | 'loop-settings';

const TAB_OPTIONS = [
  { value: 'agents' as TabKey, label: '智能体', icon: <RobotOutlined /> },
  { value: 'loop-settings' as TabKey, label: 'Loop设置', icon: <SettingOutlined /> },
];

export function WorkspaceDetailPage({ workspace, onBack }: WorkspaceDetailPageProps) {
  const [activeTab, setActiveTab] = useState<TabKey>('agents');
  const isMobile = useIsMobile();

  /**
   * 渲染标签页内容
   */
  const renderTabContent = (key: TabKey) => {
    if (key === 'agents') {
      return (
        <>
          <WorkspaceAgentPanel workspaceId={workspace.id} />
          <div style={{ marginTop: 24 }}>
            <WorkspaceSlashCommandsPanel workspaceId={workspace.id} />
          </div>
          <div style={{ marginTop: 24 }}>
            <WorkspaceSettingsPanel workspaceId={workspace.id} />
          </div>
        </>
      );
    }
    return <ReviewTemplatesPanel workspaceId={workspace.id} />;
  };

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
      <div className="workspace-detail-page">
        {/* 手机端：使用 Select 下拉切换 */}
        {isMobile ? (
          <div className="mobile-tabs-container">
            <Select
              value={activeTab}
              onChange={(v) => setActiveTab(v)}
              style={{ width: '100%', marginBottom: 12 }}
              options={TAB_OPTIONS.map((opt) => ({
                value: opt.value,
                label: (
                  <span>
                    {opt.icon} {opt.label}
                  </span>
                ),
              }))}
            />
            <div className="mobile-tab-content">
              {renderTabContent(activeTab)}
            </div>
          </div>
        ) : (
          /* 桌面端：使用 Tabs */
          <Tabs
            activeKey={activeTab}
            onChange={(v) => setActiveTab(v as TabKey)}
            style={{ marginTop: 4 }}
            items={[
              {
                key: 'agents',
                label: (
                  <span>
                    <RobotOutlined /> 智能体
                  </span>
                ),
                children: renderTabContent('agents'),
              },
              {
                key: 'loop-settings',
                label: (
                  <span>
                    <SettingOutlined /> Loop设置
                  </span>
                ),
                children: renderTabContent('loop-settings'),
              },
            ]}
          />
        )}
      </div>
    </PageCard>
  );
}
