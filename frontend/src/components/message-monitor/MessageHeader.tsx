import { Button, Space, Tag } from 'antd';
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
    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16 }}>
      <div>
        <h2 style={{ margin: 0, fontSize: 18 }}>{workspaceName} - 消息监控台</h2>
        <p style={{ margin: '4px 0 0', color: 'var(--color-text-secondary)', fontSize: 13 }}>
          实时查看和管理工作空间的消息记录
        </p>
      </div>

      <Space size="middle">
        {stats && (
          <Space size={16}>
            <Tag color="blue">今日消息: {stats.last_24h_messages}</Tag>
            <Tag color="green">已处理: {stats.processed}</Tag>
            <Tag color="orange">未处理: {stats.unprocessed}</Tag>
          </Space>
        )}

        <Space>
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
        </Space>
      </Space>
    </div>
  );
}
