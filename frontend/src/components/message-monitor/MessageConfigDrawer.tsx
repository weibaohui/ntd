import { Drawer, Divider } from 'antd';
import { WorkspaceSlashCommandsPanel } from '@/components/settings/workspace/WorkspaceSlashCommandsPanel';
import { DefaultResponseConfigPanel } from '@/components/settings/workspace/DefaultResponseConfigPanel';

interface MessageConfigDrawerProps {
  open: boolean;
  workspaceId: number;
  onClose: () => void;
  onChanged: () => void;
}

export function MessageConfigDrawer({ open, workspaceId, onClose, onChanged }: MessageConfigDrawerProps) {
  return (
    <Drawer
      title="智能助手配置"
      open={open}
      onClose={onClose}
      width={480}
      placement="right"
      destroyOnClose
    >
      <div style={{ padding: '8px 0' }}>
        <div style={{ marginBottom: 16 }}>
          <h4 style={{ margin: '0 0 12px', fontSize: 14, fontWeight: 500 }}>斜杠命令</h4>
          <WorkspaceSlashCommandsPanel
            workspaceId={workspaceId}
            onChanged={onChanged}
          />
        </div>

        <Divider style={{ margin: '16px 0' }} />

        <div style={{ marginBottom: 16 }}>
          <h4 style={{ margin: '0 0 12px', fontSize: 14, fontWeight: 500 }}>默认响应规则</h4>
          <DefaultResponseConfigPanel
            workspaceId={workspaceId}
            onChanged={onChanged}
          />
        </div>
      </div>
    </Drawer>
  );
}
