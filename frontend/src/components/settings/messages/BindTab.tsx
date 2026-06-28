import { Card, Button, Spin, List, Empty, Select, AutoComplete, Input, InputNumber, Switch, Modal, Tag, Typography, Form, Tooltip, Space, Popconfirm, message, Radio } from 'antd';
import { QrcodeOutlined, DeleteOutlined, CopyOutlined, PlusOutlined, MinusCircleOutlined, InfoCircleOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';
import type { FeishuPushStatus, WhitelistEntry, FeishuSenderItem } from '@/utils/database';
import type { Todo } from '@/types';
import type { LoopListItem } from '@/types/loop';
import { useState, useEffect } from 'react';
import { copyToClipboard } from '@/utils/clipboard';

const { Paragraph } = Typography;

export function BindTab({
  agentBots, botsLoading, feishuPushStatus, whitelistBotId, groupWhitelist,
  whitelistOpenId, whitelistName, binding, bindModalOpen, qrCodeUrl, pollError, bindSuccess,
  historySenders, historyViewMsg, todos, configForm, configSaving, handleSaveConfig,
  setWhitelistOpenId, setWhitelistName, setBindModalOpen, setQrCodeUrl, setPollError, setBindSuccess,
  setWhitelistBotId, setHistoryViewMsg,
  onDeleteBot, onAddWhitelist, onDeleteWhitelist, onLoadGroupWhitelist, onLoadHistorySenders, onStartBind,
  onRefresh, onAfterBindModalClose, workspaceId,
}: {
  agentBots: db.AgentBot[];
  botsLoading: boolean;
  feishuPushStatus: FeishuPushStatus[];
  whitelistBotId: number | null;
  groupWhitelist: WhitelistEntry[];
  whitelistOpenId: string;
  whitelistName: string;
  binding: boolean;
  bindModalOpen: boolean;
  qrCodeUrl: string;
  pollError: string;
  bindSuccess: boolean;
  historySenders: FeishuSenderItem[];
  historyViewMsg: string | null;
  todos: Todo[];
  configForm: any;
  configSaving: boolean;
  handleSaveConfig: () => Promise<void>;
  setWhitelistOpenId: (v: string) => void;
  setWhitelistName: (v: string) => void;
  setBindModalOpen: (v: boolean) => void;
  setQrCodeUrl: (v: string) => void;
  setPollError: (v: string) => void;
  setBindSuccess: (v: boolean) => void;
  setWhitelistBotId: (v: number | null) => void;
  setHistoryViewMsg: (v: string | null) => void;
  onDeleteBot: (botId: number) => Promise<void>;
  onAddWhitelist: () => Promise<void>;
  onDeleteWhitelist: (id: number) => Promise<void>;
  onLoadGroupWhitelist: (botId: number) => void;
  onLoadHistorySenders: () => Promise<void>;
  onStartBind: () => Promise<void>;
  onRefresh: () => void;
  /** Called after the bind modal finishes its close animation — use this to reload bot list */
  onAfterBindModalClose?: () => void;
  workspaceId: number | null;
}) {
  // 内部状态：环路列表
  const [loops, setLoops] = useState<LoopListItem[]>([]);

  // 加载环路列表
  useEffect(() => {
    if (workspaceId) {
      db.listLoops(workspaceId).then(setLoops).catch(() => {});
    }
  }, [workspaceId]);

  return (
    <div className="settings-messages-tab" style={{ maxWidth: 700 }}>
      <Card
        title="绑定消息接收智能体"
        size="small"
        style={{ marginBottom: 24 }}
        extra={
          <Button type="primary" icon={<QrcodeOutlined />} onClick={onStartBind} loading={binding} size="small">
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
                    onRefresh();
                  } catch (e: any) {
                    message.error('保存配置失败: ' + (e.message || '未知错误'));
                  }
                };

                const botPushStatus = feishuPushStatus.find(p => p.bot_id === bot.id);
                const hasPushTarget = !!botPushStatus;
                const handlePushLevelChange = async (level: db.FeishuPushLevel) => {
                  try {
                    await db.updateFeishuPush({ botId: bot.id, pushLevel: level });
                    onRefresh();
                  } catch (e: any) {
                    message.error('设置推送失败: ' + (e.message || '未知错误'));
                  }
                };
                const handlePushTargetUpdate = async (field: 'p2p_receive_id' | 'receive_id_type' | 'group_chat_id', value: string) => {
                  try {
                    const updateField = field === 'p2p_receive_id' ? 'p2pReceiveId'
                      : field === 'group_chat_id' ? 'groupChatId' : 'receiveIdType';
                    await db.updateFeishuPush({ botId: bot.id, [updateField]: value });
                    onRefresh();
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
                    onRefresh();
                  } catch (e: any) {
                    message.error('更新响应开关失败: ' + (e.message || '未知错误'));
                  }
                };
                const doCopyText = async (text: string, label: string) => {
                  // 使用统一的复制工具（兼容 HTTP 环境）
                  const ok = await copyToClipboard(text);
                  if (ok) {
                    message.success(`${label} 已复制`);
                  } else {
                    message.error('复制失败');
                  }
                };

                return (
                  <div key={bot.id} style={{ padding: '12px', background: 'var(--color-bg)', borderRadius: 8, marginBottom: 8, border: '1px solid var(--color-border-light)' }}>
                    <div style={{ display: 'flex', alignItems: 'flex-start', gap: 10 }}>
                      <div style={{ width: 36, height: 36, borderRadius: 8, background: isFeishu ? '#1976D2' : '#888', display: 'flex', alignItems: 'center', justifyContent: 'center', color: '#fff', fontWeight: 700, fontSize: 14, flexShrink: 0 }}>
                        {isFeishu ? '飞' : '其他'}
                      </div>
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
                          <span style={{ fontWeight: 600, fontSize: 14, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{bot.bot_name}</span>
                          <Tag color={bot.enabled ? 'green' : 'default'} style={{ marginRight: 0 }}>
                            {bot.enabled ? '已启用' : '已禁用'}
                          </Tag>
                        </div>
                        <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', wordBreak: 'break-all', lineHeight: 1.6 }}>App ID: {bot.app_id}</div>
                        {bot.domain && <div style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>平台: {bot.domain === 'lark' ? 'Lark 国际版' : '飞书'}</div>}
                        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginTop: 2 }}>绑定时间: {new Date(bot.created_at).toLocaleString()}</div>
                      </div>
                      <Popconfirm title="删除确认" description={`确定要删除 "${bot.bot_name}" 吗？`} onConfirm={() => onDeleteBot(bot.id)} okText="删除" cancelText="取消" okButtonProps={{ danger: true }}>
                        <Button type="text" danger icon={<DeleteOutlined />} size="small" style={{ flexShrink: 0 }} />
                      </Popconfirm>
                    </div>
                    {isFeishu && (
                      <div style={{ marginTop: 8, paddingTop: 8, borderTop: '1px solid var(--color-border-light)' }}>
                        <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 6 }}>消息配置</div>
                        <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px 16px' }}>
                          <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
                            <Switch size="small" checked={botConfig.dm_enabled !== false} onChange={v => handleConfigChange('dm_enabled', v)} />接收单聊消息
                          </span>
                          <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
                            <Switch size="small" checked={botConfig.group_enabled !== false} onChange={v => handleConfigChange('group_enabled', v)} />接收群聊消息
                          </span>
                          <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
                            <Switch size="small" checked={botConfig.group_require_mention !== false} onChange={v => handleConfigChange('group_require_mention', v)} />群聊仅处理@
                          </span>
                          <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
                            <Switch size="small" checked={botConfig.echo_reply !== false} onChange={v => handleConfigChange('echo_reply', v)} />Echo 回复
                          </span>
                          {hasPushTarget && (<>
                            <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
                              <Switch size="small" checked={botPushStatus.p2p_response_enabled} onChange={(v) => handleResponseEnabledChange(botPushStatus.bot_id, 'p2p', v)} />单聊响应
                              <InputNumber size="small" min={1} max={300} value={botPushStatus.p2p_debounce_secs} onChange={(v) => { if (v !== null) db.updateFeishuPush({ botId: botPushStatus.bot_id, p2pDebounceSecs: v }); }} style={{ width: 50, fontSize: 11 }} />
                              <span style={{ fontSize: 10 }}>秒合并</span>
                            </span>
                            <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12 }}>
                              <Switch size="small" checked={botPushStatus.group_response_enabled} onChange={(v) => handleResponseEnabledChange(botPushStatus.bot_id, 'group', v)} />群聊响应
                              <InputNumber size="small" min={1} max={300} value={botPushStatus.group_debounce_secs} onChange={(v) => { if (v !== null) db.updateFeishuPush({ botId: botPushStatus.bot_id, groupDebounceSecs: v }); }} style={{ width: 50, fontSize: 11 }} />
                              <span style={{ fontSize: 10 }}>秒合并</span>
                            </span>
                          </>)}
                        </div>
                        {hasPushTarget && (
                          <div style={{ marginTop: 10 }}>
                            <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 6 }}>
                              推送目标
                              <Select size="small" value={botPushStatus.push_level} onChange={handlePushLevelChange} style={{ width: 90, marginLeft: 8 }}
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
                                <Input size="small" value={botPushStatus.p2p_receive_id} onChange={(e) => handlePushTargetUpdate('p2p_receive_id', e.target.value)} style={{ flex: 1, fontSize: 11 }} />
                                <Button size="small" icon={<CopyOutlined />} onClick={() => doCopyText(botPushStatus.p2p_receive_id, 'p2p_receive_id')} />
                              </div>
                              <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                                <span style={{ fontSize: 11, width: 80, color: 'var(--color-text-tertiary)' }}>群ID:</span>
                                <Input size="small" value={botPushStatus.group_chat_id || ''} onChange={(e) => handlePushTargetUpdate('group_chat_id', e.target.value)} style={{ flex: 1, fontSize: 11 }} />
                                <Button size="small" icon={<CopyOutlined />} onClick={() => doCopyText(botPushStatus.group_chat_id || '', 'group_chat_id')} />
                              </div>
                              <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                                <span style={{ fontSize: 11, width: 80, color: 'var(--color-text-tertiary)' }}>发送类型:</span>
                                <Select size="small" value={botPushStatus.receive_id_type} onChange={(v) => handlePushTargetUpdate('receive_id_type', v)} style={{ width: 100 }}
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
                                size="small" placeholder="搜索或粘贴 Open ID"
                                value={whitelistBotId === botPushStatus.bot_id ? whitelistOpenId : undefined}
                                onChange={(v) => { setWhitelistBotId(botPushStatus.bot_id); setWhitelistOpenId(v); }}
                                // 每次聚焦都加载最新数据：historySenders 追加模式可安全重复调用，
                                // groupWhitelist 按 bot 加载确保切回该 bot 时数据已刷新。
                                onFocus={() => { onLoadGroupWhitelist(botPushStatus.bot_id); onLoadHistorySenders(); }}
                                filterOption={(input, option) => {
                                  if (!option?.value) return false;
                                  const val = (option.value as string).toLowerCase();
                                  const label = (option.label as string)?.toLowerCase() || '';
                                  return val.includes(input.toLowerCase()) || label.includes(input.toLowerCase());
                                }}
                                style={{ flex: 1, fontSize: 11 }}
                                options={historySenders.filter(s => s.sender_open_id).map((s) => {
                                  const typeTag = s.sender_type === 'app' ? '[Bot] ' : '';
                                  return { value: s.sender_open_id, label: `${typeTag}${s.sender_nickname || s.sender_open_id} (${s.count}条)` };
                                })}
                              />
                              <Input size="small" placeholder="备注名" value={whitelistBotId === botPushStatus.bot_id ? whitelistName : ''}
                                onChange={(e) => { setWhitelistBotId(botPushStatus.bot_id); setWhitelistName(e.target.value); }} style={{ width: 80, fontSize: 11 }} />
                              <Button size="small" onClick={onAddWhitelist}>添加</Button>
                            </div>
                            {(whitelistBotId === botPushStatus.bot_id ? groupWhitelist : []).map((w) => (
                              <div key={w.id} style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 11, marginBottom: 2 }}>
                                <span style={{ color: 'var(--color-text)', flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                                  {w.sender_name || w.sender_open_id}
                                </span>
                                <span style={{ color: 'var(--color-text-tertiary)', fontSize: 10 }}>{w.sender_open_id.slice(0, 12)}...</span>
                                <Button size="small" danger type="link" style={{ fontSize: 10, padding: 0 }} onClick={() => onDeleteWhitelist(w.id)}>删除</Button>
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

      <Card title="斜杠命令规则" size="small" style={{ marginBottom: 24 }}
        extra={<Button type="primary" size="small" onClick={handleSaveConfig} loading={configSaving}>保存规则</Button>}
      >
        <Paragraph type="secondary" style={{ marginBottom: 16, fontSize: 13 }}>
          配置全局斜杠命令，将飞书消息中的命令路由到指定 Todo。命中后会把命令后的正文作为参数传入 Todo Prompt，支持使用 {'{{'}content{'}}'}、{'{{'}message{'}}'}、{'{{'}raw_message{'}}'}、{'{{'}slash_command{'}}'}。
        </Paragraph>
        <Form form={configForm} layout="vertical">
          <Form.List name="slash_command_rules">
            {(fields, { add, remove }) => (
              <>
                {fields.length === 0 && (
                  <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无规则，点击下方按钮新增" style={{ margin: '12px 0 20px' }} />
                )}
                {fields.map((field, index) => (
                  <Card key={field.key} size="small" style={{ marginBottom: 12, background: 'var(--color-bg)' }} title={`规则 ${index + 1}`}
                    extra={<Button type="text" danger size="small" icon={<MinusCircleOutlined />} onClick={() => remove(field.name)} />}
                  >
                    <Form.Item name={[field.name, 'slash_command']} label="斜杠命令" rules={[{ required: true, message: '请输入斜杠命令' }, { validator: (_, value) => { const command = String(value || '').trim(); if (!command) return Promise.resolve(); if (!/^\/\S+$/.test(command)) return Promise.reject(new Error('命令必须以 / 开头，且不能包含空格')); return Promise.resolve(); } }]}>
                      <Input placeholder="/todo" />
                    </Form.Item>
                    <Form.Item name={[field.name, 'todo_id']} label="目标 Todo" rules={[{ required: true, message: '请选择目标 Todo' }]}>
                      <Select showSearch placeholder="搜索并选择 Todo" optionFilterProp="label" options={todos.map((todo) => ({ value: todo.id, label: `#${todo.id} ${todo.title}` }))} />
                    </Form.Item>
                    <Form.Item name={[field.name, 'enabled']} label="启用" valuePropName="checked" initialValue={true}>
                      <Switch size="small" />
                    </Form.Item>
                  </Card>
                ))}
                <Button block icon={<PlusOutlined />} onClick={() => add({ slash_command: '', todo_id: undefined, enabled: true })}>新增规则</Button>
              </>
            )}
          </Form.List>
        </Form>
      </Card>

      <Card title="默认响应" size="small" style={{ marginBottom: 24 }}
        extra={<Button type="primary" size="small" onClick={handleSaveConfig} loading={configSaving}>保存</Button>}
      >
        <Paragraph type="secondary" style={{ marginBottom: 16, fontSize: 13 }}>
          当收到的消息没有匹配到任何斜杠命令时，执行默认响应。支持使用 {'{{'}content{'}}'}、{'{{'}message{'}}'}、{'{{'}raw_message{'}}'}、{'{{'}slash_command{'}}'} 参数。
        </Paragraph>
        <Form form={configForm} layout="vertical" style={{ maxWidth: 400 }}>
          <Form.Item name="default_response_type" label="响应类型" initialValue="todo">
            <Radio.Group>
              <Radio.Button value="todo">Todo</Radio.Button>
              <Radio.Button value="loop">环路</Radio.Button>
              <Radio.Button value="executor">执行器</Radio.Button>
            </Radio.Group>
          </Form.Item>

          {configForm?.getFieldValue?.('default_response_type') === 'todo' && (
            <Form.Item name="default_response_todo_id" label="默认响应 Todo">
              <Select showSearch allowClear placeholder="选择默认响应的 Todo" optionFilterProp="label" options={todos.map((todo) => ({ value: todo.id, label: `#${todo.id} ${todo.title}` }))} />
            </Form.Item>
          )}

          {configForm?.getFieldValue?.('default_response_type') === 'loop' && (
            <Form.Item name="default_response_loop_id" label="默认响应环路">
              <Select showSearch allowClear placeholder="选择默认响应的环路" optionFilterProp="label" options={loops.map((loop) => ({ value: loop.id, label: loop.name }))} />
            </Form.Item>
          )}

          {configForm?.getFieldValue?.('default_response_type') === 'executor' && (
            <Form.Item name="default_response_executor" label="执行器类型" initialValue="claudecode">
              <Select showSearch allowClear placeholder="选择执行器" options={[
                { value: 'claudecode', label: 'Claude Code' },
                { value: 'pi', label: 'PI' },
              ]} />
            </Form.Item>
          )}
        </Form>
      </Card>

      <Modal title={<Space><QrcodeOutlined />绑定飞书智能体</Space>} open={bindModalOpen}
        onCancel={() => { setBindModalOpen(false); setQrCodeUrl(''); setPollError(''); setBindSuccess(false); }}
        footer={null} width={400} centered className="settings-bind-modal"
        afterClose={onAfterBindModalClose}
      >
        <div style={{ textAlign: 'center', padding: '16px 0' }}>
          {pollError && <div style={{ marginBottom: 16, color: '#ff4d4f', fontSize: 13 }}>{pollError}</div>}
          {bindSuccess ? (
            <div style={{ color: '#52c41a', fontSize: 48, marginBottom: 16 }}>✓</div>
          ) : (
            <>
              {qrCodeUrl ? (
                <div style={{ marginBottom: 16 }}>
                  <img src={qrCodeUrl} alt="QR Code" style={{ width: '100%', maxWidth: 200, height: 'auto' }} />
                  <div style={{ marginTop: 12, color: 'var(--color-text-secondary)', fontSize: 13 }}>请使用飞书 App 扫描二维码绑定</div>
                  <div style={{ marginTop: 6, fontSize: 12, color: 'var(--color-text-tertiary)' }}>二维码有效期 10 分钟，请尽快完成</div>
                </div>
              ) : (
                <Spin size="large" />
              )}
            </>
          )}
          {binding && !qrCodeUrl && <div style={{ marginTop: 16, color: 'var(--color-text-secondary)', fontSize: 13 }}>正在生成二维码...</div>}
        </div>
      </Modal>
    </div>
  );
}
