// 工艺仪表盘：展示模板使用排行、推荐入口。
// 数据来源：GET /api/v1/processes/stats

import { useEffect, useState } from 'react';
import { Card, Col, Row, Spin, Typography, Table, Tag } from 'antd';
import { BarChartOutlined, TrophyOutlined } from '@ant-design/icons';
import bundledApi from '@/api/bundled';

const { Title } = Typography;

interface TemplateStat {
  name: string;
  display_name: string;
  complexity: string;
  loop_count: number;
}

interface ProcessStats {
  template_stats: TemplateStat[];
  total_templates: number;
}

export function ProcessDashboard() {
  const [stats, setStats] = useState<ProcessStats | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    const load = async () => {
      setLoading(true);
      try {
        const data = await bundledApi.getProcessStats();
        setStats(data);
      } catch {
        // 统计接口可选，失败静默。
      } finally {
        setLoading(false);
      }
    };
    load();
  }, []);

  if (loading) return <Spin style={{ display: 'block', margin: '40px auto' }} />;

  const statData = stats?.template_stats || [];

  const columns = [
    { title: '模板', dataIndex: 'display_name', key: 'name',
      render: (text: string, r: TemplateStat) => <>{r.name === r.display_name ? text : `${text} (${r.name})`}</>,
    },
    { title: '复杂度', dataIndex: 'complexity', key: 'complexity',
      render: (c: string) => <Tag color={c === 'light' ? 'green' : c === 'standard' ? 'blue' : 'red'}>{c}</Tag>,
    },
    { title: '安装次数', dataIndex: 'loop_count', key: 'count', sorter: (a: TemplateStat, b: TemplateStat) => b.loop_count - a.loop_count },
  ];

  // 取最大值作为进度条基准，fallback=1 防止空数据除零导致进度条崩坏。
  const maxCount = Math.max(...statData.map((s: TemplateStat) => s.loop_count), 1);

  return (
    <div style={{ padding: 24 }}>
      <Title level={4}><BarChartOutlined /> 工艺仪表盘</Title>

      <Row gutter={16} style={{ marginBottom: 24 }}>
        <Col span={8}>
          <Card>
            <Title level={3} style={{ margin: 0 }}>{stats?.total_templates ?? 0}</Title>
            <div>工艺模板总数</div>
          </Card>
        </Col>
        <Col span={8}>
          <Card>
            <Title level={3} style={{ margin: 0 }}>
              {statData.reduce((s: number, t: TemplateStat) => s + t.loop_count, 0)}
            </Title>
            <div>总安装次数</div>
          </Card>
        </Col>
        <Col span={8}>
          <Card>
            <Title level={3} style={{ margin: 0 }}>
              {/* 奖杯金色：写死的 #faad14 在暗色下偏刺眼，用主题警告色统一 */}
              <TrophyOutlined style={{ color: 'var(--color-warning)' }} /> {statData.length > 0 ? statData[0].display_name : '-'}
            </Title>
            <div>最受欢迎</div>
          </Card>
        </Col>
      </Row>

      <Card title="模板使用排行">
        <Table
          dataSource={statData}
          columns={columns}
          rowKey="name"
          pagination={false}
          size="small"
          expandable={{
            expandedRowRender: (r: TemplateStat) => (
              <div style={{ margin: 0 }}>
                {/* 简单进度条：轨道用主题三级填充色（暗色下不再是写死浅灰），
                    条体用主题主色；条内文字恒白（主色底两种主题下都够深） */}
                <div style={{ background: 'var(--color-fill-tertiary)', borderRadius: 4, height: 20, width: '100%' }}>
                  <div style={{
                    background: 'var(--color-primary)', borderRadius: 4, height: 20,
                    width: `${(r.loop_count / maxCount) * 100}%`, minWidth: 8,
                    display: 'flex', alignItems: 'center', justifyContent: 'center',
                    color: '#fff', fontSize: 12,
                  }}>
                    {r.loop_count > 0 ? r.loop_count : ''}
                  </div>
                </div>
              </div>
            ),
            rowExpandable: () => true,
          }}
          locale={{ emptyText: '暂无模板数据。执行 bundled sync 同步工艺模板后再查看。' }}
        />
      </Card>
    </div>
  );
}
