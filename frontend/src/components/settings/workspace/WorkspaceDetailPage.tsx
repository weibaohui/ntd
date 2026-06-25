import { useState, useEffect } from 'react';
import { Tabs, Select, Button } from 'antd';
import { LeftOutlined, RobotOutlined, ThunderboltOutlined, SettingOutlined } from '@ant-design/icons';
import type { ProjectDirectory } from '@/utils/database';
import { WorkspaceAgentPanel } from './WorkspaceAgentPanel';
import { WorkspaceSlashCommandsPanel } from './WorkspaceSlashCommandsPanel';
import { WorkspaceSettingsPanel } from './WorkspaceSettingsPanel';

interface WorkspaceDetailPageProps {
  workspace: ProjectDirectory;
  onBack: () => void;
}

type TabKey = 'agents' | 'slash-commands' | 'settings';

const TAB_OPTIONS = [
  { value: 'agents' as TabKey, label: '智能体', icon: <RobotOutlined /> },
  { value: 'slash-commands' as TabKey, label: '斜杠命令', icon: <ThunderboltOutlined /> },
  { value: 'settings' as TabKey, label: '设置', icon: <SettingOutlined /> },
];

export function WorkspaceDetailPage({ workspace, onBack }: WorkspaceDetailPageProps) {
  const [activeTab, setActiveTab] = useState<TabKey>('agents');
  const [isMobile, setIsMobile] = useState(false);

  // 检测手机端：屏幕宽度 < 768px
  useEffect(() => {
    const checkMobile = () => {
      setIsMobile(window.innerWidth < 768);
    };
    checkMobile();
    window.addEventListener('resize', checkMobile);
    return () => window.removeEventListener('resize', checkMobile);
  }, []);

  const renderContent = () => {
    switch (activeTab) {
      case 'agents':
        return <WorkspaceAgentPanel workspaceId={workspace.id} />;
      case 'slash-commands':
        return <WorkspaceSlashCommandsPanel workspaceId={workspace.id} />;
      case 'settings':
        return <WorkspaceSettingsPanel workspaceId={workspace.id} />;
    }
  };

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

      {/* 手机端：使用 Select 下拉切换，平铺内容 */}
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
            {renderContent()}
          </div>
        </div>
      ) : (
        /* 桌面端：使用 Tabs */
        <Tabs
          activeKey={activeTab}
          onChange={(v) => setActiveTab(v as TabKey)}
          style={{ marginTop: 8 }}
          items={TAB_OPTIONS.map((opt) => ({
            key: opt.value,
            label: (
              <span>
                {opt.icon} {opt.label}
              </span>
            ),
            children: renderContent(),
          }))}
        />
      )}
    </div>
  );
}
