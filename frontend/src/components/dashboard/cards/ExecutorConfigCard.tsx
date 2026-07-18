// 「执行器配置」卡:已启用执行器数 + 默认执行器。
import { Tag } from 'antd';
import { ThunderboltOutlined } from '@ant-design/icons';
import { getExecutors } from '@/utils/database/skills';
import { useCardData } from '@/components/dashboard/useCardData';
import { CardShell } from './CardShell';

export function ExecutorConfigCard() {
  const { data, loading, error } = useCardData(getExecutors);
  const executors = data ?? [];
  const enabled = executors.filter((e) => e.enabled).length;
  const defaultExec = executors.find((e) => e.is_default);
  return (
    <CardShell icon={<ThunderboltOutlined />} title="执行器" loading={loading} error={error}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 6, fontSize: 13 }}>
        <div>
          启用：{enabled} / {executors.length}
        </div>
        <div>
          默认：
          {defaultExec ? (
            <Tag color="blue">{defaultExec.display_name || defaultExec.name}</Tag>
          ) : (
            <Tag>未设置</Tag>
          )}
        </div>
      </div>
    </CardShell>
  );
}
