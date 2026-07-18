// 「工作空间」卡:项目目录数量。
import { Statistic } from 'antd';
import { FolderOutlined } from '@ant-design/icons';
import { getProjectDirectories } from '@/utils/database/todos';
import { useCardData } from '@/components/dashboard/useCardData';
import { CardShell } from './CardShell';

export function WorkspaceCard() {
  const { data, loading, error } = useCardData(getProjectDirectories);
  return (
    <CardShell icon={<FolderOutlined />} title="工作空间" loading={loading} error={error}>
      <Statistic title="项目目录" value={data?.length ?? 0} />
    </CardShell>
  );
}
