import { useState, useRef, useEffect, useMemo } from "react";
import {
  Button,
  Empty,
  App,
  Tag,
  Drawer,
  Tooltip,
} from "antd";
import {
  ArrowLeftOutlined,
  StopOutlined,
  LinkOutlined,
  InfoCircleOutlined,
} from "@ant-design/icons";
import { useApp } from "@/hooks/useApp";
import { useIsMobile } from "@/hooks/useIsMobile";
import { PageCard } from "@/components/common/PageCard";
import { ExecutorBadge } from "@/components/ExecutorBadge";
import { ChatView } from "@/components/ChatView";
import { CommandPanel } from "@/components/CommandPanel";
import { CollapsibleConclusion } from "./todo-detail/CollapsibleConclusion";
import { ReplyInput } from "./todo-detail/ReplyInput";
import { RefreshBtn } from "./todo-detail/LogViewHeader";
import { LOG_TYPE_COLORS, LOG_TYPE_LABELS } from "@/constants";
import { groupBySession, formatLogTime, getElapsedSeconds } from "./todo-detail/helpers";
import { formatLocalDateTime, formatDurationSec } from "@/utils/datetime";
import { supportsResume } from "@/types";
import { parseLogsToMessages } from "./ChatView";
import { conversationToYaml } from "@/utils/markdown";
import { getExecutorOption } from "@/types";
import { copyToClipboard } from "@/utils/clipboard";
import { EXPORT } from "@/constants";
import * as db from "@/utils/database";
import type { ExecutionRecord, LogEntry } from "@/types";
import type { SessionGroup } from "./todo-detail/helpers";
import {
  Popover,
  InputNumber,
  Space,
  message as antdMessage,
} from "antd";
import { StarOutlined, StarFilled, FileTextOutlined, CaretDownOutlined, CaretUpOutlined, CopyOutlined, BranchesOutlined } from "@ant-design/icons";

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
        onRate={handleRateExecution}
        onExport={handleExportMarkdown}
      />
    </PageCard>
  );
}

// ─── 帖子组（所有记录平铺为连续楼层） ─────────────────────────

function ThreadGroup({
  group,
  getNextFloor,
  onSelectRecord,
  onStop,
  onReply,
  replyLoading,
  onOpenLogDrawer,
  resolveExecutionStats,
  todoTitle,
}: {
  group: SessionGroup;
  getNextFloor: () => number;
  onSelectRecord: (id: number) => void;
  onStop: (id: number) => Promise<void>;
  onReply: (r: ExecutionRecord, msg: string) => Promise<void>;
  replyLoading: boolean;
  onOpenLogDrawer: (id: number) => void;
  resolveExecutionStats: (r: ExecutionRecord, running: boolean) => any;
  todoTitle: string;
}) {
  const allRecords = group.records;
  const lastRecord = allRecords[allRecords.length - 1];

  return (
    <div style={{ marginBottom: 24 }}>
      {allRecords.map((record, idx) => (
        <PostCard
          key={record.id}
          record={record}
          floor={getNextFloor()}
          isContinuation={idx > 0}
          onSelect={() => onSelectRecord(record.id)}
          onStop={onStop}
          onOpenLogDrawer={onOpenLogDrawer}
          resolveExecutionStats={resolveExecutionStats}
          todoTitle={todoTitle}
        />
      ))}
      <ReplyRow record={lastRecord} onReply={onReply} loading={replyLoading} />
    </div>
  );
}

// ─── 主帖卡片 ──────────────────────────────────────────────

