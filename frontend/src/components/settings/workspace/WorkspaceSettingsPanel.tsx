import { useState, useEffect } from 'react';
import { Card, Form, Select, Button, Space, message, InputNumber, Typography } from 'antd';
import * as db from '@/utils/database';
import { getAllExperts } from '@/utils/database/experts';
import { getExpertDisplayName } from '@/types/expert';
import type { ExpertMetadata } from '@/types/expert';
import { EXECUTORS_FOR_PICKER } from '@/utils/executors';
import { ExecutorPicker } from '@/components/todo-drawer/ExecutorPicker';
import type { AgentBot } from '@/utils/database';
import type { FeishuHistoryChat } from '@/types';
import { HistoryChatsCard } from '@/components/settings/assistant/HistoryChatsCard';

const { Paragraph } = Typography;

interface WorkspaceSettingsPanelProps {
  workspaceId: number;
  onChanged?: () => void;
}

// 108 修订（群聊管家）：本面板承载工作空间级设置——对话执行器（单聊直聊/群聊管家共用）、
// 群聊管家专家（仅群聊注入）、委派接力上限、历史消息处理与拉取群。
// 默认响应机制（todo/loop/executor 三选一）已整体退役，未命中斜杠命令的消息进聊天直连。
export function WorkspaceSettingsPanel({ workspaceId, onChanged }: WorkspaceSettingsPanelProps) {
  const [experts, setExperts] = useState<ExpertMetadata[]>([]);
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
  // 接力上限「系统兜底」提示值：仅当工作空间未配置(raw=null)时，effective 才等于兜底常量，
  // 此时取它做 InputNumber placeholder；已配置时不覆盖，确保「留空→回退」提示恒指兜底常量，不硬编码 10。
  const [defaultMaxHint, setDefaultMaxHint] = useState<number>(10);

  useEffect(() => {
    loadSettings();
    loadHistorySettings();
    // 加载专家列表供管家专家选择（专家名几乎不变，失败静默降级为空列表）
    getAllExperts().then(setExperts).catch(() => {});
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
          // 空串/ null 都映射为 undefined：让 Select 显示 placeholder 而非空白值
          butler_expert_name: s.butler_expert_name || undefined,
          butler_executor: s.butler_executor || undefined,
          // raw 覆盖值：null 表示未配置（InputNumber 显示空，提示走系统默认）。
          delegate_max_rounds: s.delegate_max_rounds,
        });
        // 仅未配置时 effective 才等于兜底常量，用它做「留空回退」提示，不硬编码 10。
        if (s.delegate_max_rounds == null) setDefaultMaxHint(s.delegate_max_rounds_effective ?? 10);
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
        // 清空选择时值为 undefined，显式传空串让后端落「显式清空」（与未配置的 NULL 下游同义）
        butler_expert_name: values.butler_expert_name ?? '',
        butler_executor: values.butler_executor ?? '',
        // null=清除回退系统兜底；N(1..=50)=置工作空间默认，任务级未覆盖时以此为准。
        delegate_max_rounds: values.delegate_max_rounds ?? null,
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

  const executorValue = Form.useWatch('butler_executor', form);

  return (
    <>
      <Card size="small" loading={loading} title="对话与群聊管家">
        <Paragraph type="secondary" style={{ marginBottom: 16, fontSize: 13 }}>
          未命中斜杠命令的消息进聊天直连：单聊直接与「对话执行器」对话（多轮会话）；
          群聊由「群聊管家」处理——管家 = 专家（人设与规则，仅群聊生效，可选）+ 执行器。
          不配执行器时，未命中消息收到配置引导提示。
        </Paragraph>
        <Form form={form} layout="vertical">
          <Form.Item
            name="butler_executor"
            label="对话执行器"
            tooltip="单聊直聊与群聊管家共用的执行进程；不配置时，未命中斜杠命令的消息将收到配置引导提示"
          >
            <ExecutorPicker
              executor={executorValue || ''}
              executorOptions={EXECUTORS_FOR_PICKER}
              onChange={v => form.setFieldValue('butler_executor', v)}
            />
          </Form.Item>

          <Form.Item
            name="butler_expert_name"
            label="群聊管家专家"
            tooltip="仅群聊生效：为群聊管家注入专家的人设与行为规则；单聊始终是纯执行器对话，不读此配置"
          >
            <Select
              showSearch
              allowClear
              placeholder="选择群聊管家专家（可选，仅群聊生效）"
              filterOption={(input, option) =>
                // label 拼上专家 ID：显示名是中文时可按英文 ID 搜（反之亦然）
                (option?.label as string)?.toLowerCase().includes(input.toLowerCase())
              }
              style={{ width: 300 }}
            >
              {experts.map(expert => (
                <Select.Option
                  key={expert.name}
                  value={expert.name}
                  label={`${getExpertDisplayName(expert)}（${expert.name}）`}
                >
                  {getExpertDisplayName(expert)}（{expert.name}）
                </Select.Option>
              ))}
            </Select>
          </Form.Item>

          {/* 委派接力上限（需求 092）：工作空间级默认，任务级可单独覆盖。 */}
          <Form.Item
            name="delegate_max_rounds"
            label="委派接力上限"
            tooltip="该工作空间内委派任务「自动接力轮数上限」的默认值；单个任务可在详情页覆盖。"
            extra={`留空使用系统默认（${defaultMaxHint} 轮）；任务级未覆盖时以此默认为准。`}
          >
            <InputNumber
              min={1}
              max={50}
              placeholder={`默认 ${defaultMaxHint} 轮`}
              addonAfter="轮"
              style={{ width: '100%' }}
              data-testid="ws-delegate-max-rounds"
            />
          </Form.Item>

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
