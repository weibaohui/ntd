// 「内置资源同步」卡:专家/模板/Skills 的 git 同步状态。
// 显示 git 是否可用、是否有更新、文件数与最后同步时间。
import { Tag } from 'antd';
import { SyncOutlined } from '@ant-design/icons';
import { bundledApi } from '@/api/bundled';
import { formatRelativeTime } from '@/utils/datetime';
import { useCardData } from '../useCardData';
import { CardShell } from './CardShell';

export function BundledSyncCard() {
  // getStatus() 默认 subdir='all',聚合三类内置资源。
  const { data, loading, error } = useCardData(() => bundledApi.getStatus());
  const gitOk = data?.git_available ?? false;
  // needs_update 可为 null(无法判断时),仅 true 才提示「有更新」。
  const needsUpdate = data?.needs_update === true;
  return (
    <CardShell icon={<SyncOutlined />} title="内置资源同步" loading={loading} error={error}>
      {data && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6, fontSize: 13 }}>
          <div>
            Git:{gitOk ? <Tag color="green">可用</Tag> : <Tag color="red">缺失</Tag>}
          </div>
          <div>
            同步:{needsUpdate ? <Tag color="orange">有更新</Tag> : <Tag color="blue">最新</Tag>}
          </div>
          <div>文件数:{data.subdir_file_count}</div>
          <div>最后同步:{data.last_sync_at ? formatRelativeTime(data.last_sync_at) : '-'}</div>
        </div>
      )}
    </CardShell>
  );
}
