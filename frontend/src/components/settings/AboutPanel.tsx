import { useState, useEffect } from 'react';
import { Spin, Card, Empty, Space, Typography } from 'antd';
import * as db from '../../utils/database';
import { ShareCard } from '../ShareCard';

const { Paragraph } = Typography;

export function AboutPanel() {
  const [versionInfo, setVersionInfo] = useState<{ version: string; git_sha: string; git_describe: string } | null>(null);
  const [versionLoading, setVersionLoading] = useState(false);

  useEffect(() => {
    setVersionLoading(true);
    db.getVersion()
      .then((info) => {
        setVersionInfo(info);
      })
      .catch(() => {})
      .finally(() => setVersionLoading(false));
  }, []);

  return (
    <Spin spinning={versionLoading}>
      <Card title="NTD 版本信息" style={{ maxWidth: 600 }}>
        {versionInfo ? (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
            <div>
              <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 4 }}>版本号</div>
              <div style={{ fontSize: 24, fontWeight: 700, fontFamily: 'monospace' }}>{versionInfo.version}</div>
            </div>
            <div style={{ borderTop: '1px solid var(--color-border-light)', paddingTop: 16 }}>
              <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 8 }}>详细信息</div>
              <Space direction="vertical" size={8}>
                <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                  <span style={{ fontWeight: 500, minWidth: 80 }}>Git SHA:</span>
                  <code style={{ background: 'var(--color-bg-elevated)', padding: '2px 8px', borderRadius: 4, fontFamily: 'monospace' }}>{versionInfo.git_sha}</code>
                </div>
                <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                  <span style={{ fontWeight: 500, minWidth: 80 }}>Git Tag:</span>
                  <code style={{ background: 'var(--color-bg-elevated)', padding: '2px 8px', borderRadius: 4, fontFamily: 'monospace' }}>{versionInfo.git_describe}</code>
                </div>
              </Space>
            </div>
            <div style={{ borderTop: '1px solid var(--color-border-light)', paddingTop: 16 }}>
              <Paragraph type="secondary" style={{ margin: 0 }}>
                NTD (Nothing Todo) 是一个 AI Todo 应用，支持 Claude Code 和 JoinAI 等多种执行器。
              </Paragraph>
            </div>
          </div>
        ) : (
          <Empty description="无法获取版本信息" />
        )}
      </Card>
      <ShareCard />
    </Spin>
  );
}
