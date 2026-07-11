import { useState, useEffect } from 'react';
import { Drawer, Form, Input, Select, Switch, Button, Tag, Typography, Divider, Empty, Popconfirm } from 'antd';
import { SaveOutlined, PlusOutlined, DeleteOutlined, MessageOutlined, LockOutlined, SettingOutlined } from '@ant-design/icons';
import type { AgentBot, ProjectDirectory } from '@/utils/database';
import * as db from '@/utils/database';
import type { WhitelistEntry, FeishuPushLevel } from '@/utils/database/bots';

const { Text, Title } = Typography;
const { Option } = Select;

interface AssistantConfigDrawerProps {
  open: boolean;
  bot: AgentBot | null;
  workspaces: ProjectDirectory[];
  onClose: () => void;
  onChanged: () => void;
}

export function AssistantConfigDrawer({ open, bot, workspaces, onClose, onChanged }: AssistantConfigDrawerProps) {
  const [form] = Form.useForm();
  const [whitelist, setWhitelist] = useState<WhitelistEntry[]>([]);
  const [activeTab, setActiveTab] = useState<'push' | 'whitelist' | 'strategy'>('push');
  // bot 级接收策略开关：存储在 agent_bots.config JSON 中
  const [botConfig, setBotConfig] = useState<Record<string, boolean>>({
    dm_enabled: true,
    group_enabled: true,
    group_require_mention: true,
    echo_reply: true,
  });
  // 单聊/群聊接收 ID 用独立 state 管理(不放进与「白名单 tab」共享的 form 实例)：
  // 共享 form 实例在 tab 切换/抽屉动画挂载时字段绑定不可靠，曾导致输入框不回填已有值，
  // 用户一旦点「保存配置」就会把空串写回后端、擦掉原有 ID。改用 state：加载即回填、保存读 state。
  const [p2pReceiveId, setP2pReceiveId] = useState('');
  const [groupChatId, setGroupChatId] = useState('');

  useEffect(() => {
    if (open && bot) {
      loadConfig();
    }
  }, [open, bot]);

  const loadConfig = async () => {
    if (!bot) return;
    try {
      const [push, wl] = await Promise.all([
        db.getFeishuPush().then(list => list.find(p => p.bot_id === bot!.id) || null),
        db.getGroupWhitelist(bot!.id),
      ]);
      setWhitelist(wl);
      // 单聊/群聊接收 ID 写入独立 state，确保输入框一定能回填(见上方 state 注释)。
      setP2pReceiveId(push?.p2p_receive_id || '');
      setGroupChatId(push?.group_chat_id || '');
      form.setFieldsValue({
        pushLevel: push?.push_level || 'disabled',
        p2pResponseEnabled: push?.p2p_response_enabled || false,
        groupResponseEnabled: push?.group_response_enabled || false,
        p2pDebounceSecs: push?.p2p_debounce_secs || 60,
        groupDebounceSecs: push?.group_debounce_secs || 60,
      });
      // 解析 bot.config JSON，提取接收策略开关；默认全 true
      const defaults = { dm_enabled: true, group_enabled: true, group_require_mention: true, echo_reply: true };
      try {
        const parsed = JSON.parse(bot.config || '{}');
        setBotConfig({ ...defaults, ...parsed });
      } catch {
        setBotConfig(defaults);
      }
    } catch {}
  };

  const handleSavePush = async () => {
    if (!bot) return;
    const values = form.getFieldsValue();
    try {
      await db.updateFeishuPush({
        botId: bot.id,
        pushLevel: values.pushLevel as FeishuPushLevel,
        // 单聊/群聊接收 ID 直接读 state(非 form)，避免共享 form 实例绑定不可靠导致写空。
        p2pReceiveId,
        groupChatId,
        p2pResponseEnabled: values.p2pResponseEnabled,
        groupResponseEnabled: values.groupResponseEnabled,
        p2pDebounceSecs: values.p2pDebounceSecs,
        groupDebounceSecs: values.groupDebounceSecs,
      });
      onChanged();
    } catch {}
  };

  const handleAddWhitelist = async () => {
    if (!bot) return;
    const { senderOpenId, senderName } = form.getFieldsValue();
    if (!senderOpenId) return;
    try {
      await db.addGroupWhitelist(bot.id, senderOpenId, senderName);
      loadConfig();
      form.setFieldsValue({ senderOpenId: '', senderName: '' });
    } catch {}
  };

  const handleDeleteWhitelist = async (id: number) => {
    try {
      await db.deleteGroupWhitelist(id);
      loadConfig();
    } catch {}
  };

  const handleMoveWorkspace = async (workspaceId: number) => {
    if (!bot) return;
    try {
      await db.moveBotToWorkspace(bot.id, workspaceId);
      onChanged();
    } catch {}
  };

  // 保存 bot 级接收策略开关到 agent_bots.config JSON
  const handleSaveBotConfig = async () => {
    if (!bot) return;
    try {
      await db.updateAgentBotConfig(bot.id, JSON.stringify(botConfig));
      onChanged();
    } catch {}
  };

  if (!bot) return null;

  return (
    <Drawer
      title={`${bot.bot_name} - 配置`}
      open={open}
      onClose={onClose}
      width={480}
      footer={
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
          <Button onClick={onClose}>取消</Button>
          <Button type="primary" icon={<SaveOutlined />} onClick={handleSavePush}>保存配置</Button>
        </div>
      }
    >
      <div style={{ marginBottom: 20 }}>
        <Title level={5} style={{ margin: 0 }}>基本信息</Title>
        <div style={{ display: 'flex', alignItems: 'center', gap: 16, marginTop: 12 }}>
          <div>
            <Text style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>智能体类型</Text>
            <div style={{ fontSize: 14, fontWeight: 500, marginTop: 4 }}>
              {bot.bot_type === 'feishu' ? '飞书智能体' : bot.bot_type}
            </div>
          </div>
          <div>
            <Text style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>App ID</Text>
            <code style={{ fontSize: 12, display: 'block', marginTop: 4 }}>{bot.app_id}</code>
          </div>
          <div>
            <Text style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>状态</Text>
            <Tag color={bot.enabled ? 'green' : 'red'} style={{ marginTop: 4, display: 'block' }}>
              {bot.enabled ? '运行中' : '已停用'}
            </Tag>
          </div>
        </div>
      </div>

      <div style={{ marginBottom: 20 }}>
        <Title level={5} style={{ margin: 0 }}>服务工作空间</Title>
        <p style={{ fontSize: 12, color: 'var(--color-text-secondary)', margin: '4px 0 12px' }}>
          选择智能体当前服务的工作空间，切换后智能体将服务新的工作空间
        </p>
        <Select
          value={bot.workspace_id}
          onChange={handleMoveWorkspace}
          style={{ width: '100%' }}
          placeholder="选择工作空间"
          allowClear
        >
          {workspaces.map(w => (
            <Option key={w.id} value={w.id}>{w.name}</Option>
          ))}
        </Select>
      </div>

      <Divider />

      <div style={{ display: 'flex', gap: 8, marginBottom: 20 }}>
        <Button
          type={activeTab === 'strategy' ? 'primary' : 'text'}
          icon={<SettingOutlined />}
          onClick={() => setActiveTab('strategy')}
        >
          接收策略
        </Button>
        <Button
          type={activeTab === 'push' ? 'primary' : 'text'}
          icon={<MessageOutlined />}
          onClick={() => setActiveTab('push')}
        >
          推送规则
        </Button>
        <Button
          type={activeTab === 'whitelist' ? 'primary' : 'text'}
          icon={<LockOutlined />}
          onClick={() => setActiveTab('whitelist')}
        >
          群聊白名单
        </Button>
      </div>

      {activeTab === 'strategy' && (
        <div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
              <div>
                <Text style={{ fontSize: 13, fontWeight: 500 }}>接收单聊消息</Text>
                <div style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>开启后智能体会处理私聊消息</div>
              </div>
              <Switch
                checked={botConfig.dm_enabled !== false}
                onChange={v => setBotConfig(prev => ({ ...prev, dm_enabled: v }))}
              />
            </div>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
              <div>
                <Text style={{ fontSize: 13, fontWeight: 500 }}>接收群聊消息</Text>
                <div style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>开启后智能体会处理群聊消息</div>
              </div>
              <Switch
                checked={botConfig.group_enabled !== false}
                onChange={v => setBotConfig(prev => ({ ...prev, group_enabled: v }))}
              />
            </div>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
              <div>
                <Text style={{ fontSize: 13, fontWeight: 500 }}>群聊仅处理 @</Text>
                <div style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>开启后群聊中只有 @智能体 的消息才会被处理</div>
              </div>
              <Switch
                checked={botConfig.group_require_mention !== false}
                onChange={v => setBotConfig(prev => ({ ...prev, group_require_mention: v }))}
              />
            </div>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
              <div>
                <Text style={{ fontSize: 13, fontWeight: 500 }}>Echo 回复</Text>
                <div style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>开启后智能体会回复确认消息</div>
              </div>
              <Switch
                checked={botConfig.echo_reply !== false}
                onChange={v => setBotConfig(prev => ({ ...prev, echo_reply: v }))}
              />
            </div>
          </div>
          <Button
            type="primary"
            style={{ marginTop: 16 }}
            onClick={handleSaveBotConfig}
          >
            保存接收策略
          </Button>
        </div>
      )}

      {activeTab === 'push' && (
        <div>
          {/* /sethome 提示常驻：即使尚未设置 ID，也让用户知道可在单聊/群聊发 /sethome 自动填，
              或直接在下方输入框手动粘贴（例如从「消息监控台」复制群聊ID/私聊ID）。 */}
          <div style={{ marginBottom: 12, padding: '8px 12px', background: 'var(--color-fill-secondary)', borderRadius: 6, fontSize: 12, color: 'var(--color-text-secondary)' }}>
            💡 在单聊或群聊中发送 <code style={{ padding: '1px 6px', background: 'var(--color-fill)', borderRadius: 4 }}>/sethome</code> 可自动填写下方 ID；也可直接手动粘贴。
          </div>
          <Form form={form} layout="vertical">
          {/* 推送目标 ID：受控于独立 state(不绑定 form)，随「保存配置」持久化(对应 /sethome 写入的同一字段)。
              不设 name，避免进入共享 form 实例的字段注册导致回填/保存不稳定。 */}
          <Form.Item label="单聊接收 ID" tooltip="私聊场景的接收 open_id，形如 ou_xxxxxxxx">
            <Input
              value={p2pReceiveId}
              onChange={(e) => setP2pReceiveId(e.target.value)}
              placeholder="ou_xxxxxxxx（可手动粘贴，或发 /sethome 自动填）"
            />
          </Form.Item>
          <Form.Item label="群聊接收 ID" tooltip="群聊场景的接收 chat_id，形如 oc_xxxxxxxx">
            <Input
              value={groupChatId}
              onChange={(e) => setGroupChatId(e.target.value)}
              placeholder="oc_xxxxxxxx（可手动粘贴，或发 /sethome 自动填）"
            />
          </Form.Item>
          <Form.Item
            label="推送级别"
            name="pushLevel"
            rules={[{ required: true, message: '请选择推送级别' }]}
          >
            <Select style={{ width: '100%' }}>
              <Option value="disabled">不推送</Option>
              <Option value="result_only">仅推送结果</Option>
              <Option value="all">推送全部</Option>
            </Select>
          </Form.Item>

          <Form.Item label="私聊响应" name="p2pResponseEnabled" valuePropName="checked">
            <Switch />
          </Form.Item>

          <Form.Item label="群聊响应" name="groupResponseEnabled" valuePropName="checked">
            <Switch />
          </Form.Item>

          <Form.Item label="私聊防抖（秒）" name="p2pDebounceSecs">
            <Input type="number" min={0} max={3600} />
          </Form.Item>

          <Form.Item label="群聊防抖（秒）" name="groupDebounceSecs">
            <Input type="number" min={0} max={3600} />
          </Form.Item>
        </Form>
        </div>
      )}

      {activeTab === 'whitelist' && (
        <div>
          {/* 白名单作用说明：与推送 tab 的 /sethome 提示同款样式，让用户一眼明白
              「白名单 = 仅处理这些指定人员在群聊里发的消息，其他人发的群消息会被忽略」。 */}
          <div style={{ marginBottom: 12, padding: '8px 12px', background: 'var(--color-fill-secondary)', borderRadius: 6, fontSize: 12, color: 'var(--color-text-secondary)' }}>
            💡 仅处理白名单内指定人员在群聊中发送的消息；其他人发送的群消息将被忽略。在下方填入发送者 Open ID 即可加入白名单。
          </div>
          <Form form={form} layout="inline" style={{ marginBottom: 16 }}>
            <Form.Item name="senderOpenId">
              <Input placeholder="发送者 Open ID" style={{ width: 200 }} />
            </Form.Item>
            <Form.Item name="senderName">
              <Input placeholder="发送者名称（可选）" style={{ width: 150 }} />
            </Form.Item>
            <Form.Item>
              <Button type="primary" icon={<PlusOutlined />} onClick={handleAddWhitelist}>添加</Button>
            </Form.Item>
          </Form>

          {whitelist.length === 0 ? (
            <Empty description="暂无白名单，添加后允许该用户在群聊中使用智能体" />
          ) : (
            <div style={{ maxHeight: 300, overflowY: 'auto' }}>
              {whitelist.map(item => (
                <div
                  key={item.id}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    padding: '8px 12px',
                    backgroundColor: 'var(--color-bg-secondary)',
                    borderRadius: 4,
                    marginBottom: 8,
                  }}
                >
                  <div>
                    <Text style={{ fontSize: 13 }}>{item.sender_name || item.sender_open_id}</Text>
                    {item.sender_name && (
                      <Text style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginLeft: 8 }}>
                        {item.sender_open_id}
                      </Text>
                    )}
                  </div>
                  <Popconfirm title="确定删除该白名单？" onConfirm={() => handleDeleteWhitelist(item.id)}>
                    <Button type="text" size="small" icon={<DeleteOutlined />} danger>删除</Button>
                  </Popconfirm>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </Drawer>
  );
}