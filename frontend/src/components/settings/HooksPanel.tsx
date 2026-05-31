import { useState, useEffect } from 'react';
import {
  Table, Button, Space, Modal, Form, Input, Select, Switch, message,
  Popconfirm, Tabs, Tag, Card, Row, Col, InputNumber, Typography, Divider,
} from 'antd';
import {
  PlusOutlined, DeleteOutlined, EditOutlined, PlayCircleOutlined,
  ReloadOutlined, ClearOutlined, CheckCircleOutlined, CloseCircleOutlined,
} from '@ant-design/icons';
import type { ColumnsType } from 'antd/es/table';
import * as db from '../../utils/database';
import type {
  HookRule, CreateHookRequest, UpdateHookRequest, GlobalHookConfig,
  HookLogEntry, HookFilter, HookAction,
} from '../../utils/database/hooks';
import { HOOK_TRIGGERS } from '../../utils/database/hooks';

const { Text } = Typography;

function HookFilterEditor({ value, onChange }: { value?: HookFilter; onChange?: (v: HookFilter) => void }) {
  const [form] = Form.useForm();
  useEffect(() => {
    form.setFieldsValue(value || { status: [], title_contains: undefined, tags: [], executor: undefined });
  }, [value, form]);

  return (
    <Form form={form} layout="vertical" onValuesChange={(_, all) => onChange?.(all as HookFilter)}>
      <Row gutter={16}>
        <Col span={12}>
          <Form.Item name="status" label="状态过滤">
            <Select mode="multiple" placeholder="任意状态" allowClear>
              <Select.Option value="pending">待处理</Select.Option>
              <Select.Option value="in_progress">进行中</Select.Option>
              <Select.Option value="completed">已完成</Select.Option>
              <Select.Option value="failed">失败</Select.Option>
            </Select>
          </Form.Item>
        </Col>
        <Col span={12}>
          <Form.Item name="title_contains" label="标题包含">
            <Input placeholder="不区分大小写" />
          </Form.Item>
        </Col>
      </Row>
      <Row gutter={16}>
        <Col span={12}>
          <Form.Item name="executor" label="执行人">
            <Input placeholder="例如 claude" />
          </Form.Item>
        </Col>
        <Col span={12}>
          <Form.Item name="tags" label="标签 ID">
            <Select mode="tags" placeholder="标签 ID" allowClear>
            </Select>
          </Form.Item>
        </Col>
      </Row>
    </Form>
  );
}

function HookActionEditor({ value, onChange }: { value?: HookAction; onChange?: (v: HookAction) => void }) {
  const [form] = Form.useForm();
  useEffect(() => {
    form.setFieldsValue(value || { command: '', args: [''], env: {}, timeout_secs: 30 });
  }, [value, form]);

  return (
    <Form form={form} layout="vertical" onValuesChange={(_, all) => onChange?.(all as HookAction)}>
      <Form.Item name="command" label="命令" rules={[{ required: true }]}>
        <Input placeholder="例如 /bin/echo" />
      </Form.Item>
      <Form.Item name="args" label="参数">
        <Select mode="tags" placeholder="参数（回车添加）">
        </Select>
      </Form.Item>
      <Form.Item name="env" label="环境变量">
        <Input.TextArea placeholder='{"KEY": "VALUE"} JSON 格式' rows={2} />
      </Form.Item>
      <Form.Item name="timeout_secs" label="超时秒数">
        <InputNumber min={1} max={3600} defaultValue={30} />
      </Form.Item>
    </Form>
  );
}

interface HookFormData {
  name: string;
  description?: string;
  enabled: boolean;
  trigger: string;
  filter?: HookFilter;
  action: HookAction;
  is_async: boolean;
}

function HookModal({
  open, hook, onClose, onSave,
}: {
  open: boolean;
  hook?: HookRule;
  onClose: () => void;
  onSave: (data: CreateHookRequest | UpdateHookRequest) => Promise<void>;
}) {
  const [form] = Form.useForm<HookFormData>();
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (open) {
      if (hook) {
        form.setFieldsValue({
          name: hook.name,
          description: hook.description || undefined,
          enabled: hook.enabled,
          trigger: hook.trigger,
          filter: hook.filter,
          action: hook.action,
          is_async: hook.is_async,
        });
      } else {
        form.setFieldsValue({
          name: '',
          description: '',
          enabled: true,
          trigger: 'before_create',
          filter: { status: [], title_contains: '', tags: [], executor: '' },
          action: { command: '', args: [], env: {}, timeout_secs: 30 },
          is_async: true,
        });
      }
    }
  }, [open, hook, form]);

  const handleOk = async () => {
    try {
      const values = await form.validateFields();
      setSaving(true);
      await onSave(values);
      onClose();
    } catch {
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      title={hook ? '编辑 Hook' : '创建 Hook'}
      open={open}
      onCancel={onClose}
      onOk={handleOk}
      width={700}
      confirmLoading={saving}
      okText="保存"
      cancelText="取消"
    >
      <Form form={form} layout="vertical">
        <Row gutter={16}>
          <Col span={16}>
            <Form.Item name="name" label="名称" rules={[{ required: true }]}>
              <Input placeholder="例如：创建时通知" />
            </Form.Item>
          </Col>
          <Col span={4}>
            <Form.Item name="enabled" label="启用" valuePropName="checked">
              <Switch />
            </Form.Item>
          </Col>
          <Col span={4}>
            <Form.Item name="is_async" label="异步" valuePropName="checked">
              <Switch />
            </Form.Item>
          </Col>
        </Row>
        <Form.Item name="description" label="描述">
          <Input placeholder="可选描述" />
        </Form.Item>
        <Form.Item name="trigger" label="触发器" rules={[{ required: true }]}>
          <Select>
            {HOOK_TRIGGERS.map(t => (
              <Select.Option key={t.value} value={t.value}>{t.label}</Select.Option>
            ))}
          </Select>
        </Form.Item>
        <Divider>过滤条件</Divider>
        <HookFilterEditor />
        <Divider>执行动作</Divider>
        <HookActionEditor />
      </Form>
    </Modal>
  );
}

