import { useEffect, useState } from 'react';
import { Card, Table, Tag, Empty, Button, App, Segmented } from 'antd';
import { ReloadOutlined } from '@ant-design/icons';
import * as db from '../../utils/database';
import type { UsageStatsResponse } from '../../types';

interface UsageStatsCardProps {
  since?: string;
  until?: string;
}

type StatsTab = 'daily' | 'weekly' | 'monthly';

export function UsageStatsCard({ since, until }: UsageStatsCardProps) {
  const { message } = App.useApp();
  const [stats, setStats] = useState<UsageStatsResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [activeTab, setActiveTab] = useState<StatsTab>('daily');

  const loadStats = async () => {
    try {
      setLoading(true);
      const data = await db.getUsageStats(since, until);
      setStats(data);
    } catch {
      message.error('加载使用统计失败');
    } finally {
      setLoading(false);
    }
  };

  const handleRefresh = async () => {
    try {
      setRefreshing(true);
      const data = await db.refreshUsageStats();
      setStats(data);
      message.success('使用统计已刷新');
    } catch {
      message.error('刷新使用统计失败');
    } finally {
      setRefreshing(false);
    }
  };

  useEffect(() => {
    loadStats();
  }, [since, until]);

  const currentData = stats?.[activeTab] ?? [];
  const totals = currentData.reduce(
    (acc, d) => ({
      input: acc.input + d.input_tokens,
      output: acc.output + d.output_tokens,
      cost: acc.cost + d.total_cost,
    }),
    { input: 0, output: 0, cost: 0 }
  );

  // Get breakdowns for the selected period
  const currentBreakdowns = stats?.breakdowns ?? [];

  // Format number: if >= 10000 show as 万, otherwise show raw number
  const formatToken = (v: number) => {
    if (v >= 10000) {
      return `${(v / 10000).toFixed(1)}万`;
    }
    return v.toLocaleString();
  };

  const columns = [
    {
      title: '日期',
      dataIndex: 'date',
      key: 'date',
      width: 120,
    },
    {
      title: 'Input Tokens',
      dataIndex: 'input_tokens',
      key: 'input_tokens',
      width: 140,
      render: (v: number) => formatToken(v),
    },
    {
      title: 'Output Tokens',
      dataIndex: 'output_tokens',
      key: 'output_tokens',
      width: 140,
      render: (v: number) => formatToken(v),
    },
    {
      title: 'Cache Read',
      dataIndex: 'cache_read_tokens',
      key: 'cache_read_tokens',
      width: 120,
      render: (v: number) => formatToken(v),
    },
    {
      title: 'Cache Create',
      dataIndex: 'cache_creation_tokens',
      key: 'cache_creation_tokens',
      width: 120,
      render: (v: number) => formatToken(v),
    },
    {
      title: 'Cost',
      dataIndex: 'total_cost',
      key: 'total_cost',
      width: 100,
      render: (v: number) => `$${v.toFixed(4)}`,
    },
    {
      title: 'Models',
      dataIndex: 'models_used',
      key: 'models_used',
      render: (v: string[]) => v.slice(0, 3).map((m) => (
        <Tag key={m} style={{ marginInlineEnd: 4 }}>{m}</Tag>
      )),
    },
  ];

  const breakdownColumns = [
    {
      title: '日期',
      dataIndex: 'date',
      key: 'date',
      width: 120,
    },
    {
      title: 'Model',
      dataIndex: 'model_name',
      key: 'model_name',
      width: 150,
    },
    {
      title: 'Input Tokens',
      dataIndex: 'input_tokens',
      key: 'input_tokens',
      width: 140,
      render: (v: number) => formatToken(v),
    },
    {
      title: 'Output Tokens',
      dataIndex: 'output_tokens',
      key: 'output_tokens',
      width: 140,
      render: (v: number) => formatToken(v),
    },
    {
      title: 'Cache Read',
      dataIndex: 'cache_read_tokens',
      key: 'cache_read_tokens',
      width: 120,
      render: (v: number) => formatToken(v),
    },
    {
      title: 'Cache Create',
      dataIndex: 'cache_creation_tokens',
      key: 'cache_creation_tokens',
      width: 120,
      render: (v: number) => formatToken(v),
    },
    {
      title: 'Cost',
      dataIndex: 'cost',
      key: 'cost',
      width: 100,
      render: (v: number) => `$${v.toFixed(4)}`,
    },
  ];

  return (
    <Card
      title="Token 用量统计"
      extra={
        <Button
          icon={<ReloadOutlined />}
          size="small"
          loading={refreshing}
          onClick={handleRefresh}
        >
          刷新
        </Button>
      }
      style={{ borderRadius: 12 }}
      styles={{ body: { padding: '16px 20px' } }}
    >
      {loading ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="加载中..." />
      ) : stats ? (
        <>
          <div style={{ marginBottom: 12 }}>
            <Segmented
              value={activeTab}
              onChange={(v) => setActiveTab(v as StatsTab)}
              options={[
                { label: '日', value: 'daily' },
                { label: '周', value: 'weekly' },
                { label: '月', value: 'monthly' },
              ]}
              size="small"
            />
          </div>
          <div style={{ marginBottom: 16, display: 'flex', gap: 24 }}>
            <div>
              <div style={{ fontSize: 12, color: 'var(--color-text-tertiary)' }}>Input Tokens</div>
              <div style={{ fontSize: 20, fontWeight: 600 }}>{formatToken(totals.input)}</div>
            </div>
            <div>
              <div style={{ fontSize: 12, color: 'var(--color-text-tertiary)' }}>Output Tokens</div>
              <div style={{ fontSize: 20, fontWeight: 600 }}>{formatToken(totals.output)}</div>
            </div>
            <div>
              <div style={{ fontSize: 12, color: 'var(--color-text-tertiary)' }}>Total Cost</div>
              <div style={{ fontSize: 20, fontWeight: 600 }}>${totals.cost.toFixed(4)}</div>
            </div>
          </div>
          {currentData.length > 0 ? (
            <Table
              columns={columns}
              dataSource={currentData}
              rowKey="date"
              pagination={false}
              size="small"
              scroll={{ x: 'max-content' }}
              style={{ marginBottom: 24 }}
            />
          ) : (
            <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={`暂无${activeTab === 'daily' ? '日' : activeTab === 'weekly' ? '周' : '月'}度统计数据`} />
          )}

          {currentBreakdowns.length > 0 && (
            <>
              <div style={{ fontSize: 14, fontWeight: 500, marginBottom: 8 }}>模型维度统计</div>
              <Table
                columns={breakdownColumns}
                dataSource={currentBreakdowns}
                rowKey={(record) => `${record.date}-${record.model_name}`}
                pagination={false}
                size="small"
                scroll={{ x: 'max-content' }}
              />
            </>
          )}
        </>
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无统计数据" />
      )}
    </Card>
  );
}
