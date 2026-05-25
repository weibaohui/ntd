import { useState, useEffect } from 'react';
import { Tabs, Card, Button, Spin, List, Empty, Select, AutoComplete, Input, InputNumber, Switch, Modal, Table, Tag, Typography, Form, Tooltip, Space, Popconfirm, message } from 'antd';
import { QrcodeOutlined, DeleteOutlined, CopyOutlined, PlusOutlined, ReloadOutlined, HistoryOutlined, QuestionCircleOutlined, MinusCircleOutlined, InfoCircleOutlined } from '@ant-design/icons';
import QRCode from 'qrcode';
import { useApp } from '../../hooks/useApp';
import * as db from '../../utils/database';
import type { FeishuPushStatus, WhitelistEntry } from '../../utils/database';
import type { FeishuHistoryMessage, FeishuHistoryChat, ExecutionRecord } from '../../types';

const { Paragraph } = Typography;
const { Option } = Select;

export function MessagesPanel({ configForm, configSaving, handleSaveConfig, onBack }: {
  configForm: any;
  configSaving: boolean;
  handleSaveConfig: () => Promise<void>;
  onBack?: () => void;
}) {
  const { state, dispatch } = useApp();
  const { todos } = state;

  const [agentBots, setAgentBots] = useState<db.AgentBot[]>([]);
  const [botsLoading, setBotsLoading] = useState(false);
  const [feishuPushStatus, setFeishuPushStatus] = useState<FeishuPushStatus[]>([]);
  const [groupWhitelist, setGroupWhitelist] = useState<WhitelistEntry[]>([]);
  const [whitelistOpenId, setWhitelistOpenId] = useState('');
  const [whitelistName, setWhitelistName] = useState('');
  const [whitelistBotId, setWhitelistBotId] = useState<number | null>(null);
  const [binding, setBinding] = useState(false);
  const [bindModalOpen, setBindModalOpen] = useState(false);
  const [qrCodeUrl, setQrCodeUrl] = useState('');
  const [pollError, setPollError] = useState('');
  const [bindSuccess, setBindSuccess] = useState(false);

  // History state
  const [historyMessages, setHistoryMessages] = useState<FeishuHistoryMessage[]>([]);
  const [historyChats, setHistoryChats] = useState<FeishuHistoryChat[]>([]);
  const [historySenders, setHistorySenders] = useState<db.FeishuSenderItem[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyTotal, setHistoryTotal] = useState(0);
  const [historyPage, setHistoryPage] = useState(1);
  const [historyPageSize, setHistoryPageSize] = useState(20);
  const [historySelectedChatId, setHistorySelectedChatId] = useState<string | undefined>(undefined);
  const [historyIsHistory, setHistoryIsHistory] = useState<boolean | undefined>(undefined);
  const [historySelectedSenderId, setHistorySelectedSenderId] = useState<string | undefined>(undefined);
  const [historyViewMsg, setHistoryViewMsg] = useState<string | null>(null);
  const [historyAddModalOpen, setHistoryAddModalOpen] = useState(false);
  const [historyForm] = Form.useForm();

  // Execution record detail modal
  const [execDetailRecord, setExecDetailRecord] = useState<ExecutionRecord | null>(null);

  const loadAgentBots = () => {
    setBotsLoading(true);
    db.getAgentBots()
      .then((bots) => setAgentBots(bots))
      .catch(() => {})
      .finally(() => setBotsLoading(false));
  };

  const loadFeishuPush = () => {
    db.getFeishuPush()
      .then((status) => setFeishuPushStatus(status))
      .catch(() => {});
  };

  const loadGroupWhitelist = (botId: number) => {
    setWhitelistBotId(botId);
    db.getGroupWhitelist(botId)
      .then(setGroupWhitelist)
      .catch(() => setGroupWhitelist([]));
  };

  const handleAddWhitelist = async () => {
    if (!whitelistBotId || !whitelistOpenId.trim()) return;
    try {
      await db.addGroupWhitelist(whitelistBotId, whitelistOpenId.trim(), whitelistName.trim() || undefined);
      loadGroupWhitelist(whitelistBotId);
      setWhitelistOpenId('');
      setWhitelistName('');
    } catch (e: any) {
      message.error('添加白名单失败: ' + (e.message || '未知错误'));
    }
  };

  const handleDeleteWhitelist = async (id: number) => {
    if (!whitelistBotId) return;
    try {
      await db.deleteGroupWhitelist(id);
      loadGroupWhitelist(whitelistBotId);
    } catch (e: any) {
      message.error('删除白名单失败: ' + (e.message || '未知错误'));
    }
  };

  const loadHistoryMessages = async () => {
    setHistoryLoading(true);
    try {
      const data = await db.getFeishuHistoryMessages({
        chat_id: historySelectedChatId,
        is_history: historyIsHistory,
        sender_open_id: historySelectedSenderId,
        page: historyPage,
        page_size: historyPageSize,
      });
      setHistoryMessages(data.messages);
      setHistoryTotal(data.total);
    } catch {
      message.error('加载历史消息失败');
    } finally {
      setHistoryLoading(false);
    }
  };

  const loadHistoryChats = async () => {
    try {
      const data = await db.getFeishuHistoryChats();
      setHistoryChats(data);
    } catch (e) {
      console.error('加载群聊配置失败', e);
    }
  };

  const loadHistorySenders = async () => {
    try {
      const data = await db.getFeishuSenders();
      setHistorySenders(data);
    } catch (e) {
      console.error('加载发送者列表失败', e);
    }
  };

  useEffect(() => {
    loadHistoryChats();
    loadHistorySenders();
  }, []);

  useEffect(() => {
    loadHistoryMessages();
  }, [historyPage, historyPageSize, historySelectedChatId, historyIsHistory, historySelectedSenderId]);

  const handleAddHistoryChat = async () => {
    try {
      const values = await historyForm.validateFields();
      await db.createFeishuHistoryChat(values);
      message.success('添加成功');
      setHistoryAddModalOpen(false);
      historyForm.resetFields();
      loadHistoryChats();
    } catch (e) {
      if (e instanceof Error) {
        message.error(e.message);
      }
    }
  };

  useEffect(() => {
    loadAgentBots();
    loadFeishuPush();
  }, []);

  const handleStartFeishuBind = async () => {
    setBinding(true);
    setBindSuccess(false);
    setPollError('');
    setQrCodeUrl('');
    setBindModalOpen(true);

    try {
      const initRes = await db.feishuInit();
      if (!initRes.supported) {
        setPollError('当前环境不支持 client_secret 认证');
        setBinding(false);
        return;
      }

      const beginRes = await db.feishuBegin();

      const qrDataUrl = await QRCode.toDataURL(beginRes.qr_url, {
        width: 256,
        margin: 2,
      });
      setQrCodeUrl(qrDataUrl);

      const pollRes = await db.feishuPoll(beginRes.device_code, beginRes.interval, beginRes.expire_in);

      if (pollRes.success) {
        setBindSuccess(true);
        message.success(`绑定成功！Bot: ${pollRes.bot_name || 'Feishu Bot'}`);
        loadAgentBots();
        loadFeishuPush();
        setTimeout(() => {
          setBindModalOpen(false);
          setQrCodeUrl('');
        }, 2000);
      } else {
        const errMsg = pollRes.error === 'access_denied' ? '用户拒绝了绑定请求'
          : pollRes.error === 'expired_token' ? '二维码已过期，请重新绑定'
          : '绑定超时，请重试';
        setPollError(errMsg);
      }
    } catch (err: any) {
      setPollError(err?.message || '启动绑定失败');
    } finally {
      setBinding(false);
    }
  };

  const handleDeleteBot = async (botId: number) => {
    try {
      await db.deleteAgentBot(botId);
      message.success('已删除');
      loadAgentBots();
    } catch (err: any) {
      message.error(err?.message || '删除失败');
    }
  };

  return (
    <div>
    <Tabs
      defaultActiveKey="bind"
      size="small"
      items={[
        {
          key: 'bind',
          label: '绑定',
          children: (
            <div className="settings-messages-tab" style={{ maxWidth: 700 }}>
              <Card
                title="绑定消息接收智能体"
                size="small"
                style={{ marginBottom: 24 }}
                extra={
                  <Button
                    type="primary"
                    icon={<QrcodeOutlined />}
                    onClick={handleStartFeishuBind}
                    loading={binding}
                    size="small"
                  >
                    绑定飞书
                  </Button>
                }
              >
                <Paragraph type="secondary" style={{ marginBottom: 16, fontSize: 13 }}>
                  绑定飞书智能体 Bot 后，可以接收任务执行结果和通知消息。支持绑定多个 Bot。
                </Paragraph>

                <Spin spinning={botsLoading}>
                  {agentBots.length === 0 ? (
                    <Empty description="暂无绑定的智能体" image={Empty.PRESENTED_IMAGE_SIMPLE} />
                  ) : (
                    <List
                      dataSource={agentBots}
                      renderItem={(bot) => {
                        let botConfig: Record<string, boolean> = { dm_enabled: true, group_enabled: true, group_require_mention: true, echo_reply: true };
                        try { botConfig = JSON.parse(bot.config || '{}'); } catch {}
                        const isFeishu = bot.bot_type === 'feishu';
                        const handleConfigChange = async (key: string, val: boolean) => {
                          const newConfig = { ...botConfig, [key]: val };
                          try {
                            await db.updateAgentBotConfig(bot.id, JSON.stringify(newConfig));
                            setAgentBots(prev => prev.map(b => b.id === bot.id ? { ...b, config: JSON.stringify(newConfig) } : b));
                          } catch (e: any) {
                            message.error('保存配置失败: ' + (e.message || '未知错误'));
                          }
                        };

                        const botPushStatus = feishuPushStatus.find(p => p.bot_id === bot.id);
                        const hasPushTarget = !!botPushStatus;
                        const handlePushLevelChange = async (level: db.FeishuPushLevel) => {
                          try {
                            await db.updateFeishuPush({ botId: bot.id, pushLevel: level });
                            loadFeishuPush();
                          } catch (e: any) {
                            message.error('设置推送失败: ' + (e.message || '未知错误'));
                          }
                        };
                        const handlePushTargetUpdate = async (field: 'p2p_receive_id' | 'receive_id_type' | 'group_chat_id', value: string) => {
                          try {
                            const updateField = field === 'p2p_receive_id' ? 'p2pReceiveId'
                              : field === 'group_chat_id' ? 'groupChatId' : 'receiveIdType';
                            await db.updateFeishuPush({ botId: bot.id, [updateField]: value });
                            loadFeishuPush();
                          } catch (e: any) {
                            message.error('更新推送目标失败: ' + (e.message || '未知错误'));
                          }
                        };
                        const handleResponseEnabledChange = async (botId: number, targetType: 'p2p' | 'group', enabled: boolean) => {
                          try {
                            if (targetType === 'p2p') {
                              await db.updateFeishuPush({ botId, p2pResponseEnabled: enabled });
                            } else {
                              await db.updateFeishuPush({ botId, groupResponseEnabled: enabled });
                            }
                            loadFeishuPush();
                          } catch (e: any) {
                            message.error('更新响应开关失败: ' + (e.message || '未知错误'));
                          }
                        };
                        const copyToClipboard = (text: string, label: string) => {
                          navigator.clipboard.writeText(text).then(() => {
                            message.success(`${label} 已复制`);
                          }).catch(() => {
                            message.error('复制失败');
                          });
                        };

                        return (
                          <div
                            key={bot.id}
                            style={{
                              padding: '12px',
                              background: 'var(--color-bg)',
                              borderRadius: 8,
                              marginBottom: 8,
                              border: '1px solid var(--color-border-light)',
                            }}
                          >
                            <div style={{ display: 'flex', alignItems: 'flex-start', gap: 10 }}>
                              <div
                                style={{
                                  width: 36,
                                  height: 36,
                                  borderRadius: 8,
                                  background: isFeishu ? '#1976D2' : '#888',
                                  display: 'flex',
                                  alignItems: 'center',
                                  justifyContent: 'center',
                                  color: '#fff',
                                  fontWeight: 700,
                                  fontSize: 14,
                                  flexShrink: 0,
                                }}
                              >
                                {isFeishu ? '飞' : '其他'}
                              </div>
                              <div style={{ flex: 1, minWidth: 0 }}>
                                <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
                                  <span style={{ fontWeight: 600, fontSize: 14, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{bot.bot_name}</span>
                                  <Tag color={bot.enabled ? 'green' : 'default'} style={{ marginRight: 0 }}>
                                    {bot.enabled ? '已启用' : '已禁用'}
                                  </Tag>
                                </div>
                                <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', wordBreak: 'break-all', lineHeight: 1.6 }}>
                                  App ID: {bot.app_id}
                                </div>
                                {bot.domain && (
                                  <div style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>
                                    平台: {bot.domain === 'lark' ? 'Lark 国际版' : '飞书'}
                                  </div>
                                )}
                                <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginTop: 2 }}>
                                  绑定时间: {new Date(bot.created_at).toLocaleString()}
                                </div>
                              </div>
                              <Popconfirm
                                title="删除确认"
                                description={`确定要删除 "${bot.bot_name}" 吗？`}
                                onConfirm={() => handleDeleteBot(bot.id)}
                                okText="删除"
                                cancelText="取消"
                                okButtonProps={{ danger: true }}
                              >
                                <Button type="text" danger icon={<DeleteOutlined />} size="small" style={{ flexShrink: 0 }} />
                              </Popconfirm>
                            </div>
                            {isFeishu && (
                              <div style={{ marginTop: 8, paddingTop: 8, borderTop: '1px solid var(--color-border-light)' }}>
                                <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 6 }}>消息配置</div>
                                <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px 16px' }}>
                                  <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
                                    <Switch size="small" checked={botConfig.dm_enabled !== false} onChange={v => handleConfigChange('dm_enabled', v)} />
                                    接收单聊消息
                                  </span>
                                  <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
                                    <Switch size="small" checked={botConfig.group_enabled !== false} onChange={v => handleConfigChange('group_enabled', v)} />
                                    接收群聊消息
                                  </span>
                                  <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
                                    <Switch size="small" checked={botConfig.group_require_mention !== false} onChange={v => handleConfigChange('group_require_mention', v)} />
                                    群聊仅处理@
                                  </span>
                                  <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
                                    <Switch size="small" checked={botConfig.echo_reply !== false} onChange={v => handleConfigChange('echo_reply', v)} />
                                    Echo 回复
                                  </span>
                                  {hasPushTarget && (<>
                                    <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
                                      <Switch
                                        size="small"
                                        checked={botPushStatus.p2p_response_enabled}
                                        onChange={(v) => handleResponseEnabledChange(botPushStatus.bot_id, 'p2p', v)}
                                      />
                                      单聊响应
                                      <InputNumber
                                        size="small"
                                        min={1}
                                        max={300}
                                        value={botPushStatus.p2p_debounce_secs}
                                        onChange={(v) => { if (v !== null) db.updateFeishuPush({ botId: botPushStatus.bot_id, p2pDebounceSecs: v }); }}
                                        style={{ width: 50, fontSize: 11 }}
                                      />
                                      <span style={{ fontSize: 10 }}>秒合并</span>
                                    </span>
                                    <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
                                      <Switch
                                        size="small"
                                        checked={botPushStatus.group_response_enabled}
                                        onChange={(v) => handleResponseEnabledChange(botPushStatus.bot_id, 'group', v)}
                                      />
                                      群聊响应
                                      <InputNumber
                                        size="small"
                                        min={1}
                                        max={300}
                                        value={botPushStatus.group_debounce_secs}
                                        onChange={(v) => { if (v !== null) db.updateFeishuPush({ botId: botPushStatus.bot_id, groupDebounceSecs: v }); }}
                                        style={{ width: 50, fontSize: 11 }}
                                      />
                                      <span style={{ fontSize: 10 }}>秒合并</span>
                                    </span>
                                  </>)}
                                </div>
                                {hasPushTarget && (
                                  <div style={{ marginTop: 10 }}>
                                    <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 6 }}>
                                      推送目标
                                      <Select
                                        size="small"
                                        value={botPushStatus.push_level}
                                        onChange={handlePushLevelChange}
                                        style={{ width: 90, marginLeft: 8 }}
                                        options={[
                                          { value: 'disabled', label: '关闭' },
                                          { value: 'result_only', label: '仅结论' },
                                          { value: 'all', label: '全部' },
                                        ]}
                                      />
                                    </div>
                                    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                                      <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                                        <span style={{ fontSize: 11, width: 80, color: 'var(--color-text-tertiary)' }}>单聊ID:</span>
                                        <Input
                                          size="small"
                                          value={botPushStatus.p2p_receive_id}
                                          onChange={(e) => handlePushTargetUpdate('p2p_receive_id', e.target.value)}
                                          style={{ flex: 1, fontSize: 11 }}
                                        />
                                        <Button size="small" icon={<CopyOutlined />} onClick={() => copyToClipboard(botPushStatus.p2p_receive_id, 'p2p_receive_id')} />
                                      </div>
                                      <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                                        <span style={{ fontSize: 11, width: 80, color: 'var(--color-text-tertiary)' }}>群ID:</span>
                                        <Input
                                          size="small"
                                          value={botPushStatus.group_chat_id || ''}
                                          onChange={(e) => handlePushTargetUpdate('group_chat_id', e.target.value)}
                                          style={{ flex: 1, fontSize: 11 }}
                                        />
                                        <Button size="small" icon={<CopyOutlined />} onClick={() => copyToClipboard(botPushStatus.group_chat_id || '', 'group_chat_id')} />
                                      </div>
                                      <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                                        <span style={{ fontSize: 11, width: 80, color: 'var(--color-text-tertiary)' }}>发送类型:</span>
                                        <Select
                                          size="small"
                                          value={botPushStatus.receive_id_type}
                                          onChange={(v) => handlePushTargetUpdate('receive_id_type', v)}
                                          style={{ width: 100 }}
                                          options={[
                                            { value: 'open_id', label: '私聊' },
                                            { value: 'chat_id', label: '群聊' },
                                          ]}
                                        />
                                        <span style={{ fontSize: 10, color: 'var(--color-text-tertiary)', marginLeft: 84 }}>
                                          提示：向机器人发送 /sethome 可快速设置当前对话的 ID
                                        </span>
                                      </div>
                                    </div>
                                  </div>
                                )}
                                {hasPushTarget && (
                                  <div style={{ marginTop: 10 }}>
                                    <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 4 }}>
                                      群聊响应白名单
                                      <Tooltip title="白名单为空时不限制，仅白名单内的用户消息会触发响应">
                                        <InfoCircleOutlined style={{ marginLeft: 4, fontSize: 10 }} />
                                      </Tooltip>
                                    </div>
                                    <div style={{ display: 'flex', gap: 4, marginBottom: 4 }}>
                                      <AutoComplete
                                        size="small"
                                        placeholder="搜索或粘贴 Open ID"
                                        value={whitelistBotId === botPushStatus.bot_id ? whitelistOpenId : undefined}
                                        onChange={(v) => { setWhitelistBotId(botPushStatus.bot_id); setWhitelistOpenId(v); }}
                                        onFocus={() => { if (whitelistBotId !== botPushStatus.bot_id) { loadGroupWhitelist(botPushStatus.bot_id); loadHistorySenders(); } }}
                                        filterOption={(input, option) => {
                                          if (!option?.value) return false;
                                          const val = (option.value as string).toLowerCase();
                                          const label = (option.label as string)?.toLowerCase() || '';
                                          const q = input.toLowerCase();
                                          return val.includes(q) || label.includes(q);
                                        }}
                                        style={{ flex: 1, fontSize: 11 }}
                                        options={historySenders
                                          .filter(s => s.sender_open_id)
                                          .map((s) => {
                                            const typeTag = s.sender_type === 'app' ? '[Bot] ' : '';
                                            const label = s.sender_nickname || s.sender_open_id;
                                            return {
                                              value: s.sender_open_id,
                                              label: `${typeTag}${label} (${s.count}条)`,
                                            };
                                          })
                                        }
                                      />
                                      <Input
                                        size="small"
                                        placeholder="备注名"
                                        value={whitelistBotId === botPushStatus.bot_id ? whitelistName : ''}
                                        onChange={(e) => { setWhitelistBotId(botPushStatus.bot_id); setWhitelistName(e.target.value); }}
                                        style={{ width: 80, fontSize: 11 }}
                                      />
                                      <Button size="small" onClick={handleAddWhitelist}>添加</Button>
                                    </div>
                                    {(whitelistBotId === botPushStatus.bot_id ? groupWhitelist : []).map((w) => (
                                      <div key={w.id} style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 11, marginBottom: 2 }}>
                                        <span style={{ color: 'var(--color-text)', flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                                          {w.sender_name || w.sender_open_id}
                                        </span>
                                        <span style={{ color: 'var(--color-text-tertiary)', fontSize: 10 }}>{w.sender_open_id.slice(0, 12)}...</span>
                                        <Button size="small" danger type="link" style={{ fontSize: 10, padding: 0 }} onClick={() => handleDeleteWhitelist(w.id)}>删除</Button>
                                      </div>
                                    ))}
                                    {(whitelistBotId === botPushStatus.bot_id ? groupWhitelist : []).length === 0 && (
                                      <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>暂无白名单，所有用户均可触发响应</div>
                                    )}
                                  </div>
                                )}
                              </div>
                            )}
                          </div>
                        );
                      }}
                    />
                  )}
                </Spin>
              </Card>

              <Modal
                open={!!historyViewMsg}
                onCancel={() => setHistoryViewMsg(null)}
                footer={null}
                width={560}
                title="消息详情"
              >
                <div style={{ fontSize: 13, lineHeight: 1.8, whiteSpace: 'pre-wrap', wordBreak: 'break-all', maxHeight: 400, overflowY: 'auto' }}>
                  {historyViewMsg}
                </div>
              </Modal>

              <Card
                title="斜杠命令规则"
                size="small"
                style={{ marginBottom: 24 }}
                extra={
                  <Button type="primary" size="small" onClick={handleSaveConfig} loading={configSaving}>
                    保存规则
                  </Button>
                }
              >
                <Paragraph type="secondary" style={{ marginBottom: 16, fontSize: 13 }}>
                  配置全局斜杠命令，将飞书消息中的命令路由到指定 Todo。命中后会把命令后的正文作为参数传入 Todo Prompt，支持使用 {'{{'}content{'}}'}、{'{{'}message{'}}'}、{'{{'}raw_message{'}}'}、{'{{'}slash_command{'}}'}。
                </Paragraph>
                <Form form={configForm} layout="vertical">
                  <Form.List name="slash_command_rules">
                    {(fields, { add, remove }) => (
                      <>
                        {fields.length === 0 && (
                          <Empty
                            image={Empty.PRESENTED_IMAGE_SIMPLE}
                            description="暂无规则，点击下方按钮新增"
                            style={{ margin: '12px 0 20px' }}
                          />
                        )}
                        {fields.map((field, index) => (
                          <Card
                            key={field.key}
                            size="small"
                            style={{ marginBottom: 12, background: 'var(--color-bg)' }}
                            title={`规则 ${index + 1}`}
                            extra={
                              <Button
                                type="text"
                                danger
                                size="small"
                                icon={<MinusCircleOutlined />}
                                onClick={() => remove(field.name)}
                              />
                            }
                          >
                            <Form.Item
                              name={[field.name, 'slash_command']}
                              label="斜杠命令"
                              rules={[
                                { required: true, message: '请输入斜杠命令' },
                                {
                                  validator: (_, value) => {
                                    const command = String(value || '').trim();
                                    if (!command) return Promise.resolve();
                                    if (!/^\/\S+$/.test(command)) {
                                      return Promise.reject(new Error('命令必须以 / 开头，且不能包含空格'));
                                    }
                                    return Promise.resolve();
                                  },
                                },
                              ]}
                            >
                              <Input placeholder="/todo" />
                            </Form.Item>
                            <Form.Item
                              name={[field.name, 'todo_id']}
                              label="目标 Todo"
                              rules={[{ required: true, message: '请选择目标 Todo' }]}
                            >
                              <Select
                                showSearch
                                placeholder="搜索并选择 Todo"
                                optionFilterProp="label"
                                options={todos.map((todo) => ({
                                  value: todo.id,
                                  label: `#${todo.id} ${todo.title}`,
                                }))}
                              />
                            </Form.Item>
                            <Form.Item
                              name={[field.name, 'enabled']}
                              label="启用"
                              valuePropName="checked"
                              initialValue={true}
                            >
                              <Switch size="small" />
                            </Form.Item>
                          </Card>
                        ))}
                        <Button
                          block
                          icon={<PlusOutlined />}
                          onClick={() => add({ slash_command: '', todo_id: undefined, enabled: true })}
                        >
                          新增规则
                        </Button>
                      </>
                    )}
                  </Form.List>
                </Form>
              </Card>

              <Card
                title="默认响应"
                size="small"
                style={{ marginBottom: 24 }}
                extra={
                  <Button type="primary" size="small" onClick={handleSaveConfig} loading={configSaving}>
                    保存
                  </Button>
                }
              >
                <Paragraph type="secondary" style={{ marginBottom: 16, fontSize: 13 }}>
                  当收到的消息没有匹配到任何斜杠命令时，执行默认响应的 Todo。支持使用 {'{{'}content{'}}'}、{'{{'}message{'}}'}、{'{{'}raw_message{'}}'}、{'{{'}slash_command{'}}'} 参数。
                </Paragraph>
                <Form form={configForm} layout="vertical" style={{ maxWidth: 400 }}>
                  <Form.Item
                    name="default_response_todo_id"
                    label="默认响应 Todo"
                  >
                    <Select
                      showSearch
                      allowClear
                      placeholder="选择默认响应的 Todo"
                      optionFilterProp="label"
                      options={todos.map((todo) => ({
                        value: todo.id,
                        label: `#${todo.id} ${todo.title}`,
                      }))}
                    />
                  </Form.Item>
                </Form>
              </Card>

              <Card
                title="历史消息处理"
                size="small"
                style={{ marginBottom: 24 }}
                extra={
                  <Button type="primary" size="small" onClick={handleSaveConfig} loading={configSaving}>
                    保存
                  </Button>
                }
              >
                <Paragraph type="secondary" style={{ marginBottom: 16, fontSize: 13 }}>
                  拉取历史消息时，超过设定时间的消息将保存但跳过处理，避免离线后重新处理大量旧消息。
                </Paragraph>
                <Form form={configForm} layout="vertical" style={{ maxWidth: 400 }}>
                  <Form.Item
                    name="history_message_max_age_secs"
                    label="最大处理年龄（秒）"
                    tooltip="仅处理此时间内的历史消息，默认 600 秒（10 分钟）"
                  >
                    <InputNumber
                      min={0}
                      max={86400}
                      step={60}
                      placeholder="600"
                      addonAfter="秒"
                      style={{ width: '100%' }}
                    />
                  </Form.Item>
                </Form>
              </Card>

              <Modal
                title={
                  <Space>
                    <QrcodeOutlined />
                    绑定飞书智能体
                  </Space>
                }
                open={bindModalOpen}
                onCancel={() => {
                  setBindModalOpen(false);
                  setQrCodeUrl('');
                  setPollError('');
                  setBindSuccess(false);
                }}
                footer={null}
                width={400}
                centered
                className="settings-bind-modal"
              >
                <div style={{ textAlign: 'center', padding: '16px 0' }}>
                  {pollError && (
                    <div style={{ marginBottom: 16, color: '#ff4d4f', fontSize: 13 }}>
                      {pollError}
                    </div>
                  )}

                  {bindSuccess ? (
                    <div style={{ color: '#52c41a', fontSize: 48, marginBottom: 16 }}>
                      ✓
                    </div>
                  ) : (
                    <>
                      {qrCodeUrl ? (
                        <div style={{ marginBottom: 16 }}>
                          <img src={qrCodeUrl} alt="QR Code" style={{ width: '100%', maxWidth: 200, height: 'auto' }} />
                          <div style={{ marginTop: 12, color: 'var(--color-text-secondary)', fontSize: 13 }}>
                            请使用飞书 App 扫描二维码绑定
                          </div>
                          <div style={{ marginTop: 6, fontSize: 12, color: 'var(--color-text-tertiary)' }}>
                            二维码有效期 10 分钟，请尽快完成
                          </div>
                        </div>
                      ) : (
                        <Spin size="large" />
                      )}
                    </>
                  )}

                  {binding && !qrCodeUrl && (
                    <div style={{ marginTop: 16, color: 'var(--color-text-secondary)', fontSize: 13 }}>
                      正在生成二维码...
                    </div>
                  )}
                </div>
              </Modal>
            </div>
          ),
        },
        {
          key: 'record',
          label: '记录',
          children: (
            <div className="settings-history-tab">
              <div
                style={{
                  marginBottom: 16,
                  display: 'flex',
                  flexWrap: 'wrap',
                  gap: 8,
                  justifyContent: 'space-between',
                  alignItems: 'center',
                }}
              >
                <Space>
                  <HistoryOutlined />
                  <span style={{ fontWeight: 600 }}>飞书群聊消息</span>
                  <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                    <Tooltip title={
                      <div style={{ fontSize: 12, lineHeight: 1.6 }}>
                        <b>实时消息：</b>通过 WebSocket 实时接收的消息，接收后立即处理并触发相关事件<br/>
                        <b>历史消息：</b>通过轮询 API 拉取的群聊历史记录，不会触发实时处理事件
                      </div>
                    }>
                      <span style={{ cursor: 'help' }}><QuestionCircleOutlined /></span>
                    </Tooltip>
                  </Typography.Text>
                </Space>
                <Space wrap>
                  <Select
                    placeholder="筛选群聊"
                    allowClear
                    style={{ width: 200 }}
                    value={historySelectedChatId}
                    onChange={(v) => {
                      setHistorySelectedChatId(v);
                      setHistoryPage(1);
                    }}
                    onClear={() => {
                      setHistorySelectedChatId(undefined);
                      setHistoryPage(1);
                    }}
                  >
                    {historyChats.map((chat) => (
                      <Option key={chat.chat_id} value={chat.chat_id}>
                        {chat.chat_name || chat.chat_id}
                      </Option>
                    ))}
                  </Select>
                  <Select
                    placeholder="筛选发送者"
                    allowClear
                    style={{ width: 150 }}
                    value={historySelectedSenderId}
                    onChange={(v) => {
                      setHistorySelectedSenderId(v);
                      setHistoryPage(1);
                    }}
                    onClear={() => {
                      setHistorySelectedSenderId(undefined);
                      setHistoryPage(1);
                    }}
                  >
                    {historySenders.map((item) => (
                      <Option key={item.sender_open_id} value={item.sender_open_id}>
                        {item.sender_nickname || item.sender_open_id.slice(0, 12)} ({item.count}条)
                      </Option>
                    ))}
                  </Select>
                  <Select
                    placeholder="消息来源"
                    style={{ width: 130 }}
                    value={historyIsHistory}
                    onChange={(v) => {
                      setHistoryIsHistory(v);
                      setHistoryPage(1);
                    }}
                    allowClear
                  >
                    <Option value={true}>仅历史消息</Option>
                    <Option value={false}>仅实时消息</Option>
                  </Select>
                  <Button icon={<ReloadOutlined />} onClick={loadHistoryMessages} size="middle">
                    刷新
                  </Button>
                  <Button type="primary" icon={<PlusOutlined />} onClick={() => setHistoryAddModalOpen(true)} size="middle">
                    添加
                  </Button>
                </Space>
              </div>

              <Table
                dataSource={historyMessages}
                rowKey="id"
                loading={historyLoading}
                scroll={{ x: 'max-content' }}
                pagination={{
                  current: historyPage,
                  pageSize: historyPageSize,
                  total: historyTotal,
                  showSizeChanger: true,
                  showQuickJumper: true,
                  showTotal: (t) => `共 ${t} 条`,
                  onChange: (p, ps) => {
                    setHistoryPage(p);
                    setHistoryPageSize(ps);
                  },
                }}
                size="middle"
                columns={[
                  {
                    title: '时间',
                    dataIndex: 'created_at',
                    key: 'created_at',
                    width: 150,
                    render: (text: string) => {
                      if (!text) return '-';
                      const d = new Date(text);
                      return isNaN(d.getTime()) ? text : d.toLocaleString('zh-CN');
                    },
                  },
                  {
                    title: '来源',
                    key: 'source',
                    width: 90,
                    render: (_, record) => (
                      <Tag color={record.is_history ? 'orange' : 'cyan'}>
                        {record.is_history ? '历史' : '实时'}
                      </Tag>
                    ),
                  },
                  {
                    title: '发送者',
                    key: 'sender',
                    width: 160,
                    render: (_, record) => {
                      const isBot = record.sender_type === 'app';
                      return (
                        <Space size={2}>
                          <Tag color={isBot ? 'blue' : 'green'}>
                            {isBot ? '智能体' : '用户'}
                          </Tag>
                          <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                            {record.sender_nickname || record.sender_open_id?.slice(0, 8) || '-'}
                          </Typography.Text>
                          {record.sender_open_id && (
                            <Button
                              size="small"
                              type="link"
                              icon={<CopyOutlined />}
                              style={{ fontSize: 10, padding: 0 }}
                              onClick={() => {
                                navigator.clipboard.writeText(record.sender_open_id);
                                message.success('已复制 Open ID');
                              }}
                            />
                          )}
                        </Space>
                      );
                    },
                  },
                  {
                    title: '内容',
                    dataIndex: 'content',
                    key: 'content',
                    width: 200,
                    render: (content: string, record) => {
                      let text: string;
                      if (record.msg_type === 'text') {
                        try {
                          const parsed = JSON.parse(content);
                          text = parsed.text || content;
                        } catch {
                          text = content;
                        }
                      } else {
                        return <Tag>{record.msg_type}</Tag>;
                      }
                      const MAX = 40;
                      const truncated = text.length > MAX ? text.slice(0, MAX) + '...' : text;
                      return (
                        <span
                          style={{ cursor: 'pointer', fontSize: 12 }}
                          onClick={() => setHistoryViewMsg(text)}
                        >
                          {truncated}
                        </span>
                      );
                    },
                  },
                  {
                    title: '处理状态',
                    key: 'processed',
                    width: 90,
                    render: (_, record) => (
                      record.processed ? (
                        <Tag color="green">已处理</Tag>
                      ) : (
                        <Tag color="default">未处理</Tag>
                      )
                    ),
                  },
                  {
                    title: '触发Todo',
                    key: 'processed_todo_id',
                    width: 80,
                    render: (_, record) => (
                      record.processed_todo_id ? (
                        <Typography.Link
                          style={{ fontSize: 12 }}
                          onClick={() => {
                            dispatch({ type: 'SELECT_TODO', payload: record.processed_todo_id });
                            onBack?.();
                          }}
                        >
                          #{record.processed_todo_id}
                        </Typography.Link>
                      ) : (
                        <span style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>-</span>
                      )
                    ),
                  },
                  {
                    title: '执行记录',
                    key: 'execution_record_id',
                    width: 80,
                    render: (_, record) => (
                      record.execution_record_id ? (
                        <Typography.Link
                          style={{ fontSize: 12 }}
                          onClick={() => {
                            db.getExecutionRecord(record.execution_record_id!)
                              .then(r => setExecDetailRecord(r))
                              .catch((err) => {
                                message.error('加载执行记录失败: ' + (err instanceof Error ? err.message : '未知错误'));
                              });
                          }}
                        >
                          #{record.execution_record_id}
                        </Typography.Link>
                      ) : (
                        <span style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>-</span>
                      )
                    ),
                  },
                ]}
              />

              <Modal
                title="添加监听群聊"
                open={historyAddModalOpen}
                onOk={handleAddHistoryChat}
                onCancel={() => {
                  setHistoryAddModalOpen(false);
                  historyForm.resetFields();
                }}
                width={520}
              >
                <Form form={historyForm} layout="vertical">
                  <Form.Item
                    name="bot_id"
                    label="机器人"
                    rules={[{ required: true, message: '请选择机器人' }]}
                  >
                    <Select placeholder="请选择机器人">
                      {agentBots.filter(b => b.bot_type === 'feishu').map((bot) => (
                        <Option key={bot.id} value={bot.id}>
                          {bot.bot_name}
                        </Option>
                      ))}
                    </Select>
                  </Form.Item>
                  <Form.Item
                    name="chat_id"
                    label="群聊 ID"
                    rules={[{ required: true, message: '请输入群聊 ID' }]}
                  >
                    <Input placeholder="请输入飞书群聊 ID" />
                  </Form.Item>
                  <Form.Item name="chat_name" label="群聊名称（可选）">
                    <Input placeholder="请输入群聊名称，方便识别" />
                  </Form.Item>
                </Form>
              </Modal>
            </div>
          ),
        },
      ]}
    />

    {/* Execution record detail modal */}
    <Modal
      title={execDetailRecord ? `执行记录 #${execDetailRecord.id}` : '执行记录'}
      open={!!execDetailRecord}
      onCancel={() => setExecDetailRecord(null)}
      footer={null}
      width={700}
    >
      {execDetailRecord && (
        <div style={{ maxHeight: '60vh', overflow: 'auto' }}>
          <div style={{ display: 'flex', gap: 16, marginBottom: 12, flexWrap: 'wrap' }}>
            <span><strong>状态:</strong> {execDetailRecord.status}</span>
            <span><strong>执行器:</strong> {execDetailRecord.executor || '-'}</span>
            <span><strong>触发:</strong> {execDetailRecord.trigger_type}</span>
            {execDetailRecord.model && <span><strong>模型:</strong> {execDetailRecord.model}</span>}
          </div>
          <div style={{ marginBottom: 8, fontSize: 12, color: 'var(--color-text-secondary)' }}>
            开始: {execDetailRecord.started_at ? new Date(execDetailRecord.started_at).toLocaleString() : '-'}
            {execDetailRecord.finished_at && ` | 结束: ${new Date(execDetailRecord.finished_at).toLocaleString()}`}
          </div>
          {execDetailRecord.result && (
            <div style={{ marginBottom: 12 }}>
              <strong>结果:</strong>
              <pre style={{ background: 'var(--color-fill-quaternary)', padding: 8, borderRadius: 4, fontSize: 12, maxHeight: 200, overflow: 'auto', whiteSpace: 'pre-wrap', marginTop: 4 }}>
                {execDetailRecord.result}
              </pre>
            </div>
          )}
          {execDetailRecord.stdout && (
            <div style={{ marginBottom: 12 }}>
              <strong>输出:</strong>
              <pre style={{ background: 'var(--color-fill-quaternary)', padding: 8, borderRadius: 4, fontSize: 12, maxHeight: 200, overflow: 'auto', whiteSpace: 'pre-wrap', marginTop: 4 }}>
                {execDetailRecord.stdout}
              </pre>
            </div>
          )}
          {execDetailRecord.stderr && (
            <div>
              <strong>错误:</strong>
              <pre style={{ background: 'var(--color-fill-quaternary)', padding: 8, borderRadius: 4, fontSize: 12, maxHeight: 150, overflow: 'auto', whiteSpace: 'pre-wrap', marginTop: 4, color: 'var(--color-error)' }}>
                {execDetailRecord.stderr}
              </pre>
            </div>
          )}
        </div>
      )}
    </Modal>
    </div>
  );
}
