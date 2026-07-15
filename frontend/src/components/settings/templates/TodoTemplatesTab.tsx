// 事项模板 Tab
// 提供事项模板的列表、增删改查、同步功能
// 数据从 bundled/todos 目录 + 数据库合并展示

import { useState, useEffect, useCallback } from 'react';
import {
  App,
  Button,
  Empty,
  Input,
  Modal,
  Popconfirm,
  Space,
  Spin,
  Table,
  Tag,
  Form,
  Select,
  message as antMessage,
} from 'antd';
import {
  PlusOutlined,
  ReloadOutlined,
  EditOutlined,
  DeleteOutlined,
  CopyOutlined,
} from '@ant-design/icons';
import * as db from '@/utils/database';
import type { TodoTemplate } from '@/types/todo';

/**
 * 事项模板 Tab
 */
export function TodoTemplatesTab() {
  const { message } = App.useApp();
  const [templates, setTemplates] = useState<TodoTemplate[]>([]);
  const [loading, setLoading] = useState(false);
  const [editing, setEditing] = useState<TodoTemplate | null>(null);
  const [modalOpen, setModalOpen] = useState(false);

  const loadTemplates = useCallback(async () => {
    setLoading(true);
    try {
      const list = await db.getTodoTemplates();
      setTemplates(list);
    } catch (e: any) {
      message.error('加载模板失败: ' + (e?.message || e));
    } finally {
      setLoading(false);
    }
  }, [message]);

  useEffect(() => {
    loadTemplates();
  }, [loadTemplates]);

  const handleDelete = async (id: number) => {
    try {
      await db.deleteTodoTemplate(id);
      message.success('已删除');
      await loadTemplates();
    } catch (e: any) {
      message.error('删除失败: ' + (e?.message || e));
    }
  };

  const handleCopy = async (id: number) => {
    try {
      await db.copyTodoTemplate(id);
      message.success('已复制');
      await loadTemplates();
    } catch (e: any) {
      message.error('复制失败: ' + (e?.message || e));
    }
  };

  return (
    <div>
      <Space style={{ marginBottom: 16 }} wrap>
        <Button
          type="primary"
          icon={<PlusOutlined />}
          onClick={() => {
            setEditing(null);
            setModalOpen(true);
          }}
        >
          新建模板
        </Button>
        <Button icon={<ReloadOutlined />} onClick={loadTemplates} loading={loading}>
          刷新
        </Button>
      </Space>

      <Spin spinning={loading}>
        {templates.length === 0 ? (
          <Empty description="暂无事项模板" />
        ) : (
          <Table
            rowKey="id"
            dataSource={templates}
            pagination={false}
            columns={[
              {
                title: '标题',
                dataIndex: 'title',
                key: 'title',
                render: (text: string, record: TodoTemplate) => (
                  <Space>
                    {text}
                    {record.is_system ? <Tag color="blue">系统</Tag> : null}
                  </Space>
                ),
              },
              {
                title: '分类',
                dataIndex: 'category',
                key: 'category',
                width: 120,
                render: (v: string) => v || '-',
              },
              {
                title: 'Prompt',
                dataIndex: 'prompt',
                key: 'prompt',
                ellipsis: true,
                render: (v: string) => v ? v.substring(0, 60) + (v.length > 60 ? '...' : '') : '-',
              },
              {
                title: '排序',
                dataIndex: 'sort_order',
                key: 'sort_order',
                width: 80,
              },
              {
                title: '操作',
                key: 'actions',
                width: 200,
                render: (_, record: TodoTemplate) => (
                  <Space>
                    <Button
                      type="text"
                      size="small"
                      icon={<EditOutlined />}
                      onClick={() => {
                        setEditing(record);
                        setModalOpen(true);
                      }}
                    />
                    <Button
                      type="text"
                      size="small"
                      icon={<CopyOutlined />}
                      onClick={() => handleCopy(record.id)}
                    />
                    {!record.is_system && (
                      <Popconfirm
                        title="确定删除此模板？"
                        onConfirm={() => handleDelete(record.id)}
                      >
                        <Button
                          type="text"
                          size="small"
                          danger
                          icon={<DeleteOutlined />}
                        />
                      </Popconfirm>
                    )}
                  </Space>
                ),
              },
            ]}
          />
        )}
      </Spin>

      <TemplateEditModal
        open={modalOpen}
        template={editing}
        onClose={() => setModalOpen(false)}
        onSaved={async () => {
          setModalOpen(false);
          await loadTemplates();
        }}
      />
    </div>
  );
}

/**
 * 模板编辑弹窗
 */
function TemplateEditModal({
  open,
  template,
  onClose,
  onSaved,
}: {
  open: boolean;
  template: TodoTemplate | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [form] = Form.useForm();
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (open) {
      if (template) {
        form.setFieldsValue({
          title: template.title,
          prompt: template.prompt,
          category: template.category,
          sort_order: template.sort_order,
        });
      } else {
        form.resetFields();
        form.setFieldsValue({ category: 'general', sort_order: 0 });
      }
    }
  }, [open, template, form]);

  const handleSave = async () => {
    try {
      const values = await form.validateFields();
      setSaving(true);
      if (template) {
        await db.updateTodoTemplate(
          template.id,
          values.title,
          values.prompt,
          values.category,
          values.sort_order,
        );
      } else {
        await db.createTodoTemplate(
          values.title,
          values.prompt,
          values.category,
          values.sort_order,
        );
      }
      onSaved();
    } catch (e: any) {
      if (e?.errorFields) return;
      antMessage.error('保存失败: ' + (e?.message || e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      title={template ? '编辑模板' : '新建模板'}
      open={open}
      onCancel={onClose}
      onOk={handleSave}
      confirmLoading={saving}
      width={600}
    >
      <Form form={form} layout="vertical" style={{ marginTop: 16 }}>
        <Form.Item
          name="title"
          label="标题"
          rules={[{ required: true, message: '请输入标题' }]}
        >
          <Input placeholder="模板标题" />
        </Form.Item>
        <Form.Item name="category" label="分类">
          <Select
            options={[
              { value: 'general', label: '通用' },
              { value: 'bug', label: 'Bug' },
              { value: 'feature', label: '功能' },
              { value: 'task', label: '任务' },
              { value: 'refactor', label: '重构' },
            ]}
          />
        </Form.Item>
        <Form.Item name="prompt" label="Prompt">
          <Input.TextArea rows={6} placeholder="模板的 AI prompt 内容" />
        </Form.Item>
        <Form.Item name="sort_order" label="排序">
          <Input type="number" placeholder="0" />
        </Form.Item>
      </Form>
    </Modal>
  );
}
