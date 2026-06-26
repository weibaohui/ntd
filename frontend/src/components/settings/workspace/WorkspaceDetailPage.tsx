import { useState } from 'react';
import { Tabs, Select, Button } from 'antd';
import { ArrowLeftOutlined, RobotOutlined, SettingOutlined } from '@ant-design/icons';
import { useIsMobile } from '@/hooks/useIsMobile';
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

  return (
    <div className="workspace-detail-page">
      <div className="detail-header">
        <Button
          type="text"
          size="small"
          icon={<ArrowLeftOutlined />}
          onClick={onBack}
          className="back-btn"
        />
        <h3 className="card-title">{workspace.name}</h3>
      </div>

      <div style={{ padding: '0 16px' }}>
        <span style={{ color: '#666', fontSize: 12 }}>{workspace.path}</span>
      </div>

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
            {activeTab === 'agents' && (
              <>
                <WorkspaceAgentPanel workspaceId={workspace.id} />
                <div style={{ marginTop: 24 }}>
                  <WorkspaceSlashCommandsPanel workspaceId={workspace.id} />
                </div>
                <div style={{ marginTop: 24 }}>
                  <WorkspaceSettingsPanel workspaceId={workspace.id} />
                </div>
              </>
            )}
            {activeTab === 'loop-settings' && <ReviewTemplatesPanel workspaceId={workspace.id} />}
          </div>
        </div>
      ) : (
        /* 桌面端：使用 Tabs */
        <Tabs
          activeKey={activeTab}
          onChange={(v) => setActiveTab(v as TabKey)}
          style={{ marginTop: 8 }}
          items={[
            {
              key: 'agents',
              label: (
                <span>
                  <RobotOutlined /> 智能体
                </span>
              ),
              children: (
                <>
                  <WorkspaceAgentPanel workspaceId={workspace.id} />
                  <div style={{ marginTop: 24 }}>
                    <WorkspaceSlashCommandsPanel workspaceId={workspace.id} />
                  </div>
                  <div style={{ marginTop: 24 }}>
                    <WorkspaceSettingsPanel workspaceId={workspace.id} />
                  </div>
                </>
              ),
            },
            {
              key: 'loop-settings',
              label: (
                <span>
                  <SettingOutlined /> Loop设置
                </span>
              ),
              children: <ReviewTemplatesPanel workspaceId={workspace.id} />,
            },
          ]}
        />
      )}
    </div>
  );
}
