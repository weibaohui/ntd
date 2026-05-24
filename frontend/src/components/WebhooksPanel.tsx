import { useState, useEffect, useMemo, useCallback } from 'react';
import {
  Table,
  Card,
  Space,
  Button,
  Switch,
  Modal,
  Form,
  Input,
  Select,
  message,
  Popconfirm,
  Tag,
  Descriptions,
  Tabs,
  Empty,
  Typography,
} from 'antd';
import {
  PlusOutlined,
  DeleteOutlined,
  EditOutlined,
  ApiOutlined,
  HistoryOutlined,
} from '@ant-design/icons';
import * as db from '../utils/database';
import type { Webhook, WebhookRecord } from '../utils/database';
import type { Todo } from '../types';

interface WebhooksPanelProps {
  todos: Todo[];
}

export function WebhooksPanel({ todos }: WebhooksPanelProps) {
  const [webhooks, setWebhooks] = useState<Webhook[]>([]);
  const [webhooksLoading, setWebhooksLoading] = useState(false);
  const [webhookFormOpen, setWebhookFormOpen] = useState(false);
  const [editingWebhook, setEditingWebhook] = useState<Webhook | null>(null);
  const [webhookForm] = Form.useForm();
  const [webhookFormSaving, setWebhookFormSaving] = useState(false);

  // Records state
  const [records, setRecords] = useState<WebhookRecord[]>([]);
  const [recordsLoading, setRecordsLoading] = useState(false);
  const [recordsTotal, setRecordsTotal] = useState(0);
  const [recordsPage, setRecordsPage] = useState(1);
  const [recordsPageSize, setRecordsPageSize] = useState(20);
  const [viewRecord, setViewRecord] = useState<WebhookRecord | null>(null);

  const loadWebhooks = async () => {
    setWebhooksLoading(true);
    try {
      const data = await db.getWebhooks();
      setWebhooks(data);
    } catch (e: any) {
      message.error('加载 webhook 失败: ' + (e?.message || String(e)));
    } finally {
      setWebhooksLoading(false);
    }
  };

  const loadRecords = async (page: number, pageSize: number) => {
    setRecordsLoading(true);
    try {
      const data = await db.getWebhookRecords({ offset: (page - 1) * pageSize, limit: pageSize });
      setRecords(data.records);
      setRecordsTotal(data.total);
    } catch (e: any) {
      message.error('加载记录失败: ' + (e?.message || String(e)));
    } finally {
      setRecordsLoading(false);
    }
  };

  useEffect(() => {
    loadWebhooks();
    loadRecords(1, recordsPageSize);
  }, []);

  const handleCreateWebhook = useCallback(() => {
    setEditingWebhook(null);
    webhookForm.resetFields();
    setWebhookFormOpen(true);
  }, [webhookForm]);

  const handleEditWebhook = useCallback((webhook: Webhook) => {
    setEditingWebhook(webhook);
    webhookForm.setFieldsValue({
      name: webhook.name,
      enabled: webhook.enabled,
      default_todo_id: webhook.default_todo_id,
    });
    setWebhookFormOpen(true);
  }, [webhookForm]);

  const handleSaveWebhook = useCallback(async () => {
    try {
      const values = await webhookForm.validateFields();
      setWebhookFormSaving(true);

      if (editingWebhook) {
        await db.updateWebhook(editingWebhook.id, values.name, values.enabled, values.default_todo_id);
        message.success('Webhook 已更新');
      } else {
        await db.createWebhook(values.name, values.enabled, values.default_todo_id);
        message.success('Webhook 已创建');
      }

      setWebhookFormOpen(false);
      loadWebhooks();
    } catch (e: any) {
      if (e?.errorFields) return; // Form validation error
      message.error('保存失败: ' + (e?.message || String(e)));
    } finally {
      setWebhookFormSaving(false);
    }
  }, [webhookForm, editingWebhook]);

  const handleDeleteWebhook = useCallback(async (id: number) => {
    try {
      await db.deleteWebhook(id);
      message.success('Webhook 已删除');
      loadWebhooks();
    } catch (e: any) {
      message.error('删除失败: ' + (e?.message || String(e)));
    }
  }, []);

  const handleToggleEnabled = useCallback(async (webhook: Webhook) => {
    try {
      await db.updateWebhook(webhook.id, webhook.name, !webhook.enabled, webhook.default_todo_id ?? undefined);
      loadWebhooks();
    } catch (e: any) {
      message.error('更新失败: ' + (e?.message || String(e)));
    }
  }, []);

  const baseUrl = window.location.origin;

  const webhookColumns = useMemo(() => [
    {
      title: '名称',
      dataIndex: 'name',
      key: 'name',
      width: 150,
    },
    {
      title: '状态',
      dataIndex: 'enabled',
      key: 'enabled',
      width: 80,
      render: (enabled: boolean, record: Webhook) => (
        <Switch checked={enabled} onChange={() => handleToggleEnabled(record)} size="small" />
      ),
    },
    {
      title: '默认触发 Todo',
      dataIndex: 'default_todo_id',
      key: 'default_todo_id',
      width: 200,
      render: (todoId: number | null) => {
        if (!todoId) return <span style={{ color: 'var(--color-text-tertiary)' }}>未设置</span>;
        const todo = todos.find(t => t.id === todoId);
        return todo ? todo.title : `Todo #${todoId}`;
      },
    },
    {
      title: '触发 URL',
      key: 'urls',
      width: 300,
      render: (_: any, record: Webhook) => (
        <Space direction="vertical" size={4}>
          <Typography.Text
            copyable={{ text: `${baseUrl}/webhook/trigger` }}
            style={{ fontSize: 11 }}
          >
            <code>{baseUrl}/webhook/trigger</code>
          </Typography.Text>
          {record.default_todo_id && (
            <Typography.Text
              copyable={{ text: `${baseUrl}/webhook/trigger/${record.default_todo_id}` }}
              style={{ fontSize: 11 }}
            >
              <code>{baseUrl}/webhook/trigger/{record.default_todo_id}</code>
            </Typography.Text>
          )}
        </Space>
      ),
    },
    {
      title: '创建时间',
      dataIndex: 'created_at',
      key: 'created_at',
      width: 160,
      render: (t: string) => t ? new Date(t).toLocaleString() : '-',
    },
    {
      title: '操作',
      key: 'actions',
      width: 120,
      render: (_: any, record: Webhook) => (
        <Space>
          <Button
            type="text"
            size="small"
            icon={<EditOutlined />}
            onClick={() => handleEditWebhook(record)}
          />
          <Popconfirm
            title="确定删除此 Webhook？"
            onConfirm={() => handleDeleteWebhook(record.id)}
          >
            <Button type="text" size="small" danger icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      ),
    },
  ], [todos, baseUrl, handleToggleEnabled, handleEditWebhook, handleDeleteWebhook]);

  const recordColumns = useMemo(() => [
    {
      title: '时间',
      dataIndex: 'created_at',
      key: 'created_at',
      width: 160,
      render: (t: string) => t ? new Date(t).toLocaleString() : '-',
    },
    {
      title: '方法',
      dataIndex: 'method',
      key: 'method',
      width: 70,
      render: (m: string) => <Tag color={m === 'GET' ? 'blue' : 'green'}>{m}</Tag>,
    },
    {
      title: '路径',
      dataIndex: 'path',
      key: 'path',
      width: 180,
      ellipsis: true,
    },
    {
      title: 'Webhook',
      dataIndex: 'webhook_name',
      key: 'webhook_name',
      width: 120,
      render: (n: string | null) => n || '-',
    },
    {
      title: '触发 Todo',
      dataIndex: 'triggered_todo_title',
      key: 'triggered_todo_title',
      width: 150,
      ellipsis: true,
      render: (t: string | null, record: WebhookRecord) => {
        if (!record.triggered_todo_id) return <span style={{ color: 'var(--color-text-tertiary)' }}>未找到</span>;
        return t || `Todo #${record.triggered_todo_id}`;
      },
    },
    {
      title: '状态',
      dataIndex: 'status_code',
      key: 'status_code',
      width: 80,
      render: (code: number | null) => {
        if (!code) return '-';
        return <Tag color={code >= 200 && code < 300 ? 'success' : code >= 400 ? 'error' : 'warning'}>{code}</Tag>;
      },
    },
    {
      title: '操作',
      key: 'actions',
      width: 80,
      render: (_: any, record: WebhookRecord) => (
        <Button type="text" size="small" onClick={() => setViewRecord(record)}>
          详情
        </Button>
      ),
    },
  ], [setViewRecord]);

  return (
    <div>
      <Tabs
        defaultActiveKey="webhooks"
        items={[
          {
            key: 'webhooks',
            label: (
              <span>
                <ApiOutlined style={{ marginRight: 6 }} />
                Webhook 配置
              </span>
            ),
            children: (
              <Card
                title="Webhook 列表"
                extra={
                  <Button type="primary" icon={<PlusOutlined />} onClick={handleCreateWebhook}>
                    添加 Webhook
                  </Button>
                }
                style={{ marginBottom: 16 }}
              >
                <Table
                  dataSource={webhooks}
                  columns={webhookColumns}
                  rowKey="id"
                  loading={webhooksLoading}
                  pagination={false}
                  size="small"
                />
                {webhooks.length === 0 && !webhooksLoading && (
                  <Empty description="暂无 Webhook，点击添加按钮创建" style={{ marginTop: 24 }} />
                )}
              </Card>
            ),
          },
          {
            key: 'records',
            label: (
              <span>
                <HistoryOutlined style={{ marginRight: 6 }} />
                调用记录
              </span>
            ),
            children: (
              <Card title="Webhook 调用记录">
                <Table
                  dataSource={records}
                  columns={recordColumns}
                  rowKey="id"
                  loading={recordsLoading}
                  size="small"
                  pagination={{
                    current: recordsPage,
                    pageSize: recordsPageSize,
                    total: recordsTotal,
                    onChange: (page, size) => {
                      setRecordsPage(page);
                      setRecordsPageSize(size);
                      loadRecords(page, size);
                    },
                    showSizeChanger: true,
                    pageSizeOptions: ['20', '50', '100'],
                  }}
                />
                {records.length === 0 && !recordsLoading && (
                  <Empty description="暂无调用记录" style={{ marginTop: 24 }} />
                )}
              </Card>
            ),
          },
        ]}
      />

      <Modal
        title={editingWebhook ? '编辑 Webhook' : '添加 Webhook'}
        open={webhookFormOpen}
        onOk={handleSaveWebhook}
        onCancel={() => setWebhookFormOpen(false)}
        confirmLoading={webhookFormSaving}
        width={500}
      >
        <Form form={webhookForm} layout="vertical" style={{ marginTop: 16 }}>
          <Form.Item
            name="name"
            label="名称"
            rules={[{ required: true, message: '请输入 Webhook 名称' }]}
          >
            <Input placeholder="例如: 默认 Webhook" />
          </Form.Item>
          <Form.Item
            name="enabled"
            label="启用状态"
            valuePropName="checked"
            initialValue={true}
          >
            <Switch />
          </Form.Item>
          <Form.Item
            name="default_todo_id"
            label="默认触发 Todo"
            tooltip="当使用 /webhook/trigger 路径时触发的 Todo"
          >
            <Select
              allowClear
              placeholder="选择默认触发的 Todo"
              showSearch
              filterOption={(input, option) =>
                (option?.label ?? '').toLowerCase().includes(input.toLowerCase())
              }
              options={todos.map(t => ({
                value: t.id,
                label: t.title,
              }))}
            />
          </Form.Item>
        </Form>
      </Modal>

      <Modal
        title="调用记录详情"
        open={!!viewRecord}
        onCancel={() => setViewRecord(null)}
        footer={null}
        width={600}
      >
        {viewRecord && (
          <Descriptions column={1} size="small" style={{ marginTop: 16 }}>
            <Descriptions.Item label="记录 ID">{viewRecord.id}</Descriptions.Item>
            <Descriptions.Item label="时间">{viewRecord.created_at ? new Date(viewRecord.created_at).toLocaleString() : '-'}</Descriptions.Item>
            <Descriptions.Item label="方法">{viewRecord.method}</Descriptions.Item>
            <Descriptions.Item label="路径">{viewRecord.path}</Descriptions.Item>
            <Descriptions.Item label="Content-Type">{viewRecord.content_type || '-'}</Descriptions.Item>
            <Descriptions.Item label="Webhook">{viewRecord.webhook_name || '-'}</Descriptions.Item>
            <Descriptions.Item label="触发 Todo">
              {viewRecord.triggered_todo_title || (viewRecord.triggered_todo_id ? `Todo #${viewRecord.triggered_todo_id}` : '-')}
            </Descriptions.Item>
            <Descriptions.Item label="状态码">
              {viewRecord.status_code && <Tag color={viewRecord.status_code >= 200 && viewRecord.status_code < 300 ? 'success' : 'error'}>{viewRecord.status_code}</Tag>}
            </Descriptions.Item>
            {viewRecord.query_params && (
              <Descriptions.Item label="Query 参数">
                <pre style={{ background: 'var(--color-fill-quaternary)', padding: 8, borderRadius: 4, fontSize: 12, maxHeight: 150, overflow: 'auto' }}>
                  {viewRecord.query_params}
                </pre>
              </Descriptions.Item>
            )}
            {viewRecord.body && (
              <Descriptions.Item label="请求体">
                <pre style={{ background: 'var(--color-fill-quaternary)', padding: 8, borderRadius: 4, fontSize: 12, maxHeight: 200, overflow: 'auto', whiteSpace: 'pre-wrap' }}>
                  {viewRecord.body}
                </pre>
              </Descriptions.Item>
            )}
            {viewRecord.response_body && (
              <Descriptions.Item label="响应体">
                <pre style={{ background: 'var(--color-fill-quaternary)', padding: 8, borderRadius: 4, fontSize: 12, maxHeight: 200, overflow: 'auto', whiteSpace: 'pre-wrap' }}>
                  {viewRecord.response_body}
                </pre>
              </Descriptions.Item>
            )}
          </Descriptions>
        )}
      </Modal>
    </div>
  );
}
