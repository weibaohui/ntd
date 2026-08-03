// 飞书推送配置卡片：推送级别 + 单聊/群聊响应开关。

import { Card, Select, Switch } from 'antd';
import type { FeishuPushStatus } from '@/utils/database';

interface PushStatusCardProps {
  pushStatus: FeishuPushStatus;
  onPushLevelChange: (level: 'disabled' | 'result_only' | 'all') => void;
  onResponseEnabledChange: (targetType: 'p2p' | 'group', enabled: boolean) => void;
}

export function PushStatusCard({ pushStatus, onPushLevelChange, onResponseEnabledChange }: PushStatusCardProps) {
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