function PostCard({
  record,
  floor,
  isContinuation = false,
  onSelect,
  onStop,
  onOpenLogDrawer,
  resolveExecutionStats,
  todoTitle,
}: {
  record: ExecutionRecord;
  floor: number;
  isContinuation?: boolean;
  onSelect: () => void;
  onStop: (id: number) => Promise<void>;
  onOpenLogDrawer: (id: number) => void;
  resolveExecutionStats: (r: ExecutionRecord, running: boolean) => any;
  todoTitle?: string;
}) {
  const isRunning = record.status === "running";
  const [elapsedSec, setElapsedSec] = useState(
    isRunning ? getElapsedSeconds(record.started_at) : 0
  );

  useEffect(() => {
    if (!isRunning) return;
    const tick = () => setElapsedSec(getElapsedSeconds(record.started_at));
    tick();
    const timer = setInterval(tick, 1000);
    return () => clearInterval(timer);
  }, [isRunning, record.started_at]);

  const stats = resolveExecutionStats(record, isRunning);

  return (
    <div
      onClick={onSelect}
      style={{
        background: "var(--color-bg-elevated)",
        border: "1px solid var(--color-border-light)",
        borderRadius: 8,
        padding: "16px 20px",
        cursor: "pointer",
        marginBottom: 2,
      }}
    >
      {/* 帖子头：楼号、标题、状态和操作按钮 */}
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: 10,
          paddingBottom: 8,
          borderBottom: "1px dashed var(--color-border-light)",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 10, flex: 1, minWidth: 0 }}>
          <span style={{ fontSize: 14, fontWeight: 700, color: "var(--color-primary)", flexShrink: 0 }}>
            #{floor}
          </span>
          {isContinuation && <LinkOutlined style={{ fontSize: 12, color: "var(--color-primary)", flexShrink: 0 }} />}
          <span style={{
            fontSize: 13,
            fontWeight: 500,
            color: "var(--color-text)",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}>
            {isContinuation
              ? (record.resume_message
                  ? String(record.resume_message)
                  : "继续对话")
              : todoTitle || "初始执行"}
          </span>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span
            style={{
              fontSize: 11,
              padding: "2px 10px",
              borderRadius: 12,
              backgroundColor:
                record.status === "success"
                  ? "var(--color-success)"
                  : record.status === "failed"
                  ? "var(--color-error)"
                  : "var(--color-info)",
              color: "#fff",
              fontWeight: 600,
            }}
          >
            {record.status === "success" ? "成功" : record.status === "failed" ? "失败" : "进行中"}
          </span>
          {isRunning && (
            <Button
              type="text" size="small" danger icon={<StopOutlined />}
              onClick={(e) => { e.stopPropagation(); onStop(record.id); }}
            >
              停止
            </Button>
          )}
          <Button
            type="text" size="small" icon={<InfoCircleOutlined />}
            onClick={(e) => { e.stopPropagation(); onOpenLogDrawer(record.id); }}
          >
            详情
          </Button>
        </div>
      </div>

      {/* 帖子内容 —— 结论 */}
      {record.result ? (
        <CollapsibleConclusion result={record.result} status={record.status} recordId={record.id} />
      ) : isRunning ? (
        <div style={{ color: "var(--color-text-tertiary)", fontSize: 13, padding: "8px 0" }}>
          执行中...
        </div>
      ) : (
        <div style={{ color: "var(--color-text-tertiary)", fontSize: 13, padding: "8px 0" }}>
          暂无结论
        </div>
      )}

      {/* worktree 路径：仅当 record.worktree_path 非空时渲染 */}
      <WorktreePathDisplay worktreePath={record.worktree_path ?? null} />

      {/* 元信息：执行器、时间、触发类型、耗时 */}
      <div style={{
        display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap",
        marginTop: 8, fontSize: 12,
      }}>
        {record.executor && <ExecutorBadge executor={record.executor} />}
        {record.model && (
          <Tag color="#3b82f6" style={{ margin: 0, fontSize: 11 }}>
            {record.model}
          </Tag>
        )}
        <span style={{ color: "var(--color-text-tertiary)" }}>
          {formatLocalDateTime(record.started_at)}
        </span>
        <Tag
          color={
            record.trigger_type === "cron"
              ? "#8b5cf6"
              : record.trigger_type?.startsWith("hook:")
              ? "#a855f7"
              : "#6b7280"
          }
          style={{ margin: 0, fontSize: 11 }}
        >
          {record.trigger_type === "cron"
            ? "Cron"
            : record.trigger_type?.startsWith("hook:")
            ? "Hook"
            : "手动"}
        </Tag>
        {!isRunning && record.usage?.duration_ms && (
          <span style={{ color: "var(--color-success)", fontWeight: 600 }}>
            {formatDurationSec(record.usage.duration_ms / 1000)}
          </span>
        )}
        {isRunning && elapsedSec > 0 && (
          <span style={{ color: "var(--color-info)", fontWeight: 600 }}>
            {formatDurationSec(elapsedSec)}
          </span>
        )}
      </div>

      {/* 统计 */}
      <div style={{
        display: "flex", gap: 16, marginTop: 8, fontSize: 11,
        color: "var(--color-text-tertiary)", flexWrap: "wrap",
      }}>
        {record.usage && (
          <>
            <span>Input: <b>{record.usage.input_tokens.toLocaleString()}</b></span>
            <span>Output: <b>{record.usage.output_tokens.toLocaleString()}</b></span>
            {record.usage.cache_read_input_tokens != null && record.usage.cache_read_input_tokens > 0 && (
              <span>缓存读: <b>{record.usage.cache_read_input_tokens.toLocaleString()}</b></span>
            )}
            {record.usage.cache_creation_input_tokens != null && record.usage.cache_creation_input_tokens > 0 && (
              <span>缓存写: <b>{record.usage.cache_creation_input_tokens.toLocaleString()}</b></span>
            )}
            {record.usage.total_cost_usd != null && (
              <span style={{ color: "var(--color-warning)" }}>
                ${record.usage.total_cost_usd.toFixed(6)}
              </span>
            )}
          </>
        )}
        {stats && (
          <>
            <span>工具调用: <b style={{ color: "var(--color-primary)" }}>{stats.tool_calls}</b></span>
            <span>对话轮次: <b style={{ color: "var(--color-primary)" }}>{stats.conversation_turns}</b></span>
          </>
        )}
      </div>
    </div>
  );
}

