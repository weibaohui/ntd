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
  Tooltip,
} from 'antd';
import {
  PlusOutlined,
  DeleteOutlined,
  EditOutlined,
  ApiOutlined,
  HistoryOutlined,
  LinkOutlined,
  CopyOutlined,
} from '@ant-design/icons';
import * as db from '@/utils/database';
import type { Webhook, WebhookRecord } from '@/utils/database';
import { copyToClipboard } from '@/utils/clipboard';
import type { Todo } from '@/types';
import { useIsMobile } from '@/hooks/useIsMobile';

interface WebhooksPanelProps {
  todos: Todo[];
}

/** 单个 URL 的复制块：移动端省略中间，桌面端显示全文（仍可复制） */
function CopyableUrl({ url, compact }: { url: string; compact: boolean }) {
  const display = useMemo(() => {
    if (!compact) return url;
    // 移动端：保留协议+域名+末尾路径关键部分，中间省略
    try {
      const u = new URL(url);
      const path = u.pathname;
      if (path.length <= 14) return `${u.host}${path}`;
      const head = path.slice(0, 8);
      const tail = path.slice(-6);
      return `${u.host}${head}…${tail}`;
    } catch {
      // 退化为简单截断
      if (url.length <= 28) return url;
      return `${url.slice(0, 14)}…${url.slice(-8)}`;
    }
  }, [url, compact]);

  return (
    <Tooltip title={url} mouseEnterDelay={0.4}>
      <Button
        type="text"
        size="small"
        icon={<LinkOutlined />}
        onClick={async () => {
          // 使用统一的复制工具（兼容 HTTP 环境）
          const ok = await copyToClipboard(url);
          if (ok) {
            message.success('已复制 URL');
          } else {
            message.error('复制失败，请手动复制');
          }
        }}
        style={{
          padding: compact ? '0 6px' : '0 8px',
          height: compact ? 24 : 28,
          fontSize: compact ? 11 : 12,
          fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
          maxWidth: '100%',
        }}
      >
        <span
          style={{
            display: 'inline-block',
            maxWidth: compact ? 180 : 240,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
            verticalAlign: 'bottom',
          }}
        >
          {display}
        </span>
        <CopyOutlined style={{ fontSize: 11, marginLeft: 4, opacity: 0.6 }} />
      </Button>
    </Tooltip>
  );
}

