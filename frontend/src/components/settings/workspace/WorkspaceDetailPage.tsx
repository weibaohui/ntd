import { useState } from 'react';
import { Tabs, Button } from 'antd';
import { LeftOutlined, RobotOutlined, ThunderboltOutlined, SettingOutlined } from '@ant-design/icons';
import type { ProjectDirectory } from '@/utils/database';
import { WorkspaceAgentPanel } from './WorkspaceAgentPanel';
import { WorkspaceSlashCommandsPanel } from './WorkspaceSlashCommandsPanel';
import { WorkspaceSettingsPanel } from './WorkspaceSettingsPanel';

interface WorkspaceDetailPageProps {
  workspace: ProjectDirectory;
  onBack: () => void;
}

export function WorkspaceDetailPage({ workspace, onBack }: WorkspaceDetailPageProps) {
  const [activeTab, setActiveTab] = useState('agents');

  return (
    <div className="workspace-detail-page">
      <div className="detail-header">
        <Button
          type="text"
          size="small"
          icon={<LeftOutlined />}
          onClick={onBack}
          className="back-btn"
        />
        <h3 className="card-title">{workspace.name}</h3>
      </div>

      <div style={{ padding: '0 16px' }}>
        <span style={{ color: '#666', fontSize: 12 }}>{workspace.path}</span>
      </div>

      <Tabs
        activeKey={activeTab}
        onChange={setActiveTab}
        style={{ marginTop: 8 }}
        items={[
          {
            key: 'agents',
            label: (
              <span>
                <RobotOutlined style={{ marginRight: 6 }} />
                智能体
              </span>
            ),
            children: <WorkspaceAgentPanel workspaceId={workspace.id} />,
          },
          {
            key: 'slash-commands',
            label: (
              <span>
                <ThunderboltOutlined style={{ marginRight: 6 }} />
                斜杠命令
              </span>
            ),
            children: <WorkspaceSlashCommandsPanel workspaceId={workspace.id} />,
          },
          {
            key: 'settings',
            label: (
              <span>
                <SettingOutlined style={{ marginRight: 6 }} />
                设置
              </span>
            ),
            children: <WorkspaceSettingsPanel workspaceId={workspace.id} />,
          },
        ]}
      />
    </div>
  );
}