// ─── 回复输入行 ────────────────────────────────────────────

function ReplyRow({
  record,
  onReply,
  loading,
}: {
  record: ExecutionRecord;
  onReply: (r: ExecutionRecord, msg: string) => Promise<void>;
  loading: boolean;
}) {
  if (record.status === "running" || !supportsResume(record)) return null;
  return (
    <div style={{ padding: "4px 0" }}>
      <ReplyInput record={record} onReply={onReply} loading={loading} />
    </div>
  );
}

// ─── 日志/对话/命令抽屉 ────────────────────────────────────

function LogDrawer({
  open,
  record,
  paginatedLogs,
  logsPage,
  isLoadingLogs,
  onLoadLogs,
  onClose,
  runningTasks,
  onRate,
  onExport,
}: {
  open: boolean;
  record: ExecutionRecord | null;
  paginatedLogs: LogEntry[];
  logsPage: number;
  isLoadingLogs: boolean;
  onLoadLogs: (recordId: number, page: number) => Promise<void>;
  onClose: () => void;
  runningTasks: Record<string, any>;
  onRate: (recordId: number, rating: number | null) => Promise<void>;
  onExport: (record: ExecutionRecord) => void;
}) {
  const [viewMode, setViewMode] = useState<"chat" | "command" | "log">("chat");

  const liveLogs = (() => {
    if (!record) return null;
    const allTasks = Object.values(runningTasks);
    for (const t of allTasks) {
      if (t.recordId === record.id) return t.logs || null;
    }
    return null;
  })();

  const displayLogs = liveLogs && liveLogs.length > 0 ? liveLogs : paginatedLogs;

  return (
    <Drawer
      title={`执行详情 #${record?.id || ""}`}
      open={open}
      onClose={onClose}
      width="60%"
      styles={{ body: { padding: "12px 16px", display: "flex", flexDirection: "column" } }}
    >
      {/* 执行命令 —— 可折叠，默认收缩 */}
      {record?.command && <CollapsibleCommand command={record.command} />}

      {/* 元信息行：评分 + 导出 */}
      <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 12 }}>
        {record && record.status !== "running" && (
          <RatingControl record={record} onRate={onRate} />
        )}
        {record && record.status !== "running" && !!record.finished_at && (
          <Button type="text" size="small" icon={<FileTextOutlined />} onClick={() => onExport(record)}>
            导出YAML
          </Button>
        )}
        <div style={{ flex: 1 }} />
        <RefreshBtn onClick={() => { if (record) onLoadLogs(record.id, logsPage); }} />
      </div>

      {/* 视图切换 */}
      <div style={{ display: "flex", gap: 8, marginBottom: 12 }}>
        <Button type={viewMode === "chat" ? "primary" : "default"} size="small" onClick={() => setViewMode("chat")}>
          对话
        </Button>
        <Button type={viewMode === "command" ? "primary" : "default"} size="small" onClick={() => setViewMode("command")}>
          命令
        </Button>
        <Button type={viewMode === "log" ? "primary" : "default"} size="small" onClick={() => setViewMode("log")}>
          日志
        </Button>
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

// ─── 可折叠命令 ────────────────────────────────────────────

/**
 * 可折叠的命令展示，默认收缩。
 * 折叠态显示命令前 60 字 + 复制按钮；
 * 展开态显示完整命令文本。
 */
