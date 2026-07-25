// 任务管理页面：创建任务（输入需求 → 推荐模板 → 一键创建 Loop）与任务列表。

import { useEffect, useState } from 'react';
import { Card, Input, Button, Select, List, Tag, Typography, message, Space } from 'antd';
import { PlusOutlined, ThunderboltOutlined, RocketOutlined, PlayCircleOutlined } from '@ant-design/icons';
import bundledApi from '@/api/bundled';
import { useProjectDirectories } from '@/utils/workspaceDisplay';
import { TaskDetailPage } from '@/components/tasks/TaskDetailPage';

const { TextArea } = Input;
const { Title, Text } = Typography;

interface TaskItem {
  loop_id: number;
  name: string;
  description: string;
  template_name?: string;
  complexity?: string;
  status: string;
  created_at?: string;
  workspace_id?: number;
  latest_execution_id?: number;
  latest_execution_status?: string;
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

  const load = async () => {
    setLoading(true);
    try { setTasks(await bundledApi.listTasks()); }
    catch { message.error('加载任务列表失败'); }
    finally { setLoading(false); }
  };

  useEffect(() => { load(); }, []);

  const handleCreate = async () => {
    if (!requirement.trim()) { message.warning('请输入需求描述'); return; }
    if (!selectedWs) { message.warning('请选择工作空间'); return; }
    setCreating(true);
    try {
      const result = await bundledApi.createTask(requirement, selectedWs, selectedTemplate || undefined);
      message.success(`任务已创建：${result.loop_name}（${result.phase_count} 阶段 ${result.step_count} 环节）`);
      setRequirement('');
      load();
    } catch (e: any) { message.error(e?.message || '创建任务失败'); }
    finally { setCreating(false); }
  };

  const complexityColor = (c?: string) => {
    switch (c) { case 'light': return 'green'; case 'standard': return 'blue'; case 'complex': return 'red'; default: return 'default'; }
  };

  const statusLabel = (s: string) => {
    switch (s) { case 'enabled': return '运行中'; case 'paused': return '已暂停'; default: return s; }
  };

  // 详情模式。
  if (selectedTaskId !== null) {
    return <TaskDetailPage taskId={selectedTaskId} onBack={() => setSelectedTaskId(null)} />;
  }

  return (
    <div style={{ padding: 24, maxWidth: 800, margin: '0 auto' }}>
      <Title level={3}><RocketOutlined style={{ marginRight: 8 }} />任务</Title>

      <Card title={<><PlusOutlined /> 新建任务</>} style={{ marginBottom: 24 }}>
        <TextArea
          placeholder="我想做什么？例如：用 React + TypeScript 做一个便利贴应用，支持创建、编辑、拖拽排序…"
          value={requirement}
          onChange={e => setRequirement(e.target.value)}
          rows={4}
          style={{ marginBottom: 12 }}
        />
        <Space style={{ width: '100%', justifyContent: 'space-between' }}>
          <Space>
            <Select
              placeholder="选择工作空间"
              value={selectedWs}
              onChange={setSelectedWs}
              options={workspaces.map(ws => ({ label: `${ws.name} (${ws.path})`, value: ws.id }))}
              style={{ minWidth: 220 }}
            />
            <Select
              placeholder="工艺模板（可选，不选自动推荐）"
              value={selectedTemplate}
              onChange={setSelectedTemplate}
              allowClear
              style={{ minWidth: 200 }}
              options={[
                { label: '轻量任务 (Superpowers)', value: 'superpowers-task' },
                { label: '标准交付 (4P12S)', value: '4p12s-delivery' },
                { label: '口头需求', value: 'oral-requirement' },
                { label: '复杂重构 (GienSpec)', value: 'gienspec-complex' },
              ]}
            />
          </Space>
          <Button
            type="primary"
            icon={<ThunderboltOutlined />}
            loading={creating}
            onClick={handleCreate}
            disabled={!requirement.trim()}
          >
            创建任务
          </Button>
        </Space>
      </Card>

      <Title level={4}>已有任务</Title>
      <List
        loading={loading}
        dataSource={tasks}
        locale={{ emptyText: '暂无任务。在上方输入需求创建第一个任务。' }}
        renderItem={t => (
          <List.Item
            actions={[
              t.latest_execution_id ? (
                <Button
                  type="primary" size="small"
                  icon={<PlayCircleOutlined />}
                  onClick={(e) => {
                    e.stopPropagation();
                    setSelectedTaskId(t.loop_id);
                  }}
                >
                  查看详情
                </Button>
              ) : null,
            ].filter(Boolean)}
            onClick={() => setSelectedTaskId(t.loop_id)}
            style={{ cursor: 'pointer' }}
          >
            <List.Item.Meta
              title={
                <Space>
                  <Text strong>{t.template_name || t.name}</Text>
                  {t.complexity && <Tag color={complexityColor(t.complexity)}>{t.complexity}</Tag>}
                  <Tag color={t.status === 'enabled' ? 'green' : 'default'}>{statusLabel(t.status)}</Tag>
                  {t.latest_execution_status && (
                    <Tag color={t.latest_execution_status === 'running' ? 'blue' : t.latest_execution_status === 'success' ? 'green' : 'red'}>
                      {t.latest_execution_status}
                    </Tag>
                  )}
                </Space>
              }
              description={t.description ? (t.description.length > 100 ? t.description.slice(0, 100) + '…' : t.description) : '(无描述)'}
            />
            <Text type="secondary">{t.created_at?.slice(0, 10)}</Text>
          </List.Item>
        )}
      />
    </div>
  );
}
