import { useState, useEffect } from 'react';
import { Card, Form, Select, Button, Space, message, Radio, InputNumber, Typography } from 'antd';
import * as db from '@/utils/database';
import { listLoops } from '@/utils/database/loops';
import type { Todo } from '@/types';
import type { LoopListItem } from '@/types/loop';
import { ExecutorPickerPopover } from '@/components/common/ExecutorPickerPopover';

const { Paragraph } = Typography;

interface WorkspaceSettingsPanelProps {
  workspaceId: number;
  onChanged?: () => void;
}

export function WorkspaceSettingsPanel({ workspaceId, onChanged }: WorkspaceSettingsPanelProps) {
  const [todos, setTodos] = useState<Todo[]>([]);
  const [loops, setLoops] = useState<LoopListItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historySaving, setHistorySaving] = useState(false);
  const [form] = Form.useForm();
  const [historyForm] = Form.useForm();

  useEffect(() => {
    loadSettings();
    loadHistorySettings();
    // 按 workspace_id 过滤 Todo，使下拉列表仅显示当前工作空间内的事项
    db.getAllTodos(workspaceId).then(setTodos).catch(() => {});
    // 加载当前工作空间的环路列表
    listLoops(workspaceId).then(setLoops).catch(() => {});
  }, [workspaceId]);

  const loadSettings = () => {
    setLoading(true);
    db.getWorkspaceSettings(workspaceId)
      .then(s => {
        form.setFieldsValue({
          default_response_type: s.default_response_type || 'todo',
          default_response_todo_id: s.default_response_todo_id,
          default_response_loop_id: s.default_response_loop_id,
          default_response_executor: s.default_response_executor,
        });
      })
      .catch((err: any) => message.error('加载设置失败: ' + (err?.message || String(err))))
      .finally(() => setLoading(false));
  };

  const loadHistorySettings = () => {
    setHistoryLoading(true);
    db.getConfig()
      .then(cfg => {
        historyForm.setFieldsValue({
          history_message_max_age_secs: cfg.history_message_max_age_secs ?? 600,
        });
      })
      .catch(() => {})
      .finally(() => setHistoryLoading(false));
  };

  const handleSave = async () => {
    try {
      const values = await form.validateFields();
      setSaving(true);
      await db.updateWorkspaceSettings(workspaceId, {
        default_response_type: values.default_response_type,
        default_response_todo_id: values.default_response_type === 'todo' ? values.default_response_todo_id : undefined,
        default_response_loop_id: values.default_response_type === 'loop' ? values.default_response_loop_id : 0,
        default_response_executor: values.default_response_type === 'executor' ? values.default_response_executor : undefined,
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

  const handleSaveHistory = async () => {
    try {
      const values = await historyForm.validateFields();
      setHistorySaving(true);
      const currentConfig = await db.getConfig();
      await db.updateConfig({
        ...currentConfig,
        history_message_max_age_secs: values.history_message_max_age_secs,
      });
      message.success('历史消息设置已保存');
      loadHistorySettings();
    } catch (err: any) {
      if (!err?.errorFields) {
        message.error('保存失败: ' + (err?.message || String(err)));
      }
    } finally {
      setHistorySaving(false);
    }
  };

  const responseType = Form.useWatch('default_response_type', form);

  return (
    <>
      <Card size="small" loading={loading} title="默认响应配置">
        <Form form={form} layout="vertical" initialValues={{ default_response_type: 'todo' }}>
          <Form.Item
            name="default_response_type"
            label="响应类型"
            tooltip="当工作空间内的 Bot 收到无法匹配斜杠命令的消息时，执行默认响应"
          >
            <Radio.Group>
              <Radio.Button value="todo">Todo</Radio.Button>
              <Radio.Button value="loop">环路</Radio.Button>
              <Radio.Button value="executor">执行器</Radio.Button>
            </Radio.Group>
          </Form.Item>

          {responseType === 'todo' && (
            <Form.Item
              name="default_response_todo_id"
              label="默认响应 Todo"
              tooltip="选择工作空间内的 Todo 来处理消息"
            >
              <Select
                showSearch
                allowClear
                placeholder="选择默认响应的 Todo"
                filterOption={(input, option) =>
                  (option?.label as string)?.toLowerCase().includes(input.toLowerCase())
                }
                style={{ width: 300 }}
              >
                {todos.map(todo => (
                  <Select.Option key={todo.id} value={todo.id} label={`#${todo.id} ${todo.title}`}>
                    #{todo.id} {todo.title}
                  </Select.Option>
                ))}
              </Select>
            </Form.Item>
          )}

          {responseType === 'loop' && (
            <Form.Item
              name="default_response_loop_id"
              label="默认响应环路"
              tooltip="选择工作空间内的环路来处理消息"
            >
              <Select
                showSearch
                allowClear
                placeholder="选择默认响应的环路"
                filterOption={(input, option) =>
                  (option?.label as string)?.toLowerCase().includes(input.toLowerCase())
                }
                style={{ width: 300 }}
              >
                {loops.map(loop => (
                  <Select.Option key={loop.id} value={loop.id} label={loop.name}>
                    {loop.name}
                  </Select.Option>
                ))}
              </Select>
            </Form.Item>
          )}

          {responseType === 'executor' && (
            <Form.Item
              name="default_response_executor"
              label="执行器"
              initialValue="claudecode"
              tooltip="选择执行器来处理消息"
            >
              <ExecutorPickerPopover />
            </Form.Item>
          )}

          <Form.Item>
            <Space>
              <Button type="primary" onClick={handleSave} loading={saving}>
                保存设置
              </Button>
            </Space>
          </Form.Item>
        </Form>
      </Card>

      <Card size="small" loading={historyLoading} title="历史消息处理" style={{ marginTop: 16 }}>
        <Paragraph type="secondary" style={{ marginBottom: 16, fontSize: 13 }}>
          拉取历史消息时，超过设定时间的消息将保存但跳过处理，避免离线后重新处理大量旧消息。
        </Paragraph>
        <Form form={historyForm} layout="vertical">
          <Form.Item
            name="history_message_max_age_secs"
            label="最大处理年龄（秒）"
            tooltip="仅处理此时间内的历史消息，默认 600 秒（10 分钟）"
          >
            <InputNumber min={0} max={86400} step={60} placeholder="600" addonAfter="秒" style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item>
            <Space>
              <Button type="primary" onClick={handleSaveHistory} loading={historySaving}>
                保存设置
              </Button>
            </Space>
          </Form.Item>
        </Form>
      </Card>
    </>
  );
}
