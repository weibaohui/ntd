import { Card, Row, Col, Statistic, Tag, Typography } from 'antd';
import { TeamOutlined, ClockCircleOutlined, ThunderboltOutlined, ApiOutlined, FilterOutlined } from '@ant-design/icons';
import type { SessionStats } from '../../utils/database';
import { formatTokens, sourceConfig } from './helpers';

const { Text } = Typography;

export function StatsCards({ stats }: { stats: SessionStats | null }) {
  if (!stats) return null;

  const sourceEntries = Object.entries(stats.by_source).sort((a, b) => b[1] - a[1]);

  return (
    <>
      <Row gutter={[12, 12]} style={{ marginBottom: 12 }}>
        <Col span={6}>
          <Card size="small" style={{ textAlign: 'center' }}>
            <Statistic
              title={<Text type="secondary" style={{ fontSize: 12 }}>总会话</Text>}
              value={stats.total_sessions}
              prefix={<TeamOutlined />}
              valueStyle={{ fontSize: 20 }}
            />
          </Card>
        </Col>
        <Col span={6}>
          <Card size="small" style={{ textAlign: 'center' }}>
            <Statistic
              title={<Text type="secondary" style={{ fontSize: 12 }}>活跃会话</Text>}
              value={stats.active_sessions}
              prefix={<ClockCircleOutlined />}
              valueStyle={{ fontSize: 20, color: '#52c41a' }}
            />
          </Card>
        </Col>
        <Col span={6}>
          <Card size="small" style={{ textAlign: 'center' }}>
            <Statistic
              title={<Text type="secondary" style={{ fontSize: 12 }}>今日新增</Text>}
              value={stats.today_sessions}
              prefix={<ThunderboltOutlined />}
              valueStyle={{ fontSize: 20, color: '#faad14' }}
            />
          </Card>
        </Col>
        <Col span={6}>
          <Card size="small" style={{ textAlign: 'center' }}>
            <Statistic
              title={<Text type="secondary" style={{ fontSize: 12 }}>总 Token</Text>}
              value={formatTokens(stats.total_input_tokens + stats.total_output_tokens)}
              prefix={<ApiOutlined />}
              valueStyle={{ fontSize: 20, color: '#1677ff' }}
            />
          </Card>
        </Col>
      </Row>
      {sourceEntries.length > 0 && (
        <div style={{ marginBottom: 12, display: 'flex', gap: 6, flexWrap: 'wrap', alignItems: 'center' }}>
          <FilterOutlined style={{ color: 'var(--color-text-secondary)' }} />
          <Text type="secondary" style={{ fontSize: 12 }}>工具分布：</Text>
          {sourceEntries.map(([source, count]) => {
            const cfg = sourceConfig[source] || { label: source, color: '#6b7280' };
            return (
              <Tag key={source} color={cfg.color} style={{ fontSize: 11, margin: 0 }}>
                {cfg.label} {count}
              </Tag>
            );
          })}
        </div>
      )}
    </>
  );
}
