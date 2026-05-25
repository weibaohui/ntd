import { useState, useEffect, useMemo } from 'react';
import { Table, Tag, Select, Input, Empty, Spin, Space, Tooltip, message } from 'antd';
import Typography from 'antd/es/typography';
import { CheckCircleOutlined, SearchOutlined } from '@ant-design/icons';
import { EXECUTORS } from '../../types';
import type { SkillComparison } from '../../types';
import * as db from '../../utils/database';

const { Text } = Typography;

export function SkillsComparison() {
  const [loading, setLoading] = useState(true);
  const [data, setData] = useState<SkillComparison[]>([]);
  const [filter, setFilter] = useState<'all' | 'shared' | 'unique'>('all');
  const [searchText, setSearchText] = useState('');

  useEffect(() => {
    setLoading(true);
    db.getSkillsComparison()
      .then(setData)
      .catch(err => message.error('加载失败: ' + err.message))
      .finally(() => setLoading(false));
  }, []);

  const filtered = useMemo(() => {
    let result = data;
    if (searchText) {
      const lower = searchText.toLowerCase();
      result = result.filter(s =>
        s.skill_name.toLowerCase().includes(lower) ||
        s.description?.toLowerCase().includes(lower)
      );
    }
    if (filter === 'shared') {
      result = result.filter(s => {
        const presentCount = Object.values(s.executors).filter(e => e.present).length;
        return presentCount >= 2;
      });
    } else if (filter === 'unique') {
      result = result.filter(s => {
        const presentCount = Object.values(s.executors).filter(e => e.present).length;
        return presentCount === 1;
      });
    }
    return result;
  }, [data, filter, searchText]);

  const executorColumns = EXECUTORS.map(exec => ({
    title: (
      <Tooltip title={exec.label}>
        <span style={{ fontSize: 12, color: exec.color }}>{exec.label}</span>
      </Tooltip>
    ),
    key: exec.value,
    width: 80,
    align: 'center' as const,
    render: (_: unknown, record: SkillComparison) => {
      const presence = record.executors[exec.value];
      if (!presence?.present) {
        return <span style={{ color: '#d9d9d9' }}>-</span>;
      }
      return (
        <Tooltip title={presence.version ? `v${presence.version}` : '已安装'}>
          <CheckCircleOutlined style={{ color: exec.color, fontSize: 16 }} />
        </Tooltip>
      );
    },
  }));

  if (loading) {
    return <div style={{ textAlign: 'center', padding: 48 }}><Spin size="large" /></div>;
  }

  const sharedCount = data.filter(s => Object.values(s.executors).filter(e => e.present).length >= 2).length;
  const uniqueCount = data.filter(s => Object.values(s.executors).filter(e => e.present).length === 1).length;

  return (
    <div>
      <Space style={{ marginBottom: 16 }} wrap>
        <Input.Search
          placeholder="搜索 Skill"
          value={searchText}
          onChange={e => setSearchText(e.target.value)}
          style={{ width: 200 }}
          allowClear
          prefix={<SearchOutlined />}
        />
        <Select value={filter} onChange={setFilter} style={{ width: 140 }}>
          <Select.Option value="all">全部 ({data.length})</Select.Option>
          <Select.Option value="shared">共享 ({sharedCount})</Select.Option>
          <Select.Option value="unique">独有 ({uniqueCount})</Select.Option>
        </Select>
      </Space>

      {filtered.length === 0 ? (
        <Empty description="没有匹配的 Skills" />
      ) : (
        <Table
          dataSource={filtered}
          rowKey="skill_name"
          size="small"
          pagination={{ pageSize: 20 }}
          scroll={{ x: 900 }}
          columns={[
            {
              title: 'Skill',
              dataIndex: 'skill_name',
              width: 180,
              fixed: 'left',
              render: (name: string, record: SkillComparison) => {
                const presentCount = Object.values(record.executors).filter(e => e.present).length;
                const totalExecs = EXECUTORS.length;
                let tagColor = 'default';
                let tagLabel = '';
                if (presentCount >= 3) { tagColor = 'green'; tagLabel = '热门'; }
                else if (presentCount >= 2) { tagColor = 'blue'; tagLabel = '共享'; }
                else { tagColor = 'orange'; tagLabel = '独有'; }
                return (
                  <div>
                    <Text strong>{name}</Text>
                    <Tag color={tagColor} style={{ marginLeft: 4, fontSize: 10 }}>{tagLabel}</Tag>
                    <div style={{ marginTop: 2 }}>
                      <Text type="secondary" style={{ fontSize: 11 }}>
                        {presentCount}/{totalExecs} 执行器
                      </Text>
                    </div>
                  </div>
                );
              },
            },
            {
              title: '描述',
              dataIndex: 'description',
              width: 200,
              ellipsis: true,
              render: (desc: string) => (
                <Tooltip title={desc}>
                  <Text type="secondary" ellipsis style={{ fontSize: 12 }}>{desc || '-'}</Text>
                </Tooltip>
              ),
            },
            ...executorColumns,
          ]}
        />
      )}
    </div>
  );
}
