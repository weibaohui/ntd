import { Button, Segmented } from 'antd';
import { ReloadOutlined, UnorderedListOutlined, MessageOutlined } from '@ant-design/icons';

/** 统一刷新按钮组件 */
export const RefreshBtn = ({ onClick, size = 'small' }: { onClick: () => void; size?: 'small' | 'middle' }) => (
  <Button type="text" size={size} icon={<ReloadOutlined />} aria-label="刷新"
    onClick={(e) => { e.stopPropagation(); onClick(); }} />
);

/** 日志视图头部组件 */
export function LogViewHeader({ title, viewMode, onViewModeChange, onRefresh, fontSize = 12 }: {
  title: string;
  viewMode: 'log' | 'chat';
  onViewModeChange: (mode: 'log' | 'chat') => void;
  onRefresh: () => void;
  fontSize?: number;
}) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <span style={{ fontSize, fontWeight: 600, color: 'var(--color-primary)' }}>{title}</span>
        <RefreshBtn onClick={onRefresh} />
      </div>
      <Segmented
        size="small"
        value={viewMode}
        onChange={(value) => onViewModeChange(value as 'log' | 'chat')}
        options={[
          { value: 'log', icon: <UnorderedListOutlined />, label: '日志' },
          { value: 'chat', icon: <MessageOutlined />, label: '对话' },
        ]}
      />
    </div>
  );
}