/** 移动端单条 webhook 卡片 */
function WebhookCard({
  webhook,
  todo,
  baseUrl,
  onEdit,
  onDelete,
  onToggle,
}: {
  webhook: Webhook;
  todo: Todo | undefined;
  baseUrl: string;
  onEdit: () => void;
  onDelete: () => void;
  onToggle: () => void;
}) {
  // 根据类型生成触发 URL
  const triggerUrl = webhook.webhook_type === 'todo' && webhook.default_todo_id
    ? `${baseUrl}/webhook/trigger/todo/${webhook.default_todo_id}`
    : webhook.webhook_type === 'loop' && webhook.loop_id
    ? `${baseUrl}/webhook/trigger/loop/${webhook.loop_id}`
    : null;

  return (
    <Card
      size="small"
      style={{ marginBottom: 12 }}
      styles={{ body: { padding: 12 } }}
    >
      {/* 顶部：名称 + 类型标签 + 启用开关 + 操作 */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: 8,
          marginBottom: 8,
        }}
      >
        <div
          style={{
            fontWeight: 500,
            fontSize: 14,
            flex: 1,
            minWidth: 0,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}
          title={webhook.name}
        >
          {webhook.name}
          <Tag
            color={webhook.webhook_type === 'loop' ? 'purple' : 'blue'}
            style={{ marginLeft: 6, fontSize: 10, lineHeight: '16px' }}
          >
            {webhook.webhook_type === 'loop' ? 'Loop' : 'Todo'}
          </Tag>
        </div>
        <Space size={4} style={{ flexShrink: 0 }}>
          <Switch
            checked={webhook.enabled}
            onChange={onToggle}
            size="small"
          />
          <Button
            type="text"
            size="small"
            icon={<EditOutlined />}
            onClick={onEdit}
            aria-label="编辑"
          />
          <Popconfirm
            title="确定删除此 Webhook？"
            onConfirm={onDelete}
            okType="danger"
          >
            <Button
              type="text"
              size="small"
              danger
              icon={<DeleteOutlined />}
              aria-label="删除"
            />
          </Popconfirm>
        </Space>
      </div>

      {/* 关联目标 */}
      <div
        style={{
          fontSize: 12,
          color: 'var(--color-text-secondary)',
          marginBottom: 6,
          display: 'flex',
          gap: 4,
        }}
      >
        <span style={{ flexShrink: 0 }}>
          {webhook.webhook_type === 'loop' ? '关联 Loop：' : '默认 Todo：'}
        </span>
        <span
          style={{
            flex: 1,
            minWidth: 0,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}
          title={webhook.webhook_type === 'loop'
            ? (webhook.loop_id ? `Loop #${webhook.loop_id}` : '')
            : (todo?.title || (webhook.default_todo_id ? `Todo #${webhook.default_todo_id}` : ''))}
        >
          {webhook.webhook_type === 'loop'
            ? (webhook.loop_id ? `Loop #${webhook.loop_id}` : <span style={{ opacity: 0.5 }}>未设置</span>)
            : (webhook.default_todo_id
              ? (todo?.title || `Todo #${webhook.default_todo_id}`)
              : <span style={{ opacity: 0.5 }}>未设置</span>)}
        </span>
      </div>

      {/* URL 区域 */}
      {triggerUrl ? (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 4, marginBottom: 6 }}>
          <CopyableUrl url={triggerUrl} compact />
        </div>
      ) : (
        <div
          style={{
            fontSize: 11,
            color: 'var(--color-text-tertiary)',
            marginBottom: 6,
            padding: '4px 0',
          }}
        >
          未配置目标，无法生成触发 URL
        </div>
      )}

      {/* 创建时间 */}
      <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>
        创建：{webhook.created_at ? new Date(webhook.created_at).toLocaleString() : '-'}
      </div>
    </Card>
  );
}

