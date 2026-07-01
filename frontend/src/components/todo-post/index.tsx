// 帖子详情页组件：按 session_id 加载同 session 的所有记录。

// 重新导出子组件（供外部使用）
export { LogDrawer } from './LogDrawer';
export { CollapsibleCommand } from './CollapsibleCommand';
export { RatingControl } from './RatingControl';
export { WorktreePathDisplay } from './WorktreePathDisplay';
export { ReplyRow } from './ReplyRow';
export { PostCard } from './PostCard';
export { ThreadGroup } from './ThreadGroup';
export { getElapsedSeconds, groupBySession, formatLogTime } from './helpers';
export type { SessionGroup } from './helpers';

import { useState, useRef, useEffect, useMemo } from "react";
import {
  Button,
  Empty,
  App,
} from "antd";
import {
  ArrowLeftOutlined,
  InfoCircleOutlined,
} from "@ant-design/icons";
import { useApp } from "@/hooks/useApp";
import { useIsMobile } from "@/hooks/useIsMobile";
import { PageCard } from "@/components/common/PageCard";
import { LogDrawer } from './LogDrawer';
import { groupBySession } from "./helpers";
import { parseLogsToMessages } from "@/components/ChatView";
import { conversationToYaml } from "@/utils/markdown";
import { getExecutorOption } from "@/types";
import { EXPORT } from "@/constants";
import * as db from "@/utils/database";
import type { ExecutionRecord, LogEntry } from "@/types";
import { ThreadGroup } from "./ThreadGroup";

/**
 * 全屏帖子详情页 —— 按 session_id 加载同 session 的所有记录。
 * 进入页面后获取目标记录 → 取 session_id → 调后端接口拉取同 session 记录。
 */
