// 仪表盘新卡片通用的外壳:统一 title(图标+文案)、loading、error 三态。
// 把每张卡重复的「加载中 / 加载失败」分支收敛到一处,卡内只写成功态内容。
// className 复用 dashboard-card,享受 Dashboard.tsx 顶部注入的 hover 样式。
import type { ReactNode } from 'react';
import { Card, Spin, Empty } from 'antd';

interface CardShellProps {
  icon?: ReactNode;
  title: ReactNode;
  loading: boolean;
  error: boolean;
  children: ReactNode;
}

export function CardShell({ icon, title, loading, error, children }: CardShellProps) {
  return (
    <Card
      className="dashboard-card"
      title={<span style={{ display: 'flex', alignItems: 'center', gap: 8 }}>{icon}{title}</span>}
      style={{ borderRadius: 12 }}
    >
      {loading ? (
        <div style={{ textAlign: 'center', padding: 24 }}><Spin /></div>
      ) : error ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="加载失败" />
      ) : (
        children
      )}
    </Card>
  );
}
