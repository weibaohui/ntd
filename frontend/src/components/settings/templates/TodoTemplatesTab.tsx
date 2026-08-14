// 事项模板 Tab
// 提供事项模板的列表、增删改查、同步功能
// 数据从 bundled/todos 目录 + 数据库合并展示

import { useState, useEffect, useCallback } from 'react';
import {
  Alert,
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
  Tooltip,
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
import { ShareToRepoButton } from '@/components/settings/contribute/ShareToRepoButton';
import { buildTodoContributePrompt } from '@/components/settings/contribute/contributePrompts';
import { exportTodoTemplateYaml } from '@/utils/database/todos';

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
    // className 用于 Playwright 精确定位事项模板表格，避开嵌套 Tabs 中其它隐藏表格的干扰。
    <div className="todo-templates-tab">
      {/* 系统/用户说明（与工艺 Tab 风格一致）：系统模板来自远程仓库同步，会被覆盖；
          用户模板（自行创建或复制的）存本地数据库，不会被同步覆盖。 */}
      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 16 }}
        message="系统模板来自远程仓库，每次同步会被覆盖；用户模板（自行创建或复制的）不会被同步覆盖。"
      />
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
        {/* 表格 scroll={{ x: 'max-content' }}：列宽总和超过容器时启用横向滚动，
            避免窄屏下被压缩导致表头与单元格错位(对齐兄弟组件 ExpertsTemplatesTab 的写法，
            此前这里漏配是表格「列对不上」的根因)。 */}
        {templates.length === 0 ? (
          <Empty description="暂无事项模板" />
        ) : (
          <Table
            rowKey="id"
            dataSource={templates}
            pagination={false}
            scroll={{ x: 'max-content' }}
            columns={[
              // 操作列置于第一列：与专家/工艺/Skill 各 Tab 保持一致，操作入口前置便于定位
              {
                title: '操作',
                key: 'actions',
                width: 200,
                render: (_, record: TodoTemplate) => (
                  <Space>
                    {!record.is_system && (
                      <Tooltip title="编辑">
                        <Button
                          type="text"
                          size="small"
                          icon={<EditOutlined />}
                          onClick={() => {
                            setEditing(record);
                            setModalOpen(true);
                          }}
                        />
                      </Tooltip>
                    )}
                    {/* 分享仅对用户创建的事项模板开放（is_system=false，可修改才可分享）；
                        点击时先由后端把 DB 模板导出为 YAML 文件（onPrepare），
                        提示词里的 resource_dir/remote_path 由导出结果注入。 */}
                    {!record.is_system && (
                      <ShareToRepoButton
                        actionType="todo_contribute"
                        actionKey={`todo-${record.id}`}
                        params={{
                          resource_name: record.title,
                          version: '',
                          resource_dir: '',
                          remote_path: `todos/${record.title}.yaml`,
                        }}
                        buildPrompt={buildTodoContributePrompt}
                        panelTitle={`分享事项模板 ${record.title}`}
                        panelDescription="AI 将读取本机 PAT，把该事项模板（后端已导出为 YAML）提交为 PR 到官方仓库（可编辑下方 Prompt）"
                        onPrepare={async () => {
                          // 导出返回 ~/.ntd/contribution-export/todos/{safe_title}.yaml，
                          // 文件名取末段作为远端 todos/ 下的目标文件名（与导出保持一致）
                          const path = await exportTodoTemplateYaml(record.id);
                          const fileName = path.split('/').pop() || '';
                          return {
                            resource_dir: path,
                            remote_path: `todos/${fileName}`,
                          };
                        }}
                        size="small"
                        iconOnly
                      />
                    )}
                    <Tooltip title="复制">
                      <Button
                        type="text"
                        size="small"
                        icon={<CopyOutlined />}
                        onClick={() => handleCopy(record.id)}
                      />
                    </Tooltip>
                    {!record.is_system && (
                      <Popconfirm
                        title="确定删除此模板？"
                        onConfirm={() => handleDelete(record.id)}
                      >
                        <Tooltip title="删除">
                          <Button
                            type="text"
                            size="small"
                            danger
                            icon={<DeleteOutlined />}
                          />
                        </Tooltip>
                      </Popconfirm>
                    )}
                  </Space>
                ),
              },
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
