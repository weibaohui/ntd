// 日志/对话/命令抽屉组件：展示执行记录的日志、对话和命令三种视图。

import { useState } from 'react';
import { Drawer, Button, Empty } from 'antd';
import { ChatView } from '@/components/ChatView';
import { CommandPanel } from '@/components/CommandPanel';
import { AgentPanel } from '@/components/AgentPanel';
import { RefreshBtn } from '../todo-detail/LogViewHeader';
import { useIsMobile } from '@/hooks/useIsMobile';
import { formatLogTime } from './helpers';
import { LOG_TYPE_COLORS, LOG_TYPE_LABELS } from '@/constants';
import type { ExecutionRecord, LogEntry } from '@/types';

interface LogDrawerProps {
  open: boolean;
  record: ExecutionRecord | null;
  paginatedLogs: LogEntry[];
  logsPage: number;
  isLoadingLogs: boolean;
  onLoadLogs: (recordId: number, page: number) => Promise<void>;
  onClose: () => void;
  runningTasks: Record<string, any>;
}

export function LogDrawer({
  open,
  record,
  paginatedLogs,
  logsPage,
  isLoadingLogs,
  onLoadLogs,
  onClose,
  runningTasks,
}: LogDrawerProps) {
  const [viewMode, setViewMode] = useState<"chat" | "command" | "agent" | "log">("chat");

  const liveLogs = (() => {
    if (!record) return null;
    const allTasks = Object.values(runningTasks);
    for (const t of allTasks) {
      if (t.recordId === record.id) return t.logs || null;
    }
    return null;
  })();

  const displayLogs = liveLogs && liveLogs.length > 0 ? liveLogs : paginatedLogs;

  // 手机端使用从底部滑出的全屏抽屉；桌面端使用右侧 60% 宽度的抽屉
  const isMobile = useIsMobile();

  return (
    <Drawer
      title={`执行详情 #${record?.id || ""}`}
      open={open}
      onClose={onClose}
      placement={isMobile ? "bottom" : "right"}
      height={isMobile ? "85vh" : undefined}
      width={isMobile ? undefined : "60%"}
      styles={{ body: { padding: "12px 16px", display: "flex", flexDirection: "column" } }}
    >
      {/* 刷新 + 视图切换 —— 详情抽屉只保留日志/对话/命令三个视图 */}
      <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 12 }}>
        <Button type={viewMode === "chat" ? "primary" : "default"} size="small" onClick={() => setViewMode("chat")}>
          对话
        </Button>
        <Button type={viewMode === "command" ? "primary" : "default"} size="small" onClick={() => setViewMode("command")}>
          命令
        </Button>
        <Button type={viewMode === "agent" ? "primary" : "default"} size="small" onClick={() => setViewMode("agent")}>
          Agent
        </Button>
        <Button type={viewMode === "log" ? "primary" : "default"} size="small" onClick={() => setViewMode("log")}>
          日志
        </Button>
        <div style={{ flex: 1 }} />
        <RefreshBtn onClick={() => { if (record) onLoadLogs(record.id, logsPage); }} />
      </div>

      <div style={{ overflow: "auto", flex: 1 }}>
        {isLoadingLogs ? (
          <div style={{ textAlign: "center", padding: 40, color: "var(--color-text-tertiary)" }}>
            加载中...
          </div>
        ) : displayLogs.length === 0 ? (
          <Empty description="暂无日志" image={Empty.PRESENTED_IMAGE_SIMPLE} />
        ) : viewMode === "chat" ? (
          <ChatView logs={displayLogs} isRunning={false} />
        ) : viewMode === "command" ? (
          <CommandPanel logs={displayLogs} executor={undefined} />
        ) : viewMode === "agent" ? (
          <AgentPanel logs={displayLogs} />
        ) : (
          <div
            style={{
              background: "var(--log-bg)",
              color: "var(--log-text)",
              padding: 12,
              borderRadius: 8,
              fontFamily: "var(--font-mono)",
              fontSize: 11,
            }}
          >
            {displayLogs.length === 0 ? (
              <div style={{ color: "var(--log-text-muted)" }}>
                {isLoadingLogs ? "加载中..." : "暂无日志"}
              </div>
            ) : (
              displayLogs.map((log: LogEntry, idx: number) => (
                <div key={idx} style={{ marginBottom: 4, display: "flex", gap: 8 }}>
                  <span style={{ color: "var(--log-text-muted)", flexShrink: 0 }}>
                    {formatLogTime(log.timestamp || "")}
                  </span>
                  <span style={{ color: LOG_TYPE_COLORS[log.type || ""] || "var(--log-text)", fontWeight: 500 }}>
                    [{LOG_TYPE_LABELS[log.type || ""] || log.type}]
                  </span>
                  <span>{log.content}</span>
                </div>
              ))
            )}
          </div>
        )}
      </div>
    </Drawer>
  );
}