function HookListTab() {
  const [hooks, setHooks] = useState<HookRule[]>([]);
  const [loading, setLoading] = useState(false);
  const [modalOpen, setModalOpen] = useState(false);
  const [editingHook, setEditingHook] = useState<HookRule | undefined>();
  const [testingId, setTestingId] = useState<number | null>(null);
  const [testingResult, setTestingResult] = useState<string | null>(null);

  const loadHooks = async () => {
    setLoading(true);
    try {
      const data = await db.getHooks();
      setHooks(data);
    } catch (e: any) {
      message.error('加载 hooks 失败: ' + e.message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { loadHooks(); }, []);

  const handleSave = async (data: CreateHookRequest | UpdateHookRequest) => {
    if (editingHook) {
      await db.updateHook(editingHook.id, data);
      message.success('Hook updated');
    } else {
      await db.createHook(data as CreateHookRequest);
      message.success('Hook created');
    }
    loadHooks();
  };

  const handleDelete = async (id: number) => {
    await db.deleteHook(id);
    message.success('Hook deleted');
    loadHooks();
  };

  const handleTest = async (hook: HookRule) => {
    setTestingId(hook.id);
    setTestingResult(null);
    try {
      const result = await db.testHook(hook.id);
      const success = result.success ? 'SUCCESS' : 'FAILED';
      const output = `Exit Code: ${result.exit_code}\nDuration: ${result.duration_ms}ms\n\nStdout:\n${result.stdout || '(empty)'}\n\nStderr:\n${result.stderr || '(empty)'}`;
      setTestingResult(`[${success}] ${output}`);
    } catch (e: any) {
      setTestingResult('Error: ' + e.message);
    } finally {
      setTestingId(null);
    }
  };

  const columns: ColumnsType<HookRule> = [
    { title: '名称', dataIndex: 'name', key: 'name' },
    {
      title: '触发器', dataIndex: 'trigger', key: 'trigger',
      render: (t: string) => HOOK_TRIGGERS.find(x => x.value === t)?.label || t,
    },
    {
      title: '启用', dataIndex: 'enabled', key: 'enabled',
      render: (v: boolean) => v ? <CheckCircleOutlined style={{ color: 'green' }} /> : <CloseCircleOutlined style={{ color: 'red' }} />,
    },
    {
      title: '异步', dataIndex: 'is_async', key: 'is_async',
      render: (v: boolean) => v ? <Tag>异步</Tag> : <Tag color="orange">同步</Tag>,
    },
    {
      title: '命令', dataIndex: ['action', 'command'], key: 'command',
      ellipsis: true,
    },
    {
      title: '操作', key: 'action', width: 200,
      render: (_, record) => (
        <Space>
          <Button size="small" icon={<PlayCircleOutlined />} loading={testingId === record.id}
            onClick={() => handleTest(record)}>测试</Button>
          <Button size="small" icon={<EditOutlined />} onClick={() => { setEditingHook(record); setModalOpen(true); }} />
          <Popconfirm title="确认删除？" onConfirm={() => handleDelete(record.id)}>
            <Button size="small" danger icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      ),
    },
  ];

  return (
    <div>
      <div style={{ marginBottom: 16 }}>
        <Button type="primary" icon={<PlusOutlined />} onClick={() => { setEditingHook(undefined); setModalOpen(true); }}>
          创建 Hook
        </Button>
        <Button icon={<ReloadOutlined />} onClick={loadHooks} style={{ marginLeft: 8 }}>刷新</Button>
      </div>
      <Table columns={columns} dataSource={hooks} rowKey="id" loading={loading} size="small" />
      {testingResult && (
        <Card title="测试结果" size="small" style={{ marginTop: 16 }}>
          <pre style={{ maxHeight: 300, overflow: 'auto', fontSize: 12 }}>{testingResult}</pre>
        </Card>
      )}
      <HookModal
        open={modalOpen}
        hook={editingHook}
        onClose={() => { setModalOpen(false); setEditingHook(undefined); }}
        onSave={handleSave}
      />
    </div>
  );
}

function GlobalConfigTab() {
  const [config, setConfig] = useState<GlobalHookConfig | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => { loadConfig(); }, []);

  const loadConfig = async () => {
    try {
      const data = await db.getGlobalHookConfig();
      setConfig(data);
    } catch (e: any) {
      message.error('加载配置失败: ' + e.message);
    }
  };

  const handleSave = async () => {
    if (!config) return;
    setSaving(true);
    try {
      await db.updateGlobalHookConfig(config);
      message.success('配置已保存');
    } catch (e: any) {
      message.error('保存失败: ' + e.message);
    } finally {
      setSaving(false);
    }
  };

  if (!config) return null;

  return (
    <Card>
      <Row gutter={16}>
        <Col span={8}>
          <Form.Item label="启用">
            <Switch checked={config.enabled} onChange={(v) => setConfig({ ...config, enabled: v })} />
          </Form.Item>
        </Col>
        <Col span={8}>
          <Form.Item label="默认超时秒数">
            <InputNumber
              value={config.default_timeout_secs}
              onChange={(v) => setConfig({ ...config, default_timeout_secs: v || 30 })}
              min={1} max={3600}
            />
          </Form.Item>
        </Col>
        <Col span={8}>
          <Form.Item label="最大并发数">
            <InputNumber
              value={config.max_concurrency}
              onChange={(v) => setConfig({ ...config, max_concurrency: v || 5 })}
              min={1} max={50}
            />
          </Form.Item>
        </Col>
      </Row>
      <Button type="primary" onClick={handleSave} loading={saving}>保存配置</Button>
    </Card>
  );
}

function LogsTab() {
  const [logs, setLogs] = useState<HookLogEntry[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [page, setPage] = useState(1);
  const [status, setStatus] = useState<string | undefined>();

  const loadLogs = async () => {
    setLoading(true);
    try {
      const data = await db.getHookLogs({ page, limit: 20, status });
      setLogs(data.logs);
      setTotal(data.total);
    } catch (e: any) {
      message.error('加载日志失败: ' + e.message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { loadLogs(); }, [page, status]);

  const handleClear = async () => {
    try {
      await db.clearHookLogs();
      message.success('Logs cleared');
      loadLogs();
    } catch (e: any) {
      message.error('清空失败: ' + e.message);
    }
  };

  const columns: ColumnsType<HookLogEntry> = [
    {
      title: '时间', dataIndex: 'created_at', key: 'created_at',
      render: (t: string) => new Date(t).toLocaleString(),
      width: 160,
    },
    { title: 'Hook 名称', dataIndex: 'hook_name', key: 'hook_name' },
    { title: '触发器', dataIndex: 'trigger', key: 'trigger' },
    { title: 'Todo ID', dataIndex: 'todo_id', key: 'todo_id' },
    {
      title: '状态', dataIndex: 'success', key: 'success',
      render: (v: boolean | null) => v ? <Tag color="green">成功</Tag> : <Tag color="red">失败</Tag>,
    },
    {
      title: '耗时', dataIndex: 'duration_ms', key: 'duration_ms',
      render: (v: number | null) => v ? `${v}ms` : '-',
    },
    {
      title: '退出码', dataIndex: 'exit_code', key: 'exit_code',
      render: (v: number | null) => v ?? '-',
    },
    { title: '错误', dataIndex: 'error_msg', key: 'error_msg', ellipsis: true },
  ];

  return (
    <div>
      <div style={{ marginBottom: 16 }}>
        <Select placeholder="按状态筛选" allowClear style={{ width: 150, marginRight: 8 }}
          onChange={(v) => { setStatus(v); setPage(1); }}>
          <Select.Option value="success">成功</Select.Option>
          <Select.Option value="failed">失败</Select.Option>
        </Select>
        <Button icon={<ReloadOutlined />} onClick={loadLogs}>刷新</Button>
        <Popconfirm title="确认清空所有日志？" onConfirm={handleClear}>
          <Button danger icon={<ClearOutlined />} style={{ marginLeft: 8 }}>清空全部</Button>
        </Popconfirm>
        <Text type="secondary" style={{ marginLeft: 16 }}>总计：{total}</Text>
      </div>
      <Table columns={columns} dataSource={logs} rowKey="id" loading={loading} size="small"
        pagination={{ current: page, pageSize: 20, total, onChange: setPage }} />
    </div>
  );
}

export function HooksPanel() {
  const tabItems = [
    { key: 'hooks', label: 'Hook 规则', children: <HookListTab /> },
    { key: 'config', label: '全局配置', children: <GlobalConfigTab /> },
    { key: 'logs', label: '执行日志', children: <LogsTab /> },
  ];

  return <Tabs items={tabItems} />;
}
