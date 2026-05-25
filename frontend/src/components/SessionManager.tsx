import { useState, useEffect, useCallback } from 'react';
import {
  Table, Tag, Space, Input, Select, Button, Popconfirm, Typography, Tooltip, message,
} from 'antd';
import {
  SearchOutlined, ReloadOutlined, EyeOutlined, DeleteOutlined,
} from '@ant-design/icons';
import * as db from '../utils/database';
import type { SessionInfo, SessionStats } from '../utils/database';
import { StatsCards } from './sessions/StatsCards';
import { SessionDetailDrawer } from './sessions/SessionDetailDrawer';
import { sourceTag, formatTokens, formatTime, shortId, sourceConfig } from './sessions/helpers';

const { Text } = Typography;

export function SessionManager() {
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [stats, setStats] = useState<SessionStats | null>(null);
  const [loading, setLoading] = useState(false);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [statusFilter, setStatusFilter] = useState<string | undefined>();
  const [sourceFilter, setSourceFilter] = useState<string | undefined>();
  const [searchText, setSearchText] = useState('');
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);

  const fetchSessions = useCallback(async () => {
    setLoading(true);
    try {
      const res = await db.listSessions({
        page, page_size: pageSize, status: statusFilter,
        source: sourceFilter, search: searchText || undefined,
      });
      setSessions(res.sessions);
      setTotal(res.total);
    } catch {
      // ignore
    } finally {
      setLoading(false);
    }
  }, [page, pageSize, statusFilter, sourceFilter, searchText]);

  const fetchStats = useCallback(async () => {
    try {
      const s = await db.getSessionStats();
      setStats(s);
    } catch {
      // ignore
    }
  }, []);

  useEffect(() => { fetchSessions(); }, [fetchSessions]);
  useEffect(() => { fetchStats(); }, [fetchStats]);

  const handleDelete = async (sessionId: string) => {
    try {
      await db.deleteSession(sessionId);
      message.success('已删除');
      fetchSessions();
      fetchStats();
    } catch (e: any) {
      message.error(e.message || '删除失败');
    }
  };

  const sourceOptions = stats
    ? Object.keys(stats.by_source).map((s) => {
        const cfg = sourceConfig[s] || { label: s };
        return { label: `${cfg.label} (${stats.by_source[s]})`, value: s };
      })
    : [];

  const columns = [
    {
      title: '状态', dataIndex: 'status', width: 60,
      render: (s: string) => (
        <Tooltip title={s === 'active' ? '活跃' : '已完成'}>
          <span style={{ display: 'inline-block', width: 8, height: 8, borderRadius: '50%', background: s === 'active' ? '#52c41a' : '#d9d9d9', boxShadow: s === 'active' ? '0 0 6px rgba(82, 196, 26, 0.5)' : 'none' }} />
        </Tooltip>
      ),
    },
    { title: '工具', dataIndex: 'source', width: 120, render: (source: string) => sourceTag(source) },
    { title: 'Session ID', dataIndex: 'session_id', width: 130, render: (id: string) => <Tooltip title={id}><Text code style={{ fontSize: 12 }}>{shortId(id)}</Text></Tooltip> },
    { title: '项目', dataIndex: 'project_path', width: 180, ellipsis: true, render: (p: string) => { const short = p.split('/').slice(-2).join('/'); return <Tooltip title={p}><Text style={{ fontSize: 12 }}>{short}</Text></Tooltip>; } },
    { title: '模型', dataIndex: 'model', width: 90, ellipsis: true, render: (m: string) => <Text style={{ fontSize: 12 }}>{m}</Text> },
    { title: '分支', dataIndex: 'git_branch', width: 85, ellipsis: true, render: (b: string | null) => b ? <Tag style={{ fontSize: 11 }}>{b}</Tag> : <Text type="secondary">-</Text> },
    { title: '消息', dataIndex: 'message_count', width: 55, align: 'center' as const, render: (n: number) => <Text style={{ fontSize: 12 }}>{n}</Text> },
    { title: 'Token', width: 75, align: 'right' as const, render: (_: unknown, r: SessionInfo) => { const total = r.total_input_tokens + r.total_output_tokens; if (total === 0) return <Text type="secondary" style={{ fontSize: 12 }}>-</Text>; return <Tooltip title={`输入: ${formatTokens(r.total_input_tokens)} / 输出: ${formatTokens(r.total_output_tokens)}`}><Text style={{ fontSize: 12 }}>{formatTokens(total)}</Text></Tooltip>; } },
    { title: '首条 Prompt', dataIndex: 'first_prompt', ellipsis: true, render: (p: string | null) => <Text type="secondary" style={{ fontSize: 12 }}>{p || '-'}</Text> },
    { title: '最后活跃', dataIndex: 'last_active_at', width: 110, render: (t: string | null) => <Tooltip title={t || ''}><Text style={{ fontSize: 12 }}>{formatTime(t)}</Text></Tooltip> },
    { title: '操作', width: 70, fixed: 'right' as const, render: (_: unknown, r: SessionInfo) => (
      <Space size={4}>
        <Button type="text" size="small" icon={<EyeOutlined />} onClick={(e) => { e.stopPropagation(); setSelectedSessionId(r.session_id); setDrawerOpen(true); }} />
        <Popconfirm title="确定删除该 Session？" description="将删除会话文件数据（不可恢复）" onConfirm={(e) => { e?.stopPropagation(); handleDelete(r.session_id); }} okText="删除" cancelText="取消">
          <Button type="text" size="small" danger icon={<DeleteOutlined />} onClick={(e) => e.stopPropagation()} />
        </Popconfirm>
      </Space>
    )},
  ];

  return (
    <div>
      <StatsCards stats={stats} />

      <div style={{ display: 'flex', gap: 8, marginBottom: 12, flexWrap: 'wrap' }}>
        <Input placeholder="搜索 Prompt 内容..." prefix={<SearchOutlined />} value={searchText} onChange={(e) => { setSearchText(e.target.value); setPage(1); }} style={{ width: 220 }} allowClear />
        <Select placeholder="工具来源" value={sourceFilter} onChange={(v) => { setSourceFilter(v); setPage(1); }} style={{ width: 170 }} allowClear options={sourceOptions} />
        <Select placeholder="状态" value={statusFilter} onChange={(v) => { setStatusFilter(v); setPage(1); }} style={{ width: 110 }} allowClear options={[{ label: '活跃', value: 'active' }, { label: '已完成', value: 'completed' }]} />
        <Button icon={<ReloadOutlined />} onClick={() => { fetchSessions(); fetchStats(); }}>刷新</Button>
      </div>

      <Table
        dataSource={sessions}
        columns={columns}
        rowKey="session_id"
        loading={loading}
        size="small"
        scroll={{ x: 1300 }}
        pagination={{ current: page, pageSize, total, showSizeChanger: true, showTotal: (t) => `共 ${t} 条`, onChange: (p, ps) => { setPage(p); setPageSize(ps); } }}
        onRow={(record) => ({ onClick: () => { setSelectedSessionId(record.session_id); setDrawerOpen(true); }, style: { cursor: 'pointer' } })}
      />

      <SessionDetailDrawer sessionId={selectedSessionId} open={drawerOpen} onClose={() => { setDrawerOpen(false); setSelectedSessionId(null); }} />
    </div>
  );
}
