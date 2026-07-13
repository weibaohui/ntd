import { useState, useEffect } from 'react';
import { Card, Form, Select, Button, Space, message, Radio, InputNumber, Typography } from 'antd';
import * as db from '@/utils/database';
import { listLoops } from '@/utils/database/loops';
import { EXECUTORS_FOR_PICKER } from '@/utils/executors';
import { ExecutorPicker } from '@/components/todo-drawer/ExecutorPicker';
import type { Todo } from '@/types';
import type { LoopListItem } from '@/types/loop';
import type { AgentBot } from '@/utils/database';
import type { FeishuHistoryChat } from '@/types';
import { HistoryChatsCard } from '@/components/settings/assistant/HistoryChatsCard';

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
  // 当前工作空间的飞书 bot，及其历史拉取群配置（per-bot）
  const [bot, setBot] = useState<AgentBot | null>(null);
  const [historyChats, setHistoryChats] = useState<FeishuHistoryChat[]>([]);
  const [histChatId, setHistChatId] = useState('');
  const [histChatName, setHistChatName] = useState('');

  useEffect(() => {
    loadSettings();
    loadHistorySettings();
    // 按 workspace_id 过滤 Todo，使下拉列表仅显示当前工作空间内的事项
    db.getAllTodos(workspaceId).then(setTodos).catch(() => {});
    // 加载当前工作空间的环路列表
    listLoops(workspaceId).then(setLoops).catch(() => {});
    // 加载当前工作空间的飞书 bot（一个工作空间一个 bot），用于历史拉取群配置
    db.getAgentBots().then(bots => {
      const b = bots.find(x => x.workspace_id === workspaceId && x.bot_type === 'feishu') || null;
      setBot(b);
      if (b) reloadHistoryChats(b.id);
    }).catch(() => {});
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

  // 历史拉取群管理（per-bot）：用户填写群 chat_id，机器人定期拉取这些群的历史消息
  const reloadHistoryChats = (botId: number) => {
    db.getFeishuHistoryChats().then(all => setHistoryChats(all.filter(c => c.bot_id === botId))).catch(() => {});
  };
  const handleAddHistChat = async () => {
    // chat_id 必填、备注可选；空 chat_id 直接忽略
    if (!bot || !histChatId.trim()) return;
    try {
      await db.createFeishuHistoryChat(bot.id, histChatId.trim(), histChatName.trim() || undefined);
      setHistChatId('');
      setHistChatName('');
      reloadHistoryChats(bot.id);
    } catch (e: any) {
      message.error('添加拉取群失败: ' + (e.message || '未知错误'));
    }
  };
  const handleDeleteHistChat = async (id: number) => {
    if (!bot) return;
    try {
      await db.deleteFeishuHistoryChat(id);
      reloadHistoryChats(bot.id);
    } catch (e: any) {
      message.error('删除拉取群失败: ' + (e.message || '未知错误'));
    }
  };

  const responseType = Form.useWatch('default_response_type', form);
  const executorValue = Form.useWatch('default_response_executor', form);

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
              label="执行器类型"
              tooltip="选择执行器来处理消息"
            >
              <ExecutorPicker
                executor={executorValue || 'claudecode'}
                executorOptions={EXECUTORS_FOR_PICKER}
                onChange={v => form.setFieldValue('default_response_executor', v)}
              />
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

      {/* 历史消息拉取群（per-bot）：填写要定期拉取历史消息的群 chat_id */}
      {bot && (
        <HistoryChatsCard
          chats={historyChats}
          chatId={histChatId}
          chatName={histChatName}
          onChatIdChange={setHistChatId}
          onChatNameChange={setHistChatName}
          onAdd={handleAddHistChat}
          onDelete={handleDeleteHistChat}
        />
      )}
    </>
  );
}
