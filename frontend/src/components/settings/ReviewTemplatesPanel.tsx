// 评审模板管理面板（设置 → 评审模板）。
//
// 表格 + 编辑弹窗 + 删除确认。模板独立于 todo, 评审时由 executor/loop_runner
// 在 review_templates 表里查; 删除模板不级联清 loop 引用 (loops.review_template_id
// 业务层决定是否置 NULL)。

import { useState, useEffect } from 'react';
import { Table, Button, Space, Modal, Form, Input, Popconfirm, Empty, App as AntApp } from 'antd';
import { PlusOutlined, EditOutlined, DeleteOutlined } from '@ant-design/icons';
import * as dbReviewTemplates from '@/utils/database/reviewTemplates';
import type { ReviewTemplate } from '@/types/reviewTemplate';

interface FormValues {
  name: string;
  description?: string;
  prompt: string;
}

export function ReviewTemplatesPanel() {
  const { message } = AntApp.useApp();
  const [templates, setTemplates] = useState<ReviewTemplate[]>([]);
  const [loading, setLoading] = useState(false);
  const [editing, setEditing] = useState<ReviewTemplate | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const [form] = Form.useForm<FormValues>();

  /** 拉一次列表, 供 mount 与增删改后刷新用。 */
  const reload = () => {
    setLoading(true);
    dbReviewTemplates.listReviewTemplates()
      .then(setTemplates)
      .catch((err) => message.error(`加载失败: ${err?.message || err}`))
      .finally(() => setLoading(false));
  };

  useEffect(reload, []);

  /** 打开表单: editing 存在则进入编辑模式, 不存在则新建。 */
  const openForm = (template?: ReviewTemplate) => {
    setEditing(template ?? null);
    form.setFieldsValue(
      template
        ? { name: template.name, description: template.description ?? undefined, prompt: template.prompt }
        : { name: '', description: '', prompt: '' },
    );
    setFormOpen(true);
  };

  /** 关闭表单并清掉 form 残留。 */
  const closeForm = () => {
    setFormOpen(false);
    setEditing(null);
    form.resetFields();
  };

  /** 保存 (新建 / 全量更新) */
  const handleSave = async () => {
    let values: FormValues;
    try {
      values = await form.validateFields();
    } catch {
      // antd 已在控件上展示错误, 不弹全局
      return;
    }
    setSaving(true);
    try {
      const payload = {
        name: values.name.trim(),
        description: values.description?.trim() || null,
        prompt: values.prompt,
      };
      if (editing) {
        await dbReviewTemplates.updateReviewTemplate(editing.id, payload);
        message.success('已更新');
      } else {
        await dbReviewTemplates.createReviewTemplate(payload);
        message.success('已创建');
      }
      closeForm();
      reload();
    } catch (err: any) {
      message.error(`保存失败: ${err?.message || err}`);
    } finally {
      setSaving(false);
    }
  };

  /** 删除。默认模板也给删 (数据库无保护), 用户可手动重建; 业务无 FK 强约束。 */
  const handleDelete = async (id: number, name: string) => {
    try {
      const ok = await dbReviewTemplates.deleteReviewTemplate(id);
      if (ok) {
        message.success(`已删除「${name}」`);
        reload();
      } else {
        message.warning('模板已不存在');
      }
    } catch (err: any) {
      message.error(`删除失败: ${err?.message || err}`);
    }
  };

  return (
    <div style={{ maxWidth: 1000 }}>
      <Space style={{ marginBottom: 16 }}>
        <Button type="primary" icon={<PlusOutlined />} onClick={() => openForm()}>
          新建模板
        </Button>
        <Button onClick={reload}>刷新</Button>
      </Space>

      <Table<ReviewTemplate>
        rowKey="id"
        loading={loading}
        dataSource={templates}
        locale={{ emptyText: <Empty description="暂无评审模板" image={Empty.PRESENTED_IMAGE_SIMPLE} /> }}
        pagination={{ pageSize: 20, showSizeChanger: false }}
        columns={[
          {
            title: '名称',
            dataIndex: 'name',
            width: 180,
            render: (text: string) => <strong>{text}</strong>,
          },
          {
            title: '描述',
            dataIndex: 'description',
            ellipsis: true,
            render: (text: string | null) => text || <span style={{ color: 'var(--color-text-tertiary)' }}>—</span>,
          },
          {
            title: 'Prompt 预览',
            dataIndex: 'prompt',
            ellipsis: true,
            render: (text: string) => (
              <code style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>
                {text.length > 80 ? `${text.slice(0, 80)}…` : text}
              </code>
            ),
          },
          {
            title: '操作',
            width: 160,
            render: (_, record) => (
              <Space>
                <Button
                  type="text"
                  icon={<EditOutlined />}
                  size="small"
                  onClick={() => openForm(record)}
                >
                  编辑
                </Button>
                <Popconfirm
                  title="删除模板"
                  description={
                    <span>
                      确定要删除「{record.name}」吗？
                      <br />
                      被引用的 loop 不会自动更新, 需要手动改回默认。
                    </span>
                  }
                  okText="删除"
                  cancelText="取消"
                  okButtonProps={{ danger: true }}
                  onConfirm={() => handleDelete(record.id, record.name)}
                >
                  <Button type="text" danger icon={<DeleteOutlined />} size="small">
                    删除
                  </Button>
                </Popconfirm>
              </Space>
            ),
          },
        ]}
      />

      <Modal
        title={editing ? `编辑模板：${editing.name}` : '新建评审模板'}
        open={formOpen}
        onCancel={closeForm}
        onOk={handleSave}
        confirmLoading={saving}
        destroyOnClose
        width={720}
      >
        <Form form={form} layout="vertical" preserve={false}>
          <Form.Item
            label="名称"
            name="name"
            rules={[{ required: true, message: '请输入模板名称' }, { max: 128 }]}
          >
            <Input placeholder="如：代码评审 / 文档评审 / 验收清单" />
          </Form.Item>
          <Form.Item label="描述" name="description" rules={[{ max: 512 }]}>
            <Input placeholder="（可选）简短说明这个模板的用途" />
          </Form.Item>
          <Form.Item
            label="Prompt 模板"
            name="prompt"
            rules={[{ required: true, message: '请输入 prompt 模板' }]}
            extra={
              <span>
                支持占位符：
                <code>{'{original_prompt}'}</code>{' '}
                <code>{'{original_output}'}</code>{' '}
                <code>{'{acceptance_criteria}'}</code>{' '}
                <code>{'{max_output_chars}'}</code>
              </span>
            }
          >
            <Input.TextArea rows={10} placeholder="你是一个评审师…" />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
}
