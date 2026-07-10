import { Button, Tag } from 'antd';
import { SettingOutlined, ReloadOutlined } from '@ant-design/icons';
import type { FeishuMessageStats } from '@/types';

interface MessageHeaderProps {
  workspaceName: string;
  stats: FeishuMessageStats | null;
  loading: boolean;
  onRefresh: () => void;
  onOpenConfig: () => void;
}

export function MessageHeader({ workspaceName, stats, loading, onRefresh, onOpenConfig }: MessageHeaderProps) {
  return (
    <div style={{ marginBottom: 16 }}>
      {/* 主标题行：手机端垂直布局，桌面端水平布局 */}
      <div style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        flexWrap: 'wrap',
        gap: 12,
      }}>
        <div style={{ minWidth: 0 }}>
          <h2 style={{ margin: 0, fontSize: 18, wordBreak: 'break-word' }}>{workspaceName} - 消息监控台</h2>
          <p style={{ margin: '4px 0 0', color: 'var(--color-text-secondary)', fontSize: 13 }}>
            实时查看和管理工作空间的消息记录
          </p>
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexShrink: 0 }}>
          <Button
            type="text"
            size="small"
            icon={<ReloadOutlined />}
            onClick={onRefresh}
            loading={loading}
          >
            刷新
          </Button>
          <Button
            type="text"
            size="small"
            icon={<SettingOutlined />}
            onClick={onOpenConfig}
          >
            配置
          </Button>
        </div>
      </div>

      {/* 统计标签行：独立一行，手机端可换行 */}
      {stats && (
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginTop: 12 }}>
          <Tag color="blue">今日消息: {stats.last_24h_messages}</Tag>
          <Tag color="green">已处理: {stats.processed}</Tag>
          <Tag color="orange">未处理: {stats.unprocessed}</Tag>
        </div>
      )}
    </div>
  );
}
