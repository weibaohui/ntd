// 「系统版本」卡:当前版本 vs 最新版本,提示是否可升级。
import { Tag } from 'antd';
import { InfoCircleOutlined } from '@ant-design/icons';
import { getVersion, getLatestVersion } from '@/utils/database/skills';
import { useCardData } from '../useCardData';
import { CardShell } from './CardShell';

interface VersionSnapshot {
  current: string;
  latest: string | null;
  latestError?: string;
}

// 当前版本与最新版本并发查询;最新版本走 npm,失败时携带 error 降级展示。
async function loadVersion(): Promise<VersionSnapshot> {
  const [v, latest] = await Promise.all([getVersion(), getLatestVersion()]);
  return { current: v.version, latest: latest.latest, latestError: latest.error };
}

export function VersionCard() {
  const { data, loading, error } = useCardData(loadVersion);
  // 可升级:最新版本非空且与当前不同。
  const canUpgrade = Boolean(data?.latest && data.latest !== data?.current);
  return (
    <CardShell icon={<InfoCircleOutlined />} title="系统版本" loading={loading} error={error}>
      {data && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6, fontSize: 13 }}>
          <div>
            当前:<strong>{data.current}</strong>
          </div>
          <div>
            最新：
            {data.latestError ? (
              <Tag color="warning">查询失败</Tag>
            ) : data.latest ? (
              <span>
                {data.latest} {canUpgrade && <Tag color="green">可升级</Tag>}
              </span>
            ) : (
              <Tag>已是最新</Tag>
            )}
          </div>
        </div>
      )}
    </CardShell>
  );
}
