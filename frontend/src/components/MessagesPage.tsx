import { useState, useEffect, useCallback } from 'react';
import { Spin, Empty, Space } from 'antd';
import { MessageOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { ExecutionRecordDrawer } from '@/components/settings/messages/ExecutionRecordDrawer';
import * as db from '@/utils/database';
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
  const [searchText, setSearchText] = useState('');

  const [configDrawerOpen, setConfigDrawerOpen] = useState(false);
  const [detailDrawerOpen, setDetailDrawerOpen] = useState(false);
  const [selectedMessage, setSelectedMessage] = useState<FeishuHistoryMessage | null>(null);

  const [execDetailRecord, setExecDetailRecord] = useState<ExecutionRecord | null>(null);

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
        workspace_id: workspaceId,
        bot_id: activeBotId ?? undefined,
        page: messagesPage,
        page_size: messagesPageSize,
      });

      let filtered = data.messages;

      // 搜索过滤：在前端做文本搜索（后端不支持全文搜索）
      if (searchText) {
        const lowerSearch = searchText.toLowerCase();
        filtered = filtered.filter(m => {
          const content = m.content ? JSON.parse(m.content).text || m.content : '';
          return content.toLowerCase().includes(lowerSearch);
        });
      }

      setMessages(filtered);
      setMessagesTotal(data.total);
    } catch {
    } finally {
      setMessagesLoading(false);
    }
  }, [workspaceId, selectedChatId, isHistory, messagesPage, messagesPageSize, activeBotId, searchText]);

  useEffect(() => {
    loadMessages();
  }, [loadMessages]);

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
          searchText={searchText}
          onSearchChange={setSearchText}
          onChatChange={setSelectedChatId}
          onHistoryChange={setIsHistory}
          onPageChange={(p, ps) => { setMessagesPage(p); setMessagesPageSize(ps); }}
          onViewDetail={handleViewDetail}
          onViewExecution={handleViewExecutionRecord}
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
          searchText={searchText}
          onSearchChange={setSearchText}
          onChatChange={setSelectedChatId}
          onHistoryChange={setIsHistory}
          onPageChange={(p, ps) => { setMessagesPage(p); setMessagesPageSize(ps); }}
          onViewDetail={handleViewDetail}
          onViewExecution={handleViewExecutionRecord}
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
    </PageCard>
  );
}
