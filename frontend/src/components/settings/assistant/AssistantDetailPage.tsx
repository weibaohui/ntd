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
import { Button, Tag, Modal, message } from 'antd';
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
        <div style={{ width: '100%' }}>
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
