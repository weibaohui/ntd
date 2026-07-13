// 飞书推送配置卡片：推送级别、推送目标（所有者）展示、单聊/群聊响应开关。

import { Card, Select, Switch, Input } from 'antd';
import type { FeishuPushStatus } from '@/utils/database';
import { CopyButton } from '@/components/CopyButton';

interface PushStatusCardProps {
  pushStatus: FeishuPushStatus;
  onPushLevelChange: (level: 'disabled' | 'result_only' | 'all') => void;
  onResponseEnabledChange: (targetType: 'p2p' | 'group', enabled: boolean) => void;
  /** 复制成功回调，由父组件展示 message.success */
  onCopySuccess?: (label: string) => void;
}

export function PushStatusCard({ pushStatus, onPushLevelChange, onResponseEnabledChange, onCopySuccess }: PushStatusCardProps) {
  return (
    <Card title="推送配置" size="small" style={{ marginBottom: 16 }}>
      {/* 推送目标下拉 */}
      <div style={{ marginBottom: 12, display: 'flex', alignItems: 'center', gap: 8 }}>
        <span style={{ fontSize: 13, width: 60 }}>推送目标</span>
        <Select
          size="small"
          value={pushStatus.push_level}
          onChange={onPushLevelChange}
          style={{ width: 90 }}
          options={[
            { value: 'disabled', label: '关闭' },
            { value: 'result_only', label: '仅结论' },
            { value: 'all', label: '全部' },
          ]}
        />
        <span style={{ fontSize: 12, color: 'var(--color-text-tertiary)', display: 'flex', alignItems: 'center', gap: 4 }}>
          <span style={{ color: 'var(--color-info)' }}>💡</span>
          推送目标为所有者，首次私聊自动设置
        </span>
      </div>

      {/* 推送目标展示行：所有者 open_id，自动捕获，只读 */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 4, marginBottom: 12 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
          <span style={{ fontSize: 12, width: 60, color: 'var(--color-text-tertiary)' }}>推送目标:</span>
          <Input size="small" value={pushStatus.owner_open_id || '未设置'} style={{ flex: 1, fontSize: 12 }} readOnly />
          <CopyButton type="text" size="small" text={pushStatus.owner_open_id || ''} onCopy={() => onCopySuccess?.('owner_open_id')} />
        </div>
      </div>

      {/* 响应开关 */}
      <div style={{ display: 'flex', gap: 16, fontSize: 13 }}>
        <span style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
          <Switch size="small" checked={pushStatus.p2p_response_enabled} onChange={v => onResponseEnabledChange('p2p', v)} />
          单聊响应
        </span>
        <span style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
          <Switch size="small" checked={pushStatus.group_response_enabled} onChange={v => onResponseEnabledChange('group', v)} />
          群聊响应
        </span>
      </div>
    </Card>
  );
}
