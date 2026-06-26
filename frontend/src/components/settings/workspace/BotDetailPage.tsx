import { useState, useEffect } from 'react';
import { Card, Button, Switch, Input, Select, Tag, message, Modal, Typography, AutoComplete, Table } from 'antd';
import { ArrowLeftOutlined, CopyOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';
import type { AgentBot, FeishuPushStatus, WhitelistEntry, FeishuSenderItem } from '@/utils/database';
import type { FeishuHistoryMessage, FeishuHistoryChat, ExecutionRecord } from '@/types';
import { copyToClipboard } from '@/utils/clipboard';
import { ExecutionDetailModal } from '../messages/ExecutionDetailModal';

const { Paragraph } = Typography;

interface BotDetailPageProps {
  bot: AgentBot;
  onBack: () => void;
  onRefresh: () => void;
  /** 为 true 时，默认展开消息记录面板 */
  autoShowHistory?: boolean;
}

/**
 * Bot 详情页：单页展示 bot 基本配置和绑定信息，
 * 消息记录通过右上角按钮打开独立面板。
 */
export function BotDetailPage({ bot, onBack, onRefresh, autoShowHistory = false }: BotDetailPageProps) {
  const [botConfig, setBotConfig] = useState<Record<string, boolean>>({ dm_enabled: true, group_enabled: true, group_require_mention: true, echo_reply: true });
  const [pushStatus, setPushStatus] = useState<FeishuPushStatus | null>(null);
  const [groupWhitelist, setGroupWhitelist] = useState<WhitelistEntry[]>([]);
  const [whitelistOpenId, setWhitelistOpenId] = useState('');
  const [whitelistName, setWhitelistName] = useState('');
  const [historySenders, setHistorySenders] = useState<FeishuSenderItem[]>([]);

  // 消息记录状态
  const [historyMessages, setHistoryMessages] = useState<FeishuHistoryMessage[]>([]);
  const [historyChats, setHistoryChats] = useState<FeishuHistoryChat[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyTotal, setHistoryTotal] = useState(0);
  const [historyPage, setHistoryPage] = useState(1);
  const [historyPageSize, setHistoryPageSize] = useState(20);
  const [historySelectedChatId, setHistorySelectedChatId] = useState<string | undefined>(undefined);
  const [historyIsHistory, setHistoryIsHistory] = useState<boolean | undefined>(undefined);
  const [historySelectedSenderId] = useState<string | undefined>(undefined);
  const [historyViewMsg, setHistoryViewMsg] = useState<string | null>(null);
  const [execDetailRecord, setExecDetailRecord] = useState<ExecutionRecord | null>(null);
  const [todoDetail, setTodoDetail] = useState<{ id: number; title: string; prompt: string; status: string } | null>(null);
  const showHistory = autoShowHistory;

  // 加载 bot 配置
  useEffect(() => {
    try {
      const parsed = JSON.parse(bot.config || '{}');
      setBotConfig({ dm_enabled: true, group_enabled: true, group_require_mention: true, echo_reply: true, ...parsed });
    } catch {}
  }, [bot]);

  // 加载推送状态
  useEffect(() => {
    db.getFeishuPush().then(status => {
      const botStatus = status.find(s => s.bot_id === bot.id);
      setPushStatus(botStatus || null);
    }).catch(() => {});
  }, [bot.id]);

  // 加载白名单
  useEffect(() => {
    if (pushStatus) {
      db.getGroupWhitelist(bot.id).then(setGroupWhitelist).catch(() => setGroupWhitelist([]));
    }
  }, [bot.id, pushStatus]);

  // 加载历史发送者和群聊
  useEffect(() => {
    db.getFeishuSenders().then(setHistorySenders).catch(() => {});
    db.getFeishuHistoryChats().then(setHistoryChats).catch(() => {});
  }, []);

  // 加载历史消息（仅当显示消息面板时）
  useEffect(() => {
    if (showHistory) {
      loadHistoryMessages();
    }
  }, [historyPage, historyPageSize, historySelectedChatId, historyIsHistory, historySelectedSenderId, showHistory]);

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

  const handleViewExecutionRecord = async (recordId: number) => {
    try {
      const r = await db.getExecutionRecord(recordId);
      setExecDetailRecord(r);
    } catch {
      message.error('加载执行记录失败');
    }
  };

  const handleViewTodo = async (todoId: number) => {
    try {
      const t = await db.getTodo(todoId);
      setTodoDetail({ id: t.id, title: t.title, prompt: t.prompt, status: t.status });
    } catch {
      message.error('加载 Todo 详情失败');
    }
  };

  // 处理配置变更
  const handleConfigChange = async (key: string, val: boolean) => {
    const newConfig = { ...botConfig, [key]: val };
    try {
      await db.updateAgentBotConfig(bot.id, JSON.stringify(newConfig));
      setBotConfig(newConfig);
      onRefresh();
    } catch (e: any) {
      message.error('保存配置失败: ' + (e.message || '未知错误'));
    }
  };

  // 处理推送级别变更
  const handlePushLevelChange = async (level: db.FeishuPushLevel) => {
    try {
      await db.updateFeishuPush({ botId: bot.id, pushLevel: level });
      onRefresh();
    } catch (e: any) {
      message.error('设置推送失败: ' + (e.message || '未知错误'));
    }
  };

  // 处理响应开关变更
  const handleResponseEnabledChange = async (targetType: 'p2p' | 'group', enabled: boolean) => {
    try {
      if (targetType === 'p2p') {
        await db.updateFeishuPush({ botId: bot.id, p2pResponseEnabled: enabled });
      } else {
        await db.updateFeishuPush({ botId: bot.id, groupResponseEnabled: enabled });
      }
      onRefresh();
    } catch (e: any) {
      message.error('更新响应开关失败: ' + (e.message || '未知错误'));
    }
  };

  // 添加白名单
  const handleAddWhitelist = async () => {
    if (!whitelistOpenId.trim()) return;
    try {
      await db.addGroupWhitelist(bot.id, whitelistOpenId.trim(), whitelistName.trim() || undefined);
      setWhitelistOpenId('');
      setWhitelistName('');
      db.getGroupWhitelist(bot.id).then(setGroupWhitelist).catch(() => {});
    } catch (e: any) {
      message.error('添加白名单失败: ' + (e.message || '未知错误'));
    }
  };

  // 删除白名单
  const handleDeleteWhitelist = async (id: number) => {
    try {
      await db.deleteGroupWhitelist(id);
      db.getGroupWhitelist(bot.id).then(setGroupWhitelist).catch(() => {});
    } catch (e: any) {
      message.error('删除白名单失败: ' + (e.message || '未知错误'));
    }
  };

  // 复制文本
  const doCopyText = async (text: string, label: string) => {
    const ok = await copyToClipboard(text);
    if (ok) message.success(`${label} 已复制`);
    else message.error('复制失败');
  };

  const isFeishu = bot.bot_type === 'feishu';

  return (
    <div className="bot-detail-page">
      {/* 头部：返回、名称、标签 */}
      <div className="detail-header" style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 16 }}>
        <Button type="text" size="small" icon={<ArrowLeftOutlined />} onClick={onBack} className="back-btn" />
        <h3 className="card-title" style={{ margin: 0 }}>{bot.bot_name}</h3>
        <Tag color={bot.enabled ? 'green' : 'default'}>
          {bot.enabled ? '已启用' : '已禁用'}
        </Tag>
      </div>

      {/* 消息记录面板 */}
      {showHistory && (
        <Card title="历史消息" size="small" style={{ marginBottom: 16 }}>
          <div style={{ display: 'flex', gap: 12, marginBottom: 12, flexWrap: 'wrap' }}>
            <Select size="small" placeholder="筛选群聊" allowClear style={{ width: 150 }}
              value={historySelectedChatId}
              onChange={v => { setHistorySelectedChatId(v); setHistoryPage(1); }}
              options={historyChats.map(c => ({ value: c.chat_id, label: c.chat_name || c.chat_id }))}
            />
            <Select size="small" placeholder="消息类型" allowClear style={{ width: 120 }}
              value={historyIsHistory}
              onChange={v => { setHistoryIsHistory(v); setHistoryPage(1); }}
              options={[{ value: true, label: '历史消息' }, { value: false, label: '实时消息' }]}
            />
          </div>

          <Table
            dataSource={historyMessages}
            rowKey="id"
            loading={historyLoading}
            size="small"
            scroll={{ x: 'max-content' }}
            pagination={{
              current: historyPage,
              pageSize: historyPageSize,
              total: historyTotal,
              showSizeChanger: true,
              showQuickJumper: true,
              showTotal: (t: number) => `共 ${t} 条`,
              onChange: (p, ps) => { setHistoryPage(p); setHistoryPageSize(ps || 20); },
            }}
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
                width: 80,
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
                    <span style={{ fontSize: 12 }}>
                      <Tag color={isBot ? 'blue' : 'green'}>
                        {isBot ? '智能体' : '用户'}
                      </Tag>
                      {record.sender_nickname || record.sender_open_id?.slice(0, 8) || '-'}
                    </span>
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
                      text = content || '';
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
                    <Typography.Link style={{ fontSize: 12 }} onClick={() => handleViewTodo(record.processed_todo_id!)}>
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
                    <Typography.Link style={{ fontSize: 12 }} onClick={() => handleViewExecutionRecord(record.execution_record_id!)}>
                      #{record.execution_record_id}
                    </Typography.Link>
                  ) : (
                    <span style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>-</span>
                  )
                ),
              },
              {
                title: '工作空间',
                key: 'workspace_id',
                width: 80,
                render: (_, record) => (
                  record.workspace_id ? (
                    <Typography.Text style={{ fontSize: 12 }}>#{record.workspace_id}</Typography.Text>
                  ) : (
                    <span style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>-</span>
                  )
                ),
              },
            ]}
          />
        </Card>
      )}

      {/* 详情主体：基本配置 + 绑定信息合并（消息面板展开时隐藏） */}
      {!showHistory && (
      <div style={{ maxWidth: 700 }}>
        {/* Bot 基本信息 */}
        <Card title="基本信息" size="small" style={{ marginBottom: 16 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 12 }}>
            <div style={{ width: 36, height: 36, borderRadius: 8, background: isFeishu ? '#1976D2' : '#888', display: 'flex', alignItems: 'center', justifyContent: 'center', color: '#fff', fontWeight: 700, fontSize: 14 }}>
              {isFeishu ? '飞' : '其他'}
            </div>
            <div>
              <div style={{ fontWeight: 600, fontSize: 14 }}>{bot.bot_name}</div>
              <div style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>App ID: {bot.app_id}</div>
            </div>
          </div>

          <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px 16px' }}>
            <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 13 }}>
              <Switch size="small" checked={botConfig.dm_enabled !== false} onChange={v => handleConfigChange('dm_enabled', v)} />接收单聊消息
            </span>
            <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 13 }}>
              <Switch size="small" checked={botConfig.group_enabled !== false} onChange={v => handleConfigChange('group_enabled', v)} />接收群聊消息
            </span>
            <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 13 }}>
              <Switch size="small" checked={botConfig.group_require_mention !== false} onChange={v => handleConfigChange('group_require_mention', v)} />群聊仅处理@
            </span>
            <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 13 }}>
              <Switch size="small" checked={botConfig.echo_reply !== false} onChange={v => handleConfigChange('echo_reply', v)} />Echo 回复
            </span>
          </div>
        </Card>

        {/* 推送配置（仅飞书） */}
        {isFeishu && pushStatus && (
          <Card title="推送配置" size="small" style={{ marginBottom: 16 }}>
            <div style={{ marginBottom: 12 }}>
              <span style={{ fontSize: 13, marginRight: 8 }}>推送目标</span>
              <Select size="small" value={pushStatus.push_level} onChange={handlePushLevelChange} style={{ width: 90 }}
                options={[
                  { value: 'disabled', label: '关闭' },
                  { value: 'result_only', label: '仅结论' },
                  { value: 'all', label: '全部' },
                ]}
              />
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 4, marginBottom: 12 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                <span style={{ fontSize: 12, width: 60, color: 'var(--color-text-tertiary)' }}>单聊ID:</span>
                <Input size="small" value={pushStatus.p2p_receive_id} style={{ flex: 1, fontSize: 12 }} />
                <Button size="small" icon={<CopyOutlined />} onClick={() => doCopyText(pushStatus.p2p_receive_id, 'p2p_receive_id')} />
              </div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                <span style={{ fontSize: 12, width: 60, color: 'var(--color-text-tertiary)' }}>群ID:</span>
                <Input size="small" value={pushStatus.group_chat_id || ''} style={{ flex: 1, fontSize: 12 }} />
                <Button size="small" icon={<CopyOutlined />} onClick={() => doCopyText(pushStatus.group_chat_id || '', 'group_chat_id')} />
              </div>
            </div>
            <div style={{ display: 'flex', gap: 16, fontSize: 13 }}>
              <span style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                <Switch size="small" checked={pushStatus.p2p_response_enabled} onChange={v => handleResponseEnabledChange('p2p', v)} />单聊响应
              </span>
              <span style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                <Switch size="small" checked={pushStatus.group_response_enabled} onChange={v => handleResponseEnabledChange('group', v)} />群聊响应
              </span>
            </div>
          </Card>
        )}

        {/* 群聊响应白名单（仅飞书） */}
        {isFeishu && (
          <Card title="群聊响应白名单" size="small">
            <Paragraph type="secondary" style={{ fontSize: 13, marginBottom: 12 }}>
              白名单为空时不限制，仅白名单内的用户消息会触发响应
            </Paragraph>
            <div style={{ display: 'flex', gap: 8, marginBottom: 8 }}>
              <AutoComplete
                size="small" placeholder="搜索或粘贴 Open ID" style={{ flex: 1 }}
                value={whitelistOpenId}
                onChange={setWhitelistOpenId}
                options={historySenders.filter(s => s.sender_open_id).map(s => ({
                  value: s.sender_open_id,
                  label: `${s.sender_nickname || s.sender_open_id} (${s.count}条)`
                }))}
              />
              <Input size="small" placeholder="备注名" value={whitelistName} onChange={e => setWhitelistName(e.target.value)} style={{ width: 100 }} />
              <Button size="small" onClick={handleAddWhitelist}>添加</Button>
            </div>
            {groupWhitelist.map(w => (
              <div key={w.id} style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12, marginBottom: 4 }}>
                <span style={{ flex: 1 }}>{w.sender_name || w.sender_open_id}</span>
                <span style={{ color: 'var(--color-text-tertiary)', fontSize: 11 }}>{w.sender_open_id.slice(0, 12)}...</span>
                <Button size="small" danger type="link" style={{ fontSize: 11, padding: 0 }} onClick={() => handleDeleteWhitelist(w.id)}>删除</Button>
              </div>
            ))}
            {groupWhitelist.length === 0 && (
              <div style={{ fontSize: 12, color: 'var(--color-text-tertiary)' }}>暂无白名单，所有用户均可触发响应</div>
            )}
          </Card>
        )}
      </div>
      )}

      {/* 消息详情弹窗 */}
      <Modal open={!!historyViewMsg} onCancel={() => setHistoryViewMsg(null)} footer={null} width={560} title="消息详情">
        <div style={{ fontSize: 13, lineHeight: 1.8, whiteSpace: 'pre-wrap', wordBreak: 'break-all', maxHeight: 400, overflowY: 'auto' }}>
          {historyViewMsg}
        </div>
      </Modal>

      {/* 执行记录详情弹窗 */}
      <ExecutionDetailModal record={execDetailRecord} onClose={() => setExecDetailRecord(null)} />

      {/* Todo 详情弹窗 */}
      <Modal
        title={todoDetail ? `Todo #${todoDetail.id}` : 'Todo 详情'}
        open={!!todoDetail}
        onCancel={() => setTodoDetail(null)}
        footer={null}
        width={600}
      >
        {todoDetail && (
          <div>
            <div style={{ marginBottom: 8 }}>
              <Tag color={todoDetail.status === 'completed' ? 'green' : todoDetail.status === 'failed' ? 'red' : 'blue'}>
                {todoDetail.status}
              </Tag>
            </div>
            <div style={{ marginBottom: 8 }}>
              <strong>标题:</strong> <span>{todoDetail.title}</span>
            </div>
            <div>
              <strong>Prompt:</strong>
              <pre style={{ background: 'var(--color-fill-quaternary)', padding: 8, borderRadius: 4, fontSize: 12, maxHeight: 200, overflow: 'auto', whiteSpace: 'pre-wrap', marginTop: 4 }}>
                {todoDetail.prompt}
              </pre>
            </div>
          </div>
        )}
      </Modal>
    </div>
  );
}
