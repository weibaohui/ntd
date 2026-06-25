import { useState, useEffect } from 'react';
import { Card, Form, Select, Button, Space, message } from 'antd';
import * as db from '@/utils/database';
import type { Todo } from '@/types';

interface WorkspaceSettingsPanelProps {
  workspaceId: number;
  onChanged?: () => void;
}

export function WorkspaceSettingsPanel({ workspaceId, onChanged }: WorkspaceSettingsPanelProps) {
  const [todos, setTodos] = useState<Todo[]>([]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [form] = Form.useForm();

  useEffect(() => {
    loadSettings();
    db.getAllTodos().then(setTodos).catch(() => {});
  }, [workspaceId]);

  const loadSettings = () => {
    setLoading(true);
    db.getWorkspaceSettings(workspaceId)
      .then(s => {
        form.setFieldsValue({
          default_response_todo_id: s.default_response_todo_id,
        });
      })
      .catch((err: any) => message.error('加载设置失败: ' + (err?.message || String(err))))
      .finally(() => setLoading(false));
  };

  const handleSave = async () => {
    try {
      const values = await form.validateFields();
      setSaving(true);
      await db.updateWorkspaceSettings(workspaceId, {
        default_response_todo_id: values.default_response_todo_id,
      });
      message.success('设置已保存');
      loadSettings();
      onChanged?.();
    } catch (err: any) {
      if (!err?.errorFields) {
        message.error('保存失败: ' + (err?.message || String(err)));
      }
    } finally {
      setSaving(false);
    }
  };

  const handleClear = () => {
    form.setFieldsValue({ default_response_todo_id: undefined });
  };

  return (
    <Card size="small" loading={loading}>
      <Form form={form} layout="vertical">
        <Form.Item
          name="default_response_todo_id"
          label="默认响应 Todo"
          tooltip="当工作空间内的 Bot 收到无法匹配斜杠命令的消息时，自动使用此 Todo 处理"
        >
          <Select
            showSearch
            allowClear
            placeholder="不设置默认响应"
            filterOption={(input, option) =>
              (option?.label as string)?.toLowerCase().includes(input.toLowerCase())
            }
            style={{ width: 300 }}
          >
            {todos.map(todo => (
              <Select.Option key={todo.id} value={todo.id} label={todo.title}>
                {todo.title}
              </Select.Option>
            ))}
          </Select>
        </Form.Item>
        <Form.Item>
          <Space>
            <Button type="primary" onClick={handleSave} loading={saving}>
              保存设置
            </Button>
            <Button onClick={handleClear}>清除</Button>
          </Space>
        </Form.Item>
      </Form>
    </Card>
  );
}
