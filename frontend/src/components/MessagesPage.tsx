import { useState, useEffect, useCallback } from 'react';
import { Spin, Empty, Space, message } from 'antd';
import { MessageOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { ExecutionRecordDrawer } from '@/components/settings/messages/ExecutionRecordDrawer';
import { BlackboardDrawer } from '@/components/loop-studio/executions/BlackboardDrawer';
import * as db from '@/utils/database';
import * as dbLoops from '@/utils/database/loops';
import type { ProjectDirectory, AgentBot } from '@/utils/database';
import type { FeishuHistoryMessage, FeishuHistoryChat, FeishuMessageStats, ExecutionRecord } from '@/types';
import { useIsMobile } from '@/hooks/useIsMobile';
import { MessageHeader } from '@/components/message-monitor/MessageHeader';
import { MessageSidebar } from '@/components/message-monitor/MessageSidebar';
import { MessageTimeline } from '@/components/message-monitor/MessageTimeline';
import { MessageConfigDrawer } from '@/components/message-monitor/MessageConfigDrawer';
import { MessageDetailDrawer } from '@/components/message-monitor/MessageDetailDrawer';

interface MessagesPageProps {
  workspaceId: number | null;
  onManageWorkspace: () => void;
}

export function MessagesPage({ workspaceId, onManageWorkspace }: MessagesPageProps) {
  const isMobile = useIsMobile();

  const [workspace, setWorkspace] = useState<ProjectDirectory | null>(null);
  const [loading, setLoading] = useState(false);

  const [bots, setBots] = useState<AgentBot[]>([]);
  const [activeBotId, setActiveBotId] = useState<number | null>(null);

  const [messages, setMessages] = useState<FeishuHistoryMessage[]>([]);
  const [chats, setChats] = useState<FeishuHistoryChat[]>([]);
  const [stats, setStats] = useState<FeishuMessageStats | null>(null);
  const [messagesLoading, setMessagesLoading] = useState(false);
  const [messagesTotal, setMessagesTotal] = useState(0);
  const [messagesPage, setMessagesPage] = useState(1);
  const [messagesPageSize, setMessagesPageSize] = useState(20);

  const [selectedChatId, setSelectedChatId] = useState<string | undefined>(undefined);
  const [isHistory, setIsHistory] = useState<boolean | undefined>(undefined);
  // 处理状态筛选：undefined=全部、true=已处理、false=未处理，透传给后端按 processed 过滤。
  const [processedFilter, setProcessedFilter] = useState<boolean | undefined>(undefined);
  // 会话类型筛选：undefined=全部、'group'=群聊、'p2p'=私聊，透传给后端按 chat_type 过滤。
  const [chatTypeFilter, setChatTypeFilter] = useState<string | undefined>(undefined);
  // 处理类型筛选：undefined=全部、'slash'=斜杠命令、'executor'=执行器、'loop'=环路，透传后端按 processed_type 模糊匹配。
  const [processedTypeFilter, setProcessedTypeFilter] = useState<string | undefined>(undefined);
  // searchText 是输入框即时值(每按键更新)；debouncedSearch 是防抖后的值，真正触发后端请求。
  // 拆开是为了避免每次按键都打后端 + 全表 LIKE 扫描，停顿 300ms 才下沉。
  const [searchText, setSearchText] = useState('');
  const [debouncedSearch, setDebouncedSearch] = useState('');

  const [configDrawerOpen, setConfigDrawerOpen] = useState(false);
  const [detailDrawerOpen, setDetailDrawerOpen] = useState(false);
  const [selectedMessage, setSelectedMessage] = useState<FeishuHistoryMessage | null>(null);

  const [execDetailRecord, setExecDetailRecord] = useState<ExecutionRecord | null>(null);

  // 环路执行详情状态
  const [blackboardOpen, setBlackboardOpen] = useState(false);
  const [blackboardExecs, setBlackboardExecs] = useState<Record<string, any>[]>([]);

  // 加载工作空间信息和 Bot 列表
  useEffect(() => {
    if (workspaceId == null) {
      setWorkspace(null);
      setBots([]);
      return;
    }
    setLoading(true);
    Promise.all([
      db.getProjectDirectories(),
      db.getAgentBots(),
    ])
      .then(([dirs, allBots]) => {
        const matched = dirs.find((d) => d.id === workspaceId) ?? null;
        setWorkspace(matched);
        // 筛选当前工作空间的 Bot
        setBots(allBots.filter(b => b.workspace_id === workspaceId));
      })
      .catch(() => {
        setWorkspace(null);
        setBots([]);
      })
      .finally(() => setLoading(false));
  }, [workspaceId]);

  // 加载 chats：按 workspace 下的 bot_id 过滤
  useEffect(() => {
    if (!workspaceId || bots.length === 0) {
      setChats([]);
      return;
    }
    // 获取当前 workspace 下所有 bot 的 chats
    Promise.all(bots.map(bot => db.getFeishuHistoryChats(bot.id)))
      .then(results => {
        // 合并所有 bot 的 chats
        const allChats = results.flat();
        setChats(allChats);
      })
      .catch(() => setChats([]));
  }, [workspaceId, bots]);

  const loadMessages = useCallback(async () => {
    if (!workspaceId) return;

    setMessagesLoading(true);
    try {
      // 直接通过后端 workspace_id 和 bot_id 参数筛选，无需前端二次过滤
      const data = await db.getFeishuHistoryMessages({
        chat_id: selectedChatId,
        is_history: isHistory,
        processed: processedFilter,
        chat_type: chatTypeFilter,
        // 关键字搜索下沉到后端：原先前端只过滤当前页导致 total 与结果不一致，改后端 LIKE 后 total 准确。
        // 用防抖后的 debouncedSearch，避免每次按键打后端。
        keyword: debouncedSearch || undefined,
        processed_type: processedTypeFilter,
        workspace_id: workspaceId,
        bot_id: activeBotId ?? undefined,
        page: messagesPage,
        page_size: messagesPageSize,
      });

      // 关键字搜索已在后端完成(见 keyword 参数)，前端不再二次过滤。
      setMessages(data.messages);
      setMessagesTotal(data.total);
    } catch {
    } finally {
      setMessagesLoading(false);
    }
  }, [workspaceId, selectedChatId, isHistory, processedFilter, chatTypeFilter, processedTypeFilter, messagesPage, messagesPageSize, activeBotId, debouncedSearch]);

  useEffect(() => {
    loadMessages();
  }, [loadMessages]);

  // 搜索防抖 + 回第 1 页：输入停顿 300ms 后才把关键字下沉到 debouncedSearch。
  // 同一 tick 内一并重置页码，确保 loadMessages 只发一次「新搜索 + page=1」的请求，
  // 而非先发一次「新搜索 + 旧页码」的中间请求(旧页码可能超出筛选后的总页数 → 空列表)。
  useEffect(() => {
    const handle = setTimeout(() => {
      setDebouncedSearch(searchText);
      setMessagesPage(1);
    }, 300);
    return () => clearTimeout(handle);
  }, [searchText]);

  useEffect(() => {
    if (!workspaceId) {
      setStats(null);
      return;
    }
    // 统计也需要按 workspace 隔离
    db.getFeishuMessageStats(workspaceId).then(setStats).catch(() => setStats(null));
  }, [workspaceId]);

  const handleRefresh = () => {
    loadMessages();
    if (workspaceId) {
      db.getFeishuMessageStats(workspaceId).then(setStats).catch(() => {});
    }
  };

  const handleViewExecutionRecord = async (recordId: number) => {
    try {
      const r = await db.getExecutionRecord(recordId);
      setExecDetailRecord(r);
    } catch {
    }
  };

  // 处理环路执行详情点击
  const handleViewLoopExecution = async (msg: FeishuHistoryMessage) => {
    if (!msg.processed_id) return;
    try {
      const detail = await dbLoops.getExecutionById(msg.processed_id);
      setBlackboardExecs(detail.step_executions || []);
      setBlackboardOpen(true);
    } catch {
      message.error('加载环路执行详情失败');
    }
  };

  const handleViewDetail = (message: FeishuHistoryMessage) => {
    setSelectedMessage(message);
    setDetailDrawerOpen(true);
  };

  if (loading) {
    return (
      <PageCard icon={<MessageOutlined />} title="消息">
        <div style={{ display: 'flex', justifyContent: 'center', padding: 48 }}>
          <Spin />
        </div>
      </PageCard>
    );
  }

  if (workspace == null) {
    return (
      <PageCard icon={<MessageOutlined />} title="消息">
        <Empty
          description="请先在左上角选择一个工作空间，或前往工作空间管理新建"
          style={{ padding: 48 }}
        >
          <a onClick={onManageWorkspace} style={{ cursor: 'pointer' }}>
            前往工作空间管理
          </a>
        </Empty>
      </PageCard>
    );
  }

  if (isMobile) {
    return (
      <PageCard icon={<MessageOutlined />} title="消息监控台">
        <MessageHeader
          workspaceName={workspace.name || ''}
          stats={stats}
          loading={messagesLoading}
          onRefresh={handleRefresh}
          onOpenConfig={() => setConfigDrawerOpen(true)}
        />

        <div style={{ marginBottom: 12 }}>
          <Space wrap>
            <span style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>筛选 Bot：</span>
            <button
              onClick={() => setActiveBotId(null)}
              style={{
                padding: '4px 12px',
                borderRadius: 4,
                border: '1px solid var(--color-border-secondary)',
                backgroundColor: activeBotId === null ? 'var(--color-primary)' : 'transparent',
                color: activeBotId === null ? 'white' : 'var(--color-text-primary)',
                fontSize: 12,
                cursor: 'pointer',
              }}
            >
              全部
            </button>
            {bots.map(bot => (
              <button
                key={bot.id}
                onClick={() => setActiveBotId(bot.id)}
                style={{
                  padding: '4px 12px',
                  borderRadius: 4,
                  border: '1px solid var(--color-border-secondary)',
                  backgroundColor: activeBotId === bot.id ? 'var(--color-primary)' : 'transparent',
                  color: activeBotId === bot.id ? 'white' : 'var(--color-text-primary)',
                  fontSize: 12,
                  cursor: 'pointer',
                }}
              >
                {bot.bot_name}
              </button>
            ))}
          </Space>
        </div>

        <MessageTimeline
          messages={messages}
          chats={chats}
          bots={bots}
          loading={messagesLoading}
          total={messagesTotal}
          page={messagesPage}
          pageSize={messagesPageSize}
          selectedChatId={selectedChatId}
          isHistory={isHistory}
          processedFilter={processedFilter}
          chatTypeFilter={chatTypeFilter}
          processedTypeFilter={processedTypeFilter}
          searchText={searchText}
          onSearchChange={setSearchText}
          onChatChange={setSelectedChatId}
          onHistoryChange={setIsHistory}
          onProcessedChange={setProcessedFilter}
          onChatTypeChange={setChatTypeFilter}
          onProcessedTypeChange={setProcessedTypeFilter}
          onPageChange={(p, ps) => { setMessagesPage(p); setMessagesPageSize(ps); }}
          onViewDetail={handleViewDetail}
          onViewExecution={handleViewExecutionRecord}
          onViewLoopExecution={handleViewLoopExecution}
        />

        <MessageConfigDrawer
          open={configDrawerOpen}
          workspaceId={workspace.id}
          onClose={() => setConfigDrawerOpen(false)}
          onChanged={handleRefresh}
        />

        <MessageDetailDrawer
          open={detailDrawerOpen}
          message={selectedMessage}
          onClose={() => setDetailDrawerOpen(false)}
        />

        <ExecutionRecordDrawer record={execDetailRecord} onClose={() => setExecDetailRecord(null)} />
      </PageCard>
    );
  }

  return (
    <PageCard icon={<MessageOutlined />} title="消息监控台">
      <MessageHeader
        workspaceName={workspace.name || ''}
        stats={stats}
        loading={messagesLoading}
        onRefresh={handleRefresh}
        onOpenConfig={() => setConfigDrawerOpen(true)}
      />

      <div style={{ display: 'flex', gap: 16, height: 'calc(100vh - 200px)' }}>
        <MessageSidebar
          bots={bots}
          activeBotId={activeBotId}
          onSelectBot={setActiveBotId}
        />

        <MessageTimeline
          messages={messages}
          chats={chats}
          bots={bots}
          loading={messagesLoading}
          total={messagesTotal}
          page={messagesPage}
          pageSize={messagesPageSize}
          selectedChatId={selectedChatId}
          isHistory={isHistory}
          processedFilter={processedFilter}
          chatTypeFilter={chatTypeFilter}
          processedTypeFilter={processedTypeFilter}
          searchText={searchText}
          onSearchChange={setSearchText}
          onChatChange={setSelectedChatId}
          onHistoryChange={setIsHistory}
          onProcessedChange={setProcessedFilter}
          onChatTypeChange={setChatTypeFilter}
          onProcessedTypeChange={setProcessedTypeFilter}
          onPageChange={(p, ps) => { setMessagesPage(p); setMessagesPageSize(ps); }}
          onViewDetail={handleViewDetail}
          onViewExecution={handleViewExecutionRecord}
          onViewLoopExecution={handleViewLoopExecution}
        />
      </div>

      <MessageConfigDrawer
        open={configDrawerOpen}
        workspaceId={workspace.id}
        onClose={() => setConfigDrawerOpen(false)}
        onChanged={handleRefresh}
      />

      <MessageDetailDrawer
        open={detailDrawerOpen}
        message={selectedMessage}
        onClose={() => setDetailDrawerOpen(false)}
      />

      <ExecutionRecordDrawer record={execDetailRecord} onClose={() => setExecDetailRecord(null)} />

      <BlackboardDrawer
        open={blackboardOpen}
        stepExecs={blackboardExecs}
        onClose={() => setBlackboardOpen(false)}
      />
    </PageCard>
  );
}