function CollapsibleCommand({ command }: { command: string }) {
  const [expanded, setExpanded] = useState(false);

  const handleCopy = async () => {
    try {
      const ok = await copyToClipboard(command);
      antdMessage[ok ? "success" : "error"](ok ? "已复制" : "复制失败");
    } catch {
      antdMessage.error("复制失败");
    }
  };

  const truncated = command.length > 60 ? command.substring(0, 60) + "..." : command;

  return (
    <div
      style={{
        marginBottom: 12,
        padding: "8px 12px",
        background: "var(--log-bg)",
        borderRadius: 6,
        border: "1px solid var(--color-border-light)",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
        <Button
          type="text"
          size="small"
          icon={expanded ? <CaretUpOutlined /> : <CaretDownOutlined />}
          onClick={() => setExpanded(!expanded)}
          style={{ flexShrink: 0, padding: "0 4px" }}
        />
        <span
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: 11,
            fontFamily: "var(--font-mono)",
            color: "var(--log-text)",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: expanded ? "pre-wrap" : "nowrap",
            wordBreak: "break-all",
            cursor: "pointer",
          }}
          onClick={() => setExpanded(!expanded)}
        >
          {expanded ? command : truncated}
        </span>
        <Button
          type="text"
          size="small"
          icon={<CopyOutlined />}
          onClick={handleCopy}
          style={{ flexShrink: 0 }}
        />
      </div>
    </div>
  );
}

// ─── 评分控件 ──────────────────────────────────────────────

function RatingControl({
  record,
  onRate,
}: {
  record: ExecutionRecord;
  onRate: (recordId: number, rating: number | null) => Promise<void>;
}) {
  const [open, setOpen] = useState(false);
  const [value, setValue] = useState<number | null>(record.rating ?? null);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    setValue(record.rating ?? null);
  }, [record.rating, record.id]);

  const handleSubmit = async (next: number | null) => {
    setSubmitting(true);
    try {
      await onRate(record.id, next);
      setOpen(false);
    } finally {
      setSubmitting(false);
    }
  };

  if (record.rating != null) {
    return (
      <Popover
        open={open}
        onOpenChange={setOpen}
        trigger="click"
        content={
          <Space.Compact style={{ width: 200 }}>
            <InputNumber
              min={0} max={100} value={value}
              onChange={v => setValue(typeof v === "number" ? v : null)}
              placeholder="0-100" style={{ width: "100%" }}
              onPressEnter={() => { if (value != null) handleSubmit(value); }}
            />
            <Button type="primary" loading={submitting} onClick={() => { if (value != null) handleSubmit(value); }}>
              更新
            </Button>
          </Space.Compact>
        }
      >
        <Button type="text" size="small" icon={<StarFilled style={{ color: "#faad14" }} />}>
          {record.rating}
        </Button>
      </Popover>
    );
  }

  return (
    <Popover
      open={open}
      onOpenChange={setOpen}
      trigger="click"
      content={
        <Space.Compact style={{ width: 200 }}>
          <InputNumber
            min={0} max={100} value={value}
            onChange={v => setValue(typeof v === "number" ? v : null)}
            placeholder="0-100" style={{ width: "100%" }}
            onPressEnter={() => { if (value != null) handleSubmit(value); }}
          />
          <Button type="primary" loading={submitting} onClick={() => { if (value != null) handleSubmit(value); }}>
            评分
          </Button>
        </Space.Compact>
      }
    >
      <Button type="text" size="small" icon={<StarOutlined />}>评分</Button>
    </Popover>
  );
}

/**
 * 与 RecordDetailView 中的 WorktreePathDisplay 保持一致：
 * - 未启用 worktree 时（`worktree_path` 为 null 或空串）整段不渲染。
 * - 路径过长时只展示尾部，tooltip 显示完整路径。
 * - 点击整行复制完整路径，HTTP 环境自动 fallback 到 execCommand。
 *
 * 与 desktop 详情页的展示语义完全一致，便于用户在任何入口都能定位 worktree 目录。
 */
function WorktreePathDisplay({ worktreePath }: { worktreePath: string | null }) {
  if (!worktreePath) return null;

  // 路径过长时只展示尾部，鼠标悬停 tooltip 显示完整路径
  const displayPath = worktreePath.length > 60
    ? `…${worktreePath.slice(-59)}`
    : worktreePath;

  return (
    <Tooltip title={worktreePath}>
      <div
        onClick={async () => {
          // 复用统一复制工具：HTTPS 走 navigator.clipboard，HTTP 环境 fallback 到 execCommand
          const ok = await copyToClipboard(worktreePath);
          antdMessage[ok ? 'success' : 'error'](ok ? '已复制 worktree 路径' : '复制失败');
        }}
        style={{
          fontSize: 11,
          color: 'var(--color-text-quaternary)',
          marginBottom: 12,
          fontFamily: 'var(--font-mono)',
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          whiteSpace: 'nowrap',
          cursor: 'pointer',
          display: 'flex',
          alignItems: 'center',
          gap: 6,
        }}
      >
        <BranchesOutlined style={{ fontSize: 11, color: 'var(--color-primary)' }} />
        <span>Worktree: {displayPath}</span>
      </div>
    </Tooltip>
  );
}
