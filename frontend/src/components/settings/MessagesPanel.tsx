import { useState, useEffect, useRef } from 'react';
import { Tabs, Form, message } from 'antd';
import QRCode from 'qrcode';
import { useApp } from '@/hooks/useApp';
import * as db from '@/utils/database';
import type { FeishuPushStatus, WhitelistEntry } from '@/utils/database';
import type { FeishuHistoryMessage, FeishuHistoryChat, ExecutionRecord } from '@/types';
import { BindTab } from './messages/BindTab';
import { ProjectBindsTab } from './messages/ProjectBindsTab';
import { RecordTab } from './messages/RecordTab';
import { ExecutionDetailModal } from './messages/ExecutionDetailModal';

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
  // 保存 SSE 连接，组件卸载时关闭
  const [feishuEventSource, setFeishuEventSource] = useState<EventSource | null>(null);
  // 保存成功提示 timer，用于取消重复绑定时的旧 timer
  const successTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

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

  // 组件卸载时关闭 SSE 连接
  useEffect(() => {
    return () => {
      feishuEventSource?.close();
    };
  }, [feishuEventSource]);

  // hasPushTarget 为 true 时（feishuPushStatus 非空），预加载群聊响应白名单。
  // 这样进入绑定 Tab 时，白名单区域无需等用户聚焦输入框就已有数据。
  useEffect(() => {
    if (feishuPushStatus.length > 0 && agentBots.length > 0) {
      // 找到第一个有推送目标的 bot，加载其白名单
      const firstPushTarget = feishuPushStatus.find(ps => agentBots.some(b => b.id === ps.bot_id));
      if (firstPushTarget) {
        loadGroupWhitelist(firstPushTarget.bot_id);
      }
    }
  }, [feishuPushStatus, agentBots]);

  // 加载历史发送者列表，供白名单 AutoComplete 使用。
  useEffect(() => {
    loadHistoryChats();
    loadHistorySenders();
  }, []);

  // 加载历史发送者列表，供白名单 AutoComplete 使用。

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

  // 组件卸载时关闭 SSE 连接
  useEffect(() => {
    return () => {
      feishuEventSource?.close();
    };
  }, [feishuEventSource]);

  // 关闭绑定弹窗时清理 SSE 连接和 timer
  useEffect(() => {
    if (!bindModalOpen) {
      feishuEventSource?.close();
      setFeishuEventSource(null);
      if (successTimerRef.current) {
        clearTimeout(successTimerRef.current);
        successTimerRef.current = null;
      }
    }
  }, [bindModalOpen, feishuEventSource]);

  const handleStartFeishuBind = async () => {
    // 清理旧的 SSE 连接和 timer，防止重复绑定泄漏
    if (successTimerRef.current) {
      clearTimeout(successTimerRef.current);
      successTimerRef.current = null;
    }
    if (feishuEventSource) {
      feishuEventSource.close();
    }

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

      // 使用 SSE 方式轮询，支持页面关闭后继续执行
      const eventSource = db.feishuPollSSE(
        beginRes.device_code,
        beginRes.interval,
        beginRes.expire_in,
        (pollRes) => {
          if (pollRes.success) {
            setBindSuccess(true);
            message.success(`绑定成功！Bot: ${pollRes.bot_name || 'Feishu Bot'}`);
            loadAgentBots();
            loadFeishuPush();
            // 保存 timer id，关闭模态框
            successTimerRef.current = setTimeout(() => {
              setBindModalOpen(false);
              setQrCodeUrl('');
            }, 2000);
          } else {
            const errMsg = pollRes.error === 'access_denied' ? '用户拒绝了绑定请求'
              : pollRes.error === 'expired_token' ? '二维码已过期，请重新绑定'
              : '绑定超时，请重试';
            setPollError(errMsg);
          }
          setBinding(false);
        },
        (error) => {
          setPollError(error || 'SSE 连接失败');
          setBinding(false);
        }
      );
      setFeishuEventSource(eventSource);
    } catch (err: any) {
      setPollError(err?.message || '启动绑定失败');
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

  const handleViewTodo = (todoId: number) => {
    dispatch({ type: 'SELECT_TODO', payload: todoId });
    onBack?.();
  };

  const handleViewExecutionRecord = async (recordId: number) => {
    try {
      const r = await db.getExecutionRecord(recordId);
      setExecDetailRecord(r);
    } catch (err) {
      message.error('加载执行记录失败: ' + (err instanceof Error ? err.message : '未知错误'));
    }
  };

  const handleChatFilterChange = (v: string | undefined) => {
    setHistorySelectedChatId(v);
    setHistoryPage(1);
  };

  const handleSenderFilterChange = (v: string | undefined) => {
    setHistorySelectedSenderId(v);
    setHistoryPage(1);
  };

  const handleHistoryFilterChange = (v: boolean | undefined) => {
    setHistoryIsHistory(v);
    setHistoryPage(1);
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
              <BindTab
                agentBots={agentBots}
                botsLoading={botsLoading}
                feishuPushStatus={feishuPushStatus}
                whitelistBotId={whitelistBotId}
                groupWhitelist={groupWhitelist}
                whitelistOpenId={whitelistOpenId}
                whitelistName={whitelistName}
                binding={binding}
                bindModalOpen={bindModalOpen}
                qrCodeUrl={qrCodeUrl}
                pollError={pollError}
                bindSuccess={bindSuccess}
                historySenders={historySenders}
                historyViewMsg={historyViewMsg}
                todos={todos}
                configForm={configForm}
                configSaving={configSaving}
                handleSaveConfig={handleSaveConfig}
                setWhitelistOpenId={setWhitelistOpenId}
                setWhitelistName={setWhitelistName}
                setBindModalOpen={setBindModalOpen}
                setQrCodeUrl={setQrCodeUrl}
                setPollError={setPollError}
                setBindSuccess={setBindSuccess}
                setWhitelistBotId={setWhitelistBotId}
                setHistoryViewMsg={setHistoryViewMsg}
                onDeleteBot={handleDeleteBot}
                onAddWhitelist={handleAddWhitelist}
                onDeleteWhitelist={handleDeleteWhitelist}
                onLoadGroupWhitelist={loadGroupWhitelist}
                onLoadHistorySenders={loadHistorySenders}
                onStartBind={handleStartFeishuBind}
                onRefresh={() => { loadAgentBots(); loadFeishuPush(); }}
                onAfterBindModalClose={() => { loadAgentBots(); loadFeishuPush(); }}
                workspaceId={state.selectedWorkspace}
              />
            ),
          },
          {
            key: 'project-binds',
            label: '项目绑定',
            children: <ProjectBindsTab />,
          },
          {
            key: 'record',
            label: '记录',
            children: (
              <RecordTab
                historyMessages={historyMessages}
                historyChats={historyChats}
                historySenders={historySenders}
                historyLoading={historyLoading}
                historyTotal={historyTotal}
                historyPage={historyPage}
                historyPageSize={historyPageSize}
                historySelectedChatId={historySelectedChatId}
                historyIsHistory={historyIsHistory}
                historySelectedSenderId={historySelectedSenderId}
                historyAddModalOpen={historyAddModalOpen}
                historyForm={historyForm}
                agentBots={agentBots}
                onViewMsg={(msg) => setHistoryViewMsg(msg)}
                onViewTodo={handleViewTodo}
                onViewExecutionRecord={handleViewExecutionRecord}
                onRefreshMessages={loadHistoryMessages}
                onChatFilterChange={handleChatFilterChange}
                onSenderFilterChange={handleSenderFilterChange}
                onHistoryFilterChange={handleHistoryFilterChange}
                onPageChange={(p, ps) => { setHistoryPage(p); setHistoryPageSize(ps); }}
                onAddClick={() => setHistoryAddModalOpen(true)}
                onAddChat={handleAddHistoryChat}
                onAddModalCancel={() => { setHistoryAddModalOpen(false); historyForm.resetFields(); }}
              />
            ),
          },
        ]}
      />

      <ExecutionDetailModal
        record={execDetailRecord}
        onClose={() => setExecDetailRecord(null)}
      />
    </div>
  );
}
