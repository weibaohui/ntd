// 任务管理页面：创建任务 + 任务列表（支持状态筛选）。

import { useEffect, useState } from 'react';
import { Card, Input, Button, Select, List, Tag, Typography, message, Space, Tabs } from 'antd';
import { PlusOutlined, ThunderboltOutlined, RocketOutlined } from '@ant-design/icons';
import bundledApi from '@/api/bundled';
import { useProjectDirectories } from '@/utils/workspaceDisplay';
import { TaskDetailPage } from '@/components/tasks/TaskDetailPage';

const { TextArea } = Input;
const { Title, Text } = Typography;

interface TaskItem {
  id: number; title: string; description: string; status: string;
  template_name?: string; complexity?: string;
  loop_id?: number; workspace_id?: number;
  latest_execution_status?: string; latest_execution_requirement?: string;
  created_at?: string;
}

export function TasksPage() {
  const { dirs: workspaces } = useProjectDirectories();
  const [tasks, setTasks] = useState<TaskItem[]>([]);
  const [requirement, setRequirement] = useState('');
  const [selectedWs, setSelectedWs] = useState<number | null>(null);
  const [selectedTemplate, setSelectedTemplate] = useState<string | undefined>();
  const [creating, setCreating] = useState(false);
  const [loading, setLoading] = useState(false);
  const [selectedTaskId, setSelectedTaskId] = useState<number | null>(null);
  const [statusFilter, setStatusFilter] = useState<string | undefined>();

  const load = async () => {
    setLoading(true);
    try { setTasks(await bundledApi.listTasks(statusFilter)); }
    catch { message.error('加载任务列表失败'); }
    finally { setLoading(false); }
  };

  useEffect(() => { load(); }, [statusFilter]);

  const handleCreate = async () => {
    if (!requirement.trim()) { message.warning('请输入需求描述'); return; }
    if (!selectedWs) { message.warning('请选择工作空间'); return; }
    setCreating(true);
    try {
      const r = await bundledApi.createTask(requirement, selectedWs, selectedTemplate || undefined);
      message.success(`任务已创建，执行 #${r.execution_id}`);
      setRequirement(''); load();
    } catch (e: any) { message.error(e?.message || '创建任务失败'); }
    finally { setCreating(false); }
  };

  if (selectedTaskId !== null) {
    return <TaskDetailPage taskId={selectedTaskId} onBack={() => { setSelectedTaskId(null); load(); }} />;
  }

  const statusColor = (s: string) => ({ pending: 'default', running: 'blue', success: 'green', failed: 'red' }[s] || 'default');
  const statusLabel = (s: string) => ({ pending: '待执行', running: '进行中', success: '已完成', failed: '失败' }[s] || s);

  return (
    <div style={{ padding: 24, maxWidth: 800, margin: '0 auto' }}>
      <Title level={3}><RocketOutlined style={{ marginRight: 8 }} />任务</Title>
      <Card title={<><PlusOutlined /> 新建任务</>} style={{ marginBottom: 24 }}>
        <TextArea placeholder="我想做什么？" value={requirement} onChange={e => setRequirement(e.target.value)} rows={3} style={{ marginBottom: 12 }} />
        <Space style={{ width: '100%', justifyContent: 'space-between' }}>
          <Space>
            <Select placeholder="工作空间" value={selectedWs} onChange={setSelectedWs}
              options={workspaces.map(ws => ({ label: `${ws.name}`, value: ws.id }))} style={{ minWidth: 160 }} />
            <Select placeholder="模板(可选)" value={selectedTemplate} onChange={setSelectedTemplate} allowClear style={{ minWidth: 160 }}
              options={[{ label: '轻量(Superpowers)', value: 'superpowers-task' }, { label: '标准(4P12S)', value: '4p12s-delivery' }, { label: '口头需求', value: 'oral-requirement' }, { label: '复杂(GienSpec)', value: 'gienspec-complex' }]} />
          </Space>
          <Button type="primary" icon={<ThunderboltOutlined />} loading={creating} onClick={handleCreate} disabled={!requirement.trim()}>创建任务</Button>
        </Space>
      </Card>

      <Tabs activeKey={statusFilter || 'all'} onChange={(k) => setStatusFilter(k === 'all' ? undefined : k)}
        items={[
          { key: 'all', label: '全部' }, { key: 'running', label: '进行中' }, { key: 'success', label: '已完成' }, { key: 'failed', label: '失败' },
        ]} />

      <List loading={loading} dataSource={tasks} locale={{ emptyText: '暂无任务' }}
        renderItem={t => (
          <List.Item onClick={() => setSelectedTaskId(t.id)} style={{ cursor: 'pointer' }}
            actions={[t.latest_execution_status && <Tag color={statusColor(t.latest_execution_status)} key="s">{t.latest_execution_status}</Tag>].filter(Boolean)}
          >
            <List.Item.Meta
              title={<Space><Text strong>{t.title}</Text>{t.complexity && <Tag>{t.complexity}</Tag>}<Tag color={statusColor(t.status)}>{statusLabel(t.status)}</Tag></Space>}
              description={t.latest_execution_requirement ? (t.latest_execution_requirement.length > 80 ? t.latest_execution_requirement.slice(0,80)+'…' : t.latest_execution_requirement) : t.description}
            />
            <Text type="secondary">{t.created_at?.slice(0,10)}</Text>
          </List.Item>
        )} />
    </div>
  );
}
