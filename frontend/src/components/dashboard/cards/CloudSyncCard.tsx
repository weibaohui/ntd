// 「云同步」卡:云端同步的连接状态与最后同步时间。
import { Tag, Statistic } from 'antd';
import { CloudOutlined } from '@ant-design/icons';
import { getCloudSyncStatus } from '@/utils/database/sync';
import { formatRelativeTime } from '@/utils/datetime';
import { useCardData } from '@/components/dashboard/useCardData';
import { CardShell } from './CardShell';

export function CloudSyncCard() {
  const { data, loading, error } = useCardData(getCloudSyncStatus);
  const connected = data?.connected ?? false;
  return (
    <CardShell icon={<CloudOutlined />} title="云同步" loading={loading} error={error}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 8, fontSize: 13 }}>
        <div>
          状态:{connected ? <Tag color="green">已连接</Tag> : <Tag>未连接</Tag>}
        </div>
        <Statistic
          title="最后同步"
          value={data?.last_sync_at ? formatRelativeTime(data.last_sync_at) : '-'}
          valueStyle={{ fontSize: 14 }}
        />
      </div>
    </CardShell>
  );
}
