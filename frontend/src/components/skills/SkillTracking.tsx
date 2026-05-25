import { useState, useEffect, useMemo } from 'react';
import { Table, Tag, Select, Input, Button, Card, Row, Col, Statistic, Empty, Space, Tooltip, message } from 'antd';
import Typography from 'antd/es/typography';
import { SearchOutlined, ReloadOutlined } from '@ant-design/icons';
import { EXECUTORS } from '../../types';
import { formatTime } from './helpers';
import type { SkillInvocation } from '../../types';
import * as db from '../../utils/database';
import { useIsMobile } from '../../hooks/useIsMobile';

const { Text } = Typography;

export function SkillTracking() {
  const [loading, setLoading] = useState(true);
  const [invocations, setInvocations] = useState<SkillInvocation[]>([]);
  const [page, setPage] = useState(1);
  const [totalCount, setTotalCount] = useState(0);
  const [filterSkill, setFilterSkill] = useState<string | undefined>();
  const [filterExecutor, setFilterExecutor] = useState<string | undefined>();

  const loadData = async (p: number, skill?: string, executor?: string) => {
    setLoading(true);
    try {
      const data = await db.getSkillInvocations({
        page: p,
        limit: 20,
        skill_name: skill,
        executor,
      });
      setInvocations(data.items);
      setTotalCount(data.total);
    } catch (err: any) {
      message.error('加载失败: ' + err.message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { loadData(1); }, []);

  const handleRefresh = () => loadData(page, filterSkill, filterExecutor);

  const skillStats = useMemo(() => {
    const map = new Map<string, { count: number; executors: Set<string> }>();
    invocations.forEach(inv => {
      const s = map.get(inv.skill_name) || { count: 0, executors: new Set<string>() };
      s.count++;
      s.executors.add(inv.executor);
      map.set(inv.skill_name, s);
    });
    return Array.from(map.entries())
      .map(([name, data]) => ({ name, count: data.count, executorCount: data.executors.size }))
      .sort((a, b) => b.count - a.count);
  }, [invocations]);

  const isMobile = useIsMobile(640);

  return (
    <div>
      {skillStats.length > 0 && (
        <Row gutter={16} style={{ marginBottom: 16 }}>
          {skillStats.slice(0, isMobile ? 3 : 4).map(stat => (
            <Col xs={24} sm={12} md={6} key={stat.name}>
              <Card size="small">
                <Statistic
                  title={<Text ellipsis style={{ maxWidth: isMobile ? 80 : 120, fontSize: 12 }}>{stat.name}</Text>}
                  value={stat.count}
                  suffix="次"
                  valueStyle={{ fontSize: 18 }}
                />
                <Text type="secondary" style={{ fontSize: 10 }}>{stat.executorCount} 个执行器</Text>
              </Card>
            </Col>
          ))}
        </Row>
      )}

      <Card size="small" style={{ marginBottom: 16 }}>
        <Space wrap>
          <Input.Search
            placeholder="按 Skill 名称筛选"
            allowClear
            style={{ width: isMobile ? '100%' : 200 }}
            onSearch={v => { setFilterSkill(v || undefined); setPage(1); loadData(1, v || undefined, filterExecutor); }}
            prefix={<SearchOutlined />}
          />
          <Select
            placeholder="按执行器筛选"
            allowClear
            style={{ width: isMobile ? '100%' : 150 }}
            onChange={v => { setFilterExecutor(v || undefined); setPage(1); loadData(1, filterSkill, v || undefined); }}
          >
            {EXECUTORS.map(e => (
              <Select.Option key={e.value} value={e.value}>{e.label}</Select.Option>
            ))}
          </Select>
          <Button icon={<ReloadOutlined />} onClick={handleRefresh}>刷新</Button>
        </Space>
      </Card>

      {invocations.length === 0 ? (
        <Empty description="暂无调用记录" />
      ) : (
        <Table
          dataSource={invocations}
          rowKey="id"
          size="small"
          loading={loading}
          pagination={{
            current: page,
            pageSize: 20,
            total: totalCount,
            onChange: p => { setPage(p); loadData(p, filterSkill, filterExecutor); },
          }}
          columns={[
            {
              title: 'Skill',
              dataIndex: 'skill_name',
              width: 180,
              render: (name: string) => (
                <Text strong style={{ color: '#7C3AED' }}>{name}</Text>
              ),
            },
            {
              title: '执行器',
              dataIndex: 'executor',
              width: 120,
              render: (exec: string) => {
                const opt = EXECUTORS.find(e => e.value === exec.toLowerCase());
                return (
                  <Tag color={opt?.color || 'default'}>
                    {opt?.label || exec}
                  </Tag>
                );
              },
            },
            {
              title: '关联 Todo',
              dataIndex: 'todo_title',
              width: 200,
              ellipsis: true,
              render: (title: string | null, record: SkillInvocation) => (
                <Tooltip title={title || `Todo #${record.todo_id}`}>
                  <Text type="secondary" ellipsis>{title || `Todo #${record.todo_id}`}</Text>
                </Tooltip>
              ),
            },
            {
              title: '状态',
              dataIndex: 'status',
              width: 100,
              render: (status: string) => {
                const map: Record<string, { color: string; label: string }> = {
                  invoked: { color: 'processing', label: '已调用' },
                  completed: { color: 'success', label: '完成' },
                  failed: { color: 'error', label: '失败' },
                };
                const s = map[status] || { color: 'default', label: status };
                return <Tag color={s.color}>{s.label}</Tag>;
              },
            },
            {
              title: '耗时',
              dataIndex: 'duration_ms',
              width: 100,
              render: (ms: number | null) => ms != null ? `${(ms / 1000).toFixed(1)}s` : '-',
            },
            {
              title: '调用时间',
              dataIndex: 'invoked_at',
              width: 150,
              render: (t: string) => <Text type="secondary" style={{ fontSize: 12 }}>{formatTime(t)}</Text>,
            },
          ]}
        />
      )}
    </div>
  );
}