export function WebhooksPanel({ todos }: WebhooksPanelProps) {
  const isMobile = useIsMobile();
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
      webhook_type: webhook.webhook_type,
      default_todo_id: webhook.default_todo_id,
      loop_id: webhook.loop_id,
    });
    setWebhookFormOpen(true);
  }, [webhookForm]);

  const handleSaveWebhook = useCallback(async () => {
    try {
      const values = await webhookForm.validateFields();
      setWebhookFormSaving(true);

      if (editingWebhook) {
        await db.updateWebhook(
          editingWebhook.id,
          values.name,
          values.enabled,
          values.webhook_type,
          values.webhook_type === 'todo' ? values.default_todo_id : undefined,
          values.webhook_type === 'loop' ? values.loop_id : undefined,
        );
        message.success('Webhook 已更新');
      } else {
        await db.createWebhook(
          values.name,
          values.enabled,
          values.webhook_type,
          values.webhook_type === 'todo' ? values.default_todo_id : undefined,
          values.webhook_type === 'loop' ? values.loop_id : undefined,
        );
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
      await db.updateWebhook(
        webhook.id,
        webhook.name,
        !webhook.enabled,
        webhook.webhook_type,
        webhook.default_todo_id ?? undefined,
        webhook.loop_id ?? undefined,
      );
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
      title: '类型',
      dataIndex: 'webhook_type',
      key: 'webhook_type',
      width: 80,
      render: (type: 'todo' | 'loop') => (
        <Tag color={type === 'loop' ? 'purple' : 'blue'}>
          {type === 'loop' ? 'Loop' : 'Todo'}
        </Tag>
      ),
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
      title: '关联目标',
      dataIndex: 'default_todo_id',
      key: 'target',
      width: 200,
      render: (_: any, record: Webhook) => {
        if (record.webhook_type === 'loop') {
          return record.loop_id ? `Loop #${record.loop_id}` : <span style={{ color: 'var(--color-text-tertiary)' }}>未设置</span>;
        }
        if (!record.default_todo_id) return <span style={{ color: 'var(--color-text-tertiary)' }}>未设置</span>;
        const todo = todos.find(t => t.id === record.default_todo_id);
        return todo ? todo.title : `Todo #${record.default_todo_id}`;
      },
    },
    {
      title: '触发 URL',
      key: 'urls',
      width: 320,
      render: (_: any, record: Webhook) => {
        const url = record.webhook_type === 'loop' && record.loop_id
          ? `${baseUrl}/webhook/trigger/loop/${record.loop_id}`
          : record.default_todo_id
          ? `${baseUrl}/webhook/trigger/todo/${record.default_todo_id}`
          : null;
        if (!url) {
          return (
            <span style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>
              请先配置目标
            </span>
          );
        }
        return <CopyableUrl url={url} compact={false} />;
      },
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
  ], []);

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
                  <Button
                    type="primary"
                    icon={<PlusOutlined />}
                    onClick={handleCreateWebhook}
                    size={isMobile ? 'small' : 'middle'}
                  >
                    {isMobile ? '添加' : '添加 Webhook'}
                  </Button>
                }
                style={{ marginBottom: 16 }}
                styles={{ body: { padding: isMobile ? 12 : 24 } }}
              >
                {isMobile ? (
                  <>
                    {webhooks.map(wh => (
                      <WebhookCard
                        key={wh.id}
                        webhook={wh}
                        todo={todos.find(t => t.id === wh.default_todo_id)}
                        baseUrl={baseUrl}
                        onEdit={() => handleEditWebhook(wh)}
                        onDelete={() => handleDeleteWebhook(wh.id)}
                        onToggle={() => handleToggleEnabled(wh)}
                      />
                    ))}
                    {webhooks.length === 0 && !webhooksLoading && (
                      <Empty description="暂无 Webhook，点击添加按钮创建" style={{ marginTop: 24 }} />
                    )}
                    {webhooksLoading && webhooks.length === 0 && (
                      <div style={{ textAlign: 'center', padding: 24, color: 'var(--color-text-tertiary)' }}>
                        加载中...
                      </div>
                    )}
                  </>
                ) : (
                  <>
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
                  </>
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
              <Card title="Webhook 调用记录" styles={{ body: { padding: isMobile ? 12 : 24 } }}>
                <Table
                  dataSource={records}
                  columns={recordColumns}
                  rowKey="id"
                  loading={recordsLoading}
                  scroll={isMobile ? { x: 720 } : undefined}
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
            name="webhook_type"
            label="类型"
            tooltip="Todo 类型通过 /webhook/trigger/{todo_id}/todo 触发；Loop 类型通过 /webhook/trigger/{webhook_id}/loop 触发"
            rules={[{ required: true, message: '请选择类型' }]}
          >
            <Select
              placeholder="选择 webhook 类型"
              options={[
                { value: 'todo', label: 'Todo - 触发指定 Todo 执行' },
                { value: 'loop', label: 'Loop - 触发 Loop 的 webhook 触发器' },
              ]}
            />
          </Form.Item>
          <Form.Item noStyle shouldUpdate={(prev, curr) => prev.webhook_type !== curr.webhook_type}>
            {({ getFieldValue }) =>
              getFieldValue('webhook_type') === 'todo' ? (
                <Form.Item
                  name="default_todo_id"
                  label="默认触发 Todo"
                  tooltip="配置后可通过 /webhook/trigger/{todo_id}/todo 显式触发；必须设置才能生成触发 URL"
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
              ) : (
                <Form.Item
                  name="loop_id"
                  label="关联 Loop"
                  tooltip="配置后可通过 /webhook/trigger/{webhook_id}/loop 触发此 Loop；必须设置才能生成触发 URL"
                >
                  <Input type="number" placeholder="输入 Loop ID" />
                </Form.Item>
              )
            }
          </Form.Item>
          <Form.Item
            name="enabled"
            label="启用状态"
            valuePropName="checked"
            initialValue={true}
          >
            <Switch />
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
