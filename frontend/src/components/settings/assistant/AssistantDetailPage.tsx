// Assistant 详情页：单页展示 assistant 基本配置、推送状态、群聊白名单和消息记录。
//
// 状态说明：
// - showHistory=true：显示消息记录面板（autoShowHistory 从外部控制）
// - showHistory=false：显示基本配置区（AssistantConfigCard + PushStatusCard + WhitelistCard）
//
// 子组件划分：
// - AssistantConfigCard：基本开关配置
// - PushStatusCard：飞书推送配置
// - WhitelistCard：群聊白名单管理
// - HistoryTable：历史消息分页表格
// - ExecutionRecordDrawer / BlackboardDrawer：执行详情弹窗/抽屉（外部引入）

import { useState, useEffect } from 'react';
import { Button, Tag, Modal, message, Card, Input } from 'antd';
import { ArrowLeftOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';
import * as dbLoops from '@/utils/database/loops';
import type { AgentBot, FeishuPushStatus, WhitelistEntry, FeishuSenderItem, FeishuPushLevel } from '@/utils/database';
import type { FeishuHistoryMessage, FeishuHistoryChat, ExecutionRecord } from '@/types';
import { ExecutionRecordDrawer } from '@/components/settings/messages/ExecutionRecordDrawer';
import { BlackboardDrawer } from '@/components/loop-studio/executions/BlackboardDrawer';
import { AssistantConfigCard } from './AssistantConfigCard';
import { PushStatusCard } from './PushStatusCard';
import { WhitelistCard } from './WhitelistCard';
import { HistoryTable } from './HistoryTable';

interface AssistantDetailPageProps {
  bot: AgentBot;
  onBack: () => void;
  onRefresh: () => void;
  /** 为 true 时，默认展开消息记录面板 */
  autoShowHistory?: boolean;
}

export function AssistantDetailPage({ bot, onBack, onRefresh, autoShowHistory = false }: AssistantDetailPageProps) {
  const [botConfig, setBotConfig] = useState<Record<string, boolean>>({
    dm_enabled: true, group_enabled: true, group_require_mention: true, echo_reply: true,
  });
  const [pushStatus, setPushStatus] = useState<FeishuPushStatus | null>(null);
  const [groupWhitelist, setGroupWhitelist] = useState<WhitelistEntry[]>([]);
  const [whitelistOpenId, setWhitelistOpenId] = useState('');
  const [whitelistName, setWhitelistName] = useState('');
  // 历史拉取群添加表单（chat_id + 备注），替代旧的 /sethome 隐式写入
  const [histChatId, setHistChatId] = useState('');
  const [histChatName, setHistChatName] = useState('');
  const [historySenders, setHistorySenders] = useState<FeishuSenderItem[]>([]);

  // 消息记录分页状态
  const [historyMessages, setHistoryMessages] = useState<FeishuHistoryMessage[]>([]);
  const [historyChats, setHistoryChats] = useState<FeishuHistoryChat[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyTotal, setHistoryTotal] = useState(0);
  const [historyPage, setHistoryPage] = useState(1);
  const [historyPageSize, setHistoryPageSize] = useState(20);
  const [historySelectedChatId, setHistorySelectedChatId] = useState<string | undefined>(undefined);
  const [historyIsHistory, setHistoryIsHistory] = useState<boolean | undefined>(undefined);

  // 详情抽屉/弹窗状态
  const [historyViewMsg, setHistoryViewMsg] = useState<string | null>(null);
  const [execDetailRecord, setExecDetailRecord] = useState<ExecutionRecord | null>(null);
  const [blackboardOpen, setBlackboardOpen] = useState(false);
  const [blackboardExecs, setBlackboardExecs] = useState<Record<string, any>[]>([]);

  const showHistory = autoShowHistory;

  // ─── 加载 bot 配置 ───
  useEffect(() => {
    try {
      const parsed = JSON.parse(bot.config || '{}');
      setBotConfig({ dm_enabled: true, group_enabled: true, group_require_mention: true, echo_reply: true, ...parsed });
    } catch {}
  }, [bot]);

  // ─── 加载推送状态 ───
  useEffect(() => {
    db.getFeishuPush().then(status => {
      const botStatus = status.find(s => s.bot_id === bot.id);
      setPushStatus(botStatus || null);
    }).catch(() => {});
  }, [bot.id]);

  // ─── 加载白名单 ───
  useEffect(() => {
    if (pushStatus) {
      db.getGroupWhitelist(bot.id).then(setGroupWhitelist).catch(() => setGroupWhitelist([]));
    }
  }, [bot.id, pushStatus]);

  // ─── 加载历史发送者和群聊 ───
  useEffect(() => {
    db.getFeishuSenders().then(setHistorySenders).catch(() => {});
    db.getFeishuHistoryChats().then(setHistoryChats).catch(() => {});
  }, []);

  // ─── 加载历史消息（仅当显示消息面板时） ───
  useEffect(() => {
    if (showHistory) {
      loadHistoryMessages();
    }
  }, [historyPage, historyPageSize, historySelectedChatId, historyIsHistory, showHistory]);

  // ─── 加载执行记录详情 ───
  const handleViewExecutionRecord = async (recordId: number) => {
    try {
      const r = await db.getExecutionRecord(recordId);
      setExecDetailRecord(r);
    } catch {
      message.error('加载执行记录失败');
    }
  };

  // ─── 点击处理类型（环路类型）───
  const handleProcessedTypeClick = async (record: FeishuHistoryMessage) => {
    if (record.processed_type !== 'slash_command_loop' || !record.processed_id) return;
    try {
      const detail = await dbLoops.getExecutionById(record.processed_id);
      setBlackboardExecs(detail.step_executions || []);
      setBlackboardOpen(true);
    } catch {
      message.error('加载环路执行详情失败');
    }
  };

  // ─── 加载历史消息 ───
  const loadHistoryMessages = async () => {
    setHistoryLoading(true);
    try {
      const data = await db.getFeishuHistoryMessages({
        chat_id: historySelectedChatId,
        is_history: historyIsHistory,
        sender_open_id: undefined,
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

  // ─── 配置变更 ───
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

  // ─── 推送级别变更 ───
  const handlePushLevelChange = async (level: FeishuPushLevel) => {
    try {
      await db.updateFeishuPush({ botId: bot.id, pushLevel: level });
      // 直接更新本地状态，避免依赖 onRefresh 重新加载导致下拉框不刷新
      setPushStatus(prev => prev ? { ...prev, push_level: level } : prev);
      onRefresh();
    } catch (e: any) {
      message.error('设置推送失败: ' + (e.message || '未知错误'));
    }
  };

  // ─── 响应开关变更 ───
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

  // ─── 添加白名单 ───
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

  // ─── 删除白名单 ───
  const handleDeleteWhitelist = async (id: number) => {
    try {
      await db.deleteGroupWhitelist(id);
      db.getGroupWhitelist(bot.id).then(setGroupWhitelist).catch(() => {});
    } catch (e: any) {
      message.error('删除白名单失败: ' + (e.message || '未知错误'));
    }
  };

  // ─── 历史拉取群管理：用户在前端填写群 chat_id，替代旧 /sethome 隐式写入 group_chat_id ───
  const reloadHistoryChats = () => {
    db.getFeishuHistoryChats().then(setHistoryChats).catch(() => {});
  };
  const handleAddHistChat = async () => {
    // chat_id 必填、备注可选；空 chat_id 直接忽略，避免误创建空记录
    if (!histChatId.trim()) return;
    try {
      await db.createFeishuHistoryChat(bot.id, histChatId.trim(), histChatName.trim() || undefined);
      setHistChatId('');
      setHistChatName('');
      reloadHistoryChats();
    } catch (e: any) {
      message.error('添加拉取群失败: ' + (e.message || '未知错误'));
    }
  };
  const handleDeleteHistChat = async (id: number) => {
    try {
      await db.deleteFeishuHistoryChat(id);
      reloadHistoryChats();
    } catch (e: any) {
      message.error('删除拉取群失败: ' + (e.message || '未知错误'));
    }
  };

  const isFeishu = bot.bot_type === 'feishu';
  // historyChats 全局加载（含所有 bot），渲染时按当前 bot 过滤
  const botHistChats = historyChats.filter(c => c.bot_id === bot.id);

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
        <HistoryTable
          messages={historyMessages}
          chats={historyChats}
          loading={historyLoading}
          page={historyPage}
          pageSize={historyPageSize}
          total={historyTotal}
          selectedChatId={historySelectedChatId}
          isHistory={historyIsHistory}
          onChatChange={setHistorySelectedChatId}
          onHistoryChange={setHistoryIsHistory}
          onRefresh={loadHistoryMessages}
          onPageChange={(p, ps) => { setHistoryPage(p); setHistoryPageSize(ps || 20); }}
          onViewExecutionRecord={handleViewExecutionRecord}
          onProcessedTypeClick={handleProcessedTypeClick}
        />
      )}

      {/* 基本配置区（消息面板展开时隐藏） */}
      {!showHistory && (
        <div style={{ maxWidth: 700 }}>
          {/* Bot 基本信息 */}
          <AssistantConfigCard bot={bot} botConfig={botConfig} onConfigChange={handleConfigChange} />

          {/* 推送配置（仅飞书） */}
          {isFeishu && pushStatus && (
            <PushStatusCard
              pushStatus={pushStatus}
              onPushLevelChange={handlePushLevelChange}
              onResponseEnabledChange={handleResponseEnabledChange}
            />
          )}

          {/* 群聊响应白名单（仅飞书） */}
          {isFeishu && (
            <WhitelistCard
              whitelist={groupWhitelist}
              historySenders={historySenders}
              whitelistOpenId={whitelistOpenId}
              whitelistName={whitelistName}
              onOpenIdChange={setWhitelistOpenId}
              onNameChange={setWhitelistName}
              onAdd={handleAddWhitelist}
              onDelete={handleDeleteWhitelist}
            />
          )}

          {/* 历史消息拉取群（仅飞书）：用户填写要拉取历史的群 chat_id，替代旧 /sethome 隐式写入 */}
          {isFeishu && (
            <Card title="历史消息拉取群" size="small" style={{ marginBottom: 16 }}>
              <div style={{ fontSize: 13, color: 'var(--color-text-secondary)', marginBottom: 12 }}>
                填写需要定期拉取历史消息的群 chat_id（形如 oc_xxxxxxxx）
              </div>
              <div style={{ display: 'flex', gap: 8, marginBottom: 8 }}>
                <Input size="small" placeholder="群 chat_id（oc_xxxxxxxx）" style={{ flex: 1 }} value={histChatId} onChange={e => setHistChatId(e.target.value)} />
                <Input size="small" placeholder="群名称备注" style={{ width: 120 }} value={histChatName} onChange={e => setHistChatName(e.target.value)} />
                <Button size="small" onClick={handleAddHistChat}>添加</Button>
              </div>
              {botHistChats.map(c => (
                <div key={c.id} style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12, marginBottom: 4 }}>
                  <span style={{ flex: 1 }}>{c.chat_name || c.chat_id}</span>
                  <span style={{ color: 'var(--color-text-tertiary)', fontSize: 11 }}>{c.chat_id.slice(0, 12)}...</span>
                  <Button size="small" danger type="link" style={{ fontSize: 11, padding: 0 }} onClick={() => handleDeleteHistChat(c.id)}>删除</Button>
                </div>
              ))}
              {botHistChats.length === 0 && (
                <div style={{ fontSize: 12, color: 'var(--color-text-tertiary)' }}>暂无拉取群</div>
              )}
            </Card>
          )}
        </div>
      )}

      {/* 消息详情弹窗 */}
      <Modal
        open={!!historyViewMsg}
        onCancel={() => setHistoryViewMsg(null)}
        footer={null}
        width={560}
        title="消息详情"
      >
        <div style={{
          fontSize: 13, lineHeight: 1.8, whiteSpace: 'pre-wrap',
          wordBreak: 'break-all', maxHeight: 400, overflowY: 'auto',
        }}>
          {historyViewMsg}
        </div>
      </Modal>

      {/* 执行记录详情抽屉 */}
      <ExecutionRecordDrawer record={execDetailRecord} onClose={() => setExecDetailRecord(null)} />

      {/* 黑板抽屉：slash_command_loop 环路执行详情 */}
      <BlackboardDrawer
        open={blackboardOpen}
        stepExecs={blackboardExecs}
        onClose={() => setBlackboardOpen(false)}
      />
    </div>
  );
}
