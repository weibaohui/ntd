// 「工作空间」卡:工作空间数量。
import { Statistic } from 'antd';
import { FolderOutlined } from '@ant-design/icons';
import { getWorkspaces } from '@/utils/database/todos';
import { useCardData } from '@/components/dashboard/useCardData';
import { CardShell } from './CardShell';

export function WorkspaceCard() {
  const { data, loading, error } = useCardData(getWorkspaces);
  return (
    <CardShell icon={<FolderOutlined />} title="工作空间" loading={loading} error={error}>
      <Statistic title="工作空间" value={data?.length ?? 0} />
    </CardShell>
  );
}