export function TodoPostPage({
  todoId,
  recordId,
  onBack,
}: {
  todoId: number;
  recordId: number;
  onBack: () => void;
}) {
  const { state } = useApp();
  const isMobile = useIsMobile();
  const { message } = App.useApp();
  const { runningTasks } = state;

  const [records, setRecords] = useState<ExecutionRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [replyLoading, setReplyLoading] = useState(false);

  // 日志抽屉状态
  const [logDrawerOpen, setLogDrawerOpen] = useState(false);
  const [logDrawerRecordId, setLogDrawerRecordId] = useState<number | null>(null);
  const [paginatedLogs, setPaginatedLogs] = useState<LogEntry[]>([]);
  const [logsPage, setLogsPage] = useState(1);
  const logsPerPage = 200;
  const [isLoadingLogs, setIsLoadingLogs] = useState(false);

  // 加载 session 记录
  const loadSessionRecords = async () => {
    setLoading(true);
    try {
      // 1. 获取目标记录
      const targetRecord = await db.getExecutionRecord(recordId);
      // 2. 如果有 session_id，拉取同 session 的所有记录
      if (targetRecord.session_id) {
        const sessionRecords = await db.getExecutionRecordsBySession(targetRecord.session_id);
        // 按 started_at 排序
        sessionRecords.sort((a, b) =>
          (a.started_at || "").localeCompare(b.started_at || "")
        );
        setRecords(sessionRecords);
      } else {
        // 无 session_id，只显示这一条
        setRecords([targetRecord]);
      }
    } catch (error) {
      message.error("加载执行记录失败: " + (error instanceof Error ? error.message : String(error)));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadSessionRecords();
  }, [recordId]);

  // 加载日志
  const loadLogsForRecord = async (rId: number, page: number) => {
    setIsLoadingLogs(true);
    try {
      const result = await db.getExecutionLogs(rId, page, logsPerPage);
      setPaginatedLogs(result.logs);
      setLogsPage(page);
    } catch {
      // ignore
    } finally {
      setIsLoadingLogs(false);
    }
  };

  const isExecuting = Object.values(runningTasks).some(
    (t) => t.todoId === todoId && t.status === "running"
  );

  // 定时器 —— 实时计时
  const [, setTick] = useState(0);
  useEffect(() => {
    if (!isExecuting) return;
    const interval = setInterval(() => setTick((t) => t + 1), 1000);
    return () => clearInterval(interval);
  }, [isExecuting]);

  // 执行结束时刷新
  const prevIsExecutingRef = useRef(isExecuting);
  useEffect(() => {
    const prev = prevIsExecutingRef.current;
    if (prev && !isExecuting) {
      loadSessionRecords();
    }
    prevIsExecutingRef.current = isExecuting;
  }, [isExecuting]);

  const getRunningTaskForRecord = (r: ExecutionRecord) => {
    if (r.task_id) return runningTasks[r.task_id] || null;
    return Object.values(runningTasks).find((t) => t.todoId === r.todo_id) || null;
  };

  const resolveExecutionStats = (r: ExecutionRecord, running: boolean) => {
    if (running) {
      const task = getRunningTaskForRecord(r);
      if (task?.executionStats) return task.executionStats;
    }
    return r.execution_stats;
  };

  const sessionGroups = useMemo(() => groupBySession(records), [records]);

  const handleStopExecution = async (rId: number) => {
    try {
      await db.stopExecution(rId);
      message.info("已发送停止指令");
      await loadSessionRecords();
    } catch (error) {
      message.error("停止失败: " + (error instanceof Error ? error.message : String(error)));
    }
  };

  const handleReply = async (r: ExecutionRecord, replyMessage: string) => {
    if (!replyMessage.trim()) return;
    setReplyLoading(true);
    try {
      await db.resumeExecutionRecord(r.id, replyMessage);
      message.success("回复成功，开始执行");
      await loadSessionRecords();
    } catch (error) {
      message.error("回复失败: " + (error instanceof Error ? error.message : String(error)));
    } finally {
      setReplyLoading(false);
    }
  };

  const handleRateExecution = async (rId: number, rating: number | null) => {
    try {
      await db.rateExecutionRecord(rId, rating);
      // 刷新当前记录
      const updated = await db.getExecutionRecord(rId);
      setRecords(prev => prev.map(r => (r.id === rId ? updated : r)));
      message.success(rating == null ? "已清除评分" : `已评分 ${rating}`);
    } catch (error) {
      message.error("评分失败: " + (error instanceof Error ? error.message : String(error)));
    }
  };

  const handleExportMarkdown = async (r: ExecutionRecord) => {
    let logs: LogEntry[] = [];
    try {
      const result = await db.getExecutionLogs(r.id, 1, EXPORT.maxLogs);
      logs = result.logs;
    } catch { /* ignore */ }
    const msgs = parseLogsToMessages(logs);
    const executorLabel = r.executor ? getExecutorOption(r.executor).label : undefined;
    const content = conversationToYaml(msgs, {
      title: todo?.title,
      executor: executorLabel,
      model: r.model || undefined,
      startedAt: r.started_at,
      status: r.status,
    });
    const blob = new Blob([content], { type: "application/x-yaml;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    const ts = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
    a.download = `exec-${r.id}-${ts}.yaml`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
    message.success("导出成功");
  };

  const handleSelectRecord = (rId: number) => {
    const url = new URL(window.location.href);
    url.searchParams.set("record", String(rId));
    window.history.replaceState(null, "", url.toString());
  };

  const openLogDrawer = (rId: number) => {
    setLogDrawerRecordId(rId);
    loadLogsForRecord(rId, 1);
    setLogDrawerOpen(true);
  };

  const todo = state.todos.find((t) => t.id === todoId);
  const todoTitle = todo?.title || `事项 #${todoId}`;

  // 全局楼层号
  let floorCounter = 0;
  const getNextFloor = () => {
    floorCounter++;
    return floorCounter;
  };

  return (
    <PageCard
      icon={<InfoCircleOutlined />}
      title={todoTitle}
      extra={
        <Button type="text" icon={<ArrowLeftOutlined />} onClick={onBack}>
          返回
        </Button>
      }
      contentStyle={{ padding: isMobile ? 8 : "16px 24px 40px", overflow: "auto" }}
      style={{ flex: 1, width: "100%", minHeight: 0 }}
      contentClassName="todo-post-content"
    >
      {loading ? (
        <div style={{ textAlign: "center", padding: 40, color: "var(--color-text-tertiary)" }}>
          加载中...
        </div>
      ) : records.length === 0 ? (
        <Empty description="暂无执行记录" image={Empty.PRESENTED_IMAGE_SIMPLE} />
      ) : (
        sessionGroups.map((group) => (
          <ThreadGroup
            key={group.sessionId}
            group={group}
            getNextFloor={getNextFloor}
            onSelectRecord={handleSelectRecord}
            onStop={handleStopExecution}
            onReply={handleReply}
            replyLoading={replyLoading}
            onOpenLogDrawer={openLogDrawer}
            resolveExecutionStats={resolveExecutionStats}
            todoTitle={todoTitle}
            onRate={handleRateExecution}
            onExport={handleExportMarkdown}
          />
        ))
      )}

      {/* 日志/对话抽屉 */}
      <LogDrawer
        open={logDrawerOpen}
        record={records.find(r => r.id === logDrawerRecordId) || null}
        paginatedLogs={paginatedLogs}
        logsPage={logsPage}
        isLoadingLogs={isLoadingLogs}
        onLoadLogs={loadLogsForRecord}
        onClose={() => setLogDrawerOpen(false)}
        runningTasks={runningTasks}
      />
    </PageCard>
  );
}
