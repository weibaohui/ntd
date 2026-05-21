import { useEffect, useState, useMemo, useRef } from 'react';
import { useApp } from '../hooks/useApp';
import { Button, Empty, App, Popconfirm, Tag, Badge, Pagination, Segmented, Modal, Input, Tooltip, Select } from 'antd';
import { PlayCircleOutlined, EditOutlined, DeleteOutlined, CheckCircleOutlined, ReloadOutlined, CopyOutlined, ArrowLeftOutlined, StopOutlined, DownOutlined, UpOutlined, UnorderedListOutlined, MessageOutlined, FileTextOutlined, LinkOutlined, LoadingOutlined, ThunderboltOutlined } from '@ant-design/icons';
import { StatusPicker } from './StatusPicker';
import { PieChart } from './PieChart';
import { TodoDrawer } from './TodoDrawer';
import { ChatView } from './ChatView';
import { parseLogsToMessages } from './ChatView';
import * as db from '../utils/database';
import { formatLocalDateTime, formatDuration } from '../utils/datetime';
import { conversationToYaml } from '../utils/markdown';
import { AnimatedNumber } from './AnimatedNumber';
import { getExecutorOption, supportsResume } from '../types';
import { ExecutorBadge } from './ExecutorBadge';
import XMarkdown from '@ant-design/x-markdown';
import type { ExecutionSummary, TodoItem, ExecutionRecord, ExecutionStats, LogEntry } from '../types';

/** 统一刷新按钮组件 */
const RefreshBtn = ({ onClick, size = 'small' }: { onClick: () => void; size?: 'small' | 'middle' }) => (
  <Button type="text" size={size} icon={<ReloadOutlined />} aria-label="刷新"
    onClick={(e) => { e.stopPropagation(); onClick(); }} />
);

/** 日志视图头部组件 */
function LogViewHeader({ title, viewMode, onViewModeChange, onRefresh, fontSize = 12 }: {
  title: string;
  viewMode: 'log' | 'chat';
  onViewModeChange: (mode: 'log' | 'chat') => void;
  onRefresh: () => void;
  fontSize?: number;
}) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <span style={{ fontSize, fontWeight: 600, color: 'var(--color-primary)' }}>{title}</span>
        <RefreshBtn onClick={onRefresh} />
      </div>
      <Segmented
        size="small"
        value={viewMode}
        onChange={(value) => onViewModeChange(value as 'log' | 'chat')}
        options={[
          { value: 'log', icon: <UnorderedListOutlined />, label: '日志' },
          { value: 'chat', icon: <MessageOutlined />, label: '对话' },
        ]}
      />
    </div>
  );
}

/** 计算从 started_at 到现在的 elapsed time (秒) */
function getElapsedSeconds(startedAt: string): number {
  const start = new Date(startedAt).getTime();
  const now = Date.now();
  return Math.floor((now - start) / 1000);
}

/** 按 session_id 分组执行记录，同一 session 的记录按时间排序形成链 */
interface SessionGroup {
  sessionId: string;
  records: ExecutionRecord[];
}

function groupBySession(records: ExecutionRecord[]): SessionGroup[] {
  const map = new Map<string, ExecutionRecord[]>();
  // 无 session_id 的记录用自身 id 作为 key（各自独立）
  for (const r of records) {
    const key = r.session_id || `__single_${r.id}`;
    if (!map.has(key)) map.set(key, []);
    map.get(key)!.push(r);
  }
  const groups: SessionGroup[] = [];
  for (const [sessionId, recs] of map) {
    recs.sort((a, b) => (a.started_at || '').localeCompare(b.started_at || ''));
    groups.push({ sessionId, records: recs });
  }
  // 组间按组内最新记录的 started_at DESC 排序
  groups.sort((a, b) => {
    const aLatest = a.records[a.records.length - 1].started_at || '';
    const bLatest = b.records[b.records.length - 1].started_at || '';
    return bLatest.localeCompare(aLatest);
  });
  return groups;
}

/** 可展开的 Prompt 内容展示组件 */
function PromptDisplay({ content }: { content: string }) {
  const [expanded, setExpanded] = useState(false);
  return (
    <div style={{ marginTop: 8 }}>
      <div
        onClick={() => setExpanded(!expanded)}
        style={{
          display: 'inline-flex',
          alignItems: 'center',
          gap: 4,
          fontSize: 12,
          color: 'var(--color-text-secondary)',
          cursor: 'pointer',
          userSelect: 'none',
        }}
      >
        <span>{expanded ? '▼' : '▶'}</span>
        <span>Prompt</span>
      </div>
      {expanded && (
        <div
          style={{
            marginTop: 6,
            padding: '8px 12px',
            borderRadius: 8,
            background: 'var(--color-bg-elevated)',
            border: '1px solid var(--color-border-light)',
            maxHeight: 300,
            overflow: 'auto',
          }}
        >
          <XMarkdown content={content} />
        </div>
      )}
    </div>
  );
}

/** 内联 Token 统计摘要，支持展开查看详细分项 */
function InlineTokenStats({ input, output, cacheRead, cacheCreate, totalTokens, summary }: {
  input: number; output: number; cacheRead: number; cacheCreate: number; totalTokens: number; summary: ExecutionSummary;
}) {
  const [expanded, setExpanded] = useState(false);
  // 推理输入 = 输入 + 缓存读 + 缓存写
  const reasoningInput = input + cacheRead + cacheCreate;
  // 成本输入 = 输入 + 缓存写
  const costInput = input + cacheCreate;
  // 输出率 = 输出 / 成本输入 * 100%
  const outputRate = costInput > 0 ? (output / costInput * 100) : 0;

  const tokenSegments = [
    { value: input, color: '#3b82f6', label: '输入' },
    { value: output, color: '#22c55e', label: '输出' },
    { value: cacheRead, color: '#f59e0b', label: '缓存读' },
    { value: cacheCreate, color: '#a78bfa', label: '缓存写' },
  ];
  const extraSegments = [
    { value: reasoningInput, color: '#ec4899', label: '推理输入' },
    { value: costInput, color: '#f97316', label: '成本输入' },
    { value: outputRate, color: '#14b8a6', label: '输出率', isPercent: true },
  ];
  return (
    <div style={{ position: 'relative', display: 'inline-flex', alignItems: 'center' }}>
      <button
        type="button"
        aria-expanded={expanded}
        aria-label="Token 统计摘要，点击展开详情"
        onClick={() => setExpanded(!expanded)}
        style={{ display: 'inline-flex', alignItems: 'center', gap: 8, cursor: 'pointer', userSelect: 'none', fontSize: 11, color: 'var(--color-text-secondary)', background: 'none', border: 'none', padding: 0 }}
      >
        <PieChart segments={tokenSegments.filter(s => s.value > 0)} size={20} />
        <span style={{ fontWeight: 700, color: 'var(--color-text)', fontSize: 13 }}><AnimatedNumber value={totalTokens} duration={1.2} chineseFormat /></span>
        <span>Tokens</span>
        <span style={{ color: 'var(--color-border)' }}>|</span>
        <span>执行 <strong style={{ color: 'var(--color-text)' }}>{summary.total_executions}</strong> 次</span>
        <span style={{ color: 'var(--color-success)' }}>成功 {summary.success_count}</span>
        <span style={{ color: 'var(--color-error)' }}>失败 {summary.failed_count}</span>
        {summary.total_cost_usd != null && (
          <span style={{ color: 'var(--color-warning)', fontWeight: 600 }}>${summary.total_cost_usd.toFixed(4)}</span>
        )}
        {expanded ? <UpOutlined style={{ fontSize: 10 }} /> : <DownOutlined style={{ fontSize: 10 }} />}
      </button>
      {expanded && (
        <div style={{ position: 'absolute', top: '100%', left: 0, zIndex: 10, marginTop: 4, background: 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)', borderRadius: 8, padding: 10, boxShadow: '0 4px 12px rgba(0,0,0,0.15)', minWidth: 280 }}>
          <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap', fontSize: 11 }}>
            {tokenSegments.filter(s => s.value > 0).map(s => (
              <span key={s.label} style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                <span style={{ width: 8, height: 8, borderRadius: '50%', background: s.color }} />
                {s.label}: <AnimatedNumber value={s.value} duration={1.2} chineseFormat />
              </span>
            ))}
          </div>
          <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap', fontSize: 11, marginTop: 8, paddingTop: 8, borderTop: '1px solid var(--color-border-light)' }}>
            {extraSegments.map(s => (
              <span key={s.label} style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                <span style={{ width: 8, height: 8, borderRadius: '50%', background: s.color }} />
                {s.label}: {s.isPercent ? s.value.toFixed(1) + '%' : <AnimatedNumber value={s.value} duration={1.2} chineseFormat />}
              </span>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

/** 任务进度展示组件，显示子项完成情况 */
function ProgressWidget({ items }: { items: TodoItem[] }) {
  const [expanded, setExpanded] = useState(false);
  const total = items.length;
  const completed = items.filter(t => t.status === 'completed').length;
  const pct = Math.round((completed / total) * 100);

  return (
    <div style={{ position: 'relative', flexShrink: 0 }}>
      <div
        onClick={() => setExpanded(!expanded)}
        style={{
          background: 'var(--color-bg-elevated)',
          borderRadius: 6,
          padding: '4px 10px',
          border: `1px solid ${expanded ? 'var(--color-primary)' : 'var(--color-border-light)'}`,
          minWidth: 120,
          cursor: 'pointer',
          userSelect: 'none',
          transition: 'border-color 0.2s',
        }}
      >
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 3 }}>
          <span style={{ fontSize: 10, fontWeight: 600, color: 'var(--color-text-secondary)' }}>进度</span>
          <span style={{ fontSize: 10, color: 'var(--color-primary)', fontWeight: 600 }}>{completed}/{total} ({pct}%)</span>
        </div>
        <div style={{ height: 3, borderRadius: 2, background: 'var(--color-border-light)', marginBottom: 3 }}>
          <div style={{ height: '100%', borderRadius: 2, background: 'var(--color-primary)', width: `${pct}%`, transition: 'width 0.3s' }} />
        </div>
        <div style={{ display: 'flex', gap: 3, flexWrap: 'wrap' }}>
          {items.map((item, idx) => (
            <span key={item.id || idx} style={{ fontSize: 10, lineHeight: '14px', color: item.status === 'completed' ? 'var(--color-text-tertiary)' : item.status === 'in_progress' ? 'var(--color-primary)' : 'var(--color-text-secondary)', textDecoration: item.status === 'completed' ? 'line-through' : 'none', maxWidth: 80, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              {item.status === 'completed' ? '✓' : item.status === 'in_progress' ? '●' : '○'} {item.content}
            </span>
          ))}
        </div>
      </div>
      {expanded && (
        <div style={{
          position: 'absolute',
          top: '100%',
          right: 0,
          zIndex: 20,
          marginTop: 4,
          background: 'var(--color-bg-elevated)',
          border: '1px solid var(--color-border-light)',
          borderRadius: 8,
          padding: 12,
          boxShadow: '0 6px 20px rgba(0,0,0,0.15)',
          minWidth: 260,
          maxWidth: 360,
        }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
            <span style={{ fontSize: 12, fontWeight: 700, color: 'var(--color-text)' }}>任务进度</span>
            <span style={{ fontSize: 11, color: 'var(--color-primary)', fontWeight: 600 }}>{completed}/{total} ({pct}%)</span>
          </div>
          <div style={{ height: 4, borderRadius: 2, background: 'var(--color-border-light)', marginBottom: 10 }}>
            <div style={{ height: '100%', borderRadius: 2, background: 'var(--color-primary)', width: `${pct}%`, transition: 'width 0.3s' }} />
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6, maxHeight: 280, overflow: 'auto' }}>
            {items.map((item, idx) => (
              <div key={item.id || idx} style={{
                display: 'flex',
                alignItems: 'flex-start',
                gap: 8,
                fontSize: 12,
                lineHeight: '18px',
                color: item.status === 'completed' ? 'var(--color-text-tertiary)' : item.status === 'in_progress' ? 'var(--color-primary)' : 'var(--color-text-secondary)',
                textDecoration: item.status === 'completed' ? 'line-through' : 'none',
                padding: '4px 8px',
                borderRadius: 4,
                background: item.status === 'in_progress' ? 'var(--color-primary-bg)' : 'transparent',
              }}>
                <span style={{ flexShrink: 0, marginTop: 2 }}>
                  {item.status === 'completed' ? '✓' : item.status === 'in_progress' ? '●' : '○'}
                </span>
                <span style={{ wordBreak: 'break-word' }}>{item.content}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

/** 紧凑历史列表项的内容（不含外层容器样式） */
function CompactHistoryItem({ record, onOpenResume, onExport }: {
  record: ExecutionRecord;
  onOpenResume: (r: ExecutionRecord) => void;
  onExport: (r: ExecutionRecord) => void;
}) {
  const isRunning = record.status === 'running';
  const [elapsedSec, setElapsedSec] = useState(isRunning ? getElapsedSeconds(record.started_at) : 0);

  useEffect(() => {
    if (!isRunning) return;
    const tick = () => setElapsedSec(getElapsedSeconds(record.started_at));
    tick();
    const timer = setInterval(tick, 1000);
    return () => clearInterval(timer);
  }, [isRunning, record.started_at]);

  return (
    <>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 4 }}>
        <span style={{ fontSize: 12, color: 'var(--color-text-tertiary)' }}>
          {formatLocalDateTime(record.started_at)}
        </span>
        <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
          {record.status !== 'running' && supportsResume(record) && (
            <MessageOutlined
              style={{ fontSize: 12, color: 'var(--color-primary)', cursor: 'pointer' }}
              title="继续对话"
              onClick={(e) => { e.stopPropagation(); onOpenResume(record); }}
            />
          )}
          {hasLogsStatic(record) && (
            <FileTextOutlined
              style={{ fontSize: 12, color: 'var(--color-text-tertiary)', cursor: 'pointer' }}
              title="导出为YAML"
              onClick={(e) => { e.stopPropagation(); onExport(record); }}
            />
          )}
          <span style={{
            fontSize: 10,
            padding: '2px 8px',
            borderRadius: 10,
            backgroundColor: record.status === 'success' ? 'var(--color-success)' : record.status === 'failed' ? 'var(--color-error)' : 'var(--color-info)',
            color: '#fff',
            fontWeight: 600,
          }}>
            {record.status === 'success' ? '成功' : record.status === 'failed' ? '失败' : '进行中'}
          </span>
        </div>
      </div>
      <div style={{ display: 'flex', gap: 6, alignItems: 'center', flexWrap: 'wrap' }}>
        {record.executor && <ExecutorBadge executor={record.executor} />}
        {record.model && <Tag color="#3b82f6" style={{ fontSize: 10, padding: '0 6px', lineHeight: '18px' }}>{record.model}</Tag>}
        <Tag color={record.trigger_type === 'cron' ? '#8b5cf6' : '#6b7280'} style={{ fontSize: 10, padding: '0 6px', lineHeight: '18px' }}>
          {record.trigger_type === 'cron' ? 'Cron' : '手动'}
        </Tag>
        {record.usage?.duration_ms && (
          <span style={{ fontSize: 10, color: 'var(--color-success)', fontWeight: 600 }}>
            {formatDuration(record.usage.duration_ms / 1000)}
          </span>
        )}
        {isRunning && elapsedSec > 0 && (
          <span style={{ fontSize: 10, color: 'var(--color-info)', fontWeight: 600 }}>
            {formatDuration(elapsedSec)}
          </span>
        )}
        {record.execution_stats && (
          <span style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>
            🔧{record.execution_stats.tool_calls} 💬{record.execution_stats.conversation_turns}
          </span>
        )}
      </div>
    </>
  );
}

function hasLogsStatic(record: ExecutionRecord): boolean {
  return record.status !== 'running' && !!record.finished_at;
}

/** 任务详情面板，包含执行、编辑、历史记录等功能 */
export function TodoDetail({ onBack }: { onBack?: () => void }) {
  const { state, dispatch } = useApp();
  const { message } = App.useApp();
  const { todos, selectedTodoId, executionRecords, runningTasks } = state;
  const [isMobile, setIsMobile] = useState(false);
  const [isWide, setIsWide] = useState(false);
  const [selectedHistoryRecordId, setSelectedHistoryRecordId] = useState<number | null>(null);
  const [viewMode, setViewMode] = useState<'log' | 'chat'>('log');
  const selectedTodo = todos.find(t => t.id === selectedTodoId);

  useEffect(() => {
    const checkMobile = () => setIsMobile(window.innerWidth < 768);
    checkMobile();
    window.addEventListener('resize', checkMobile);
    return () => window.removeEventListener('resize', checkMobile);
  }, []);

  useEffect(() => {
    const checkWide = () => setIsWide(window.innerWidth >= 1440);
    checkWide();
    window.addEventListener('resize', checkWide);
    return () => window.removeEventListener('resize', checkWide);
  }, []);

  const [todoDrawerOpen, setTodoDrawerOpen] = useState(false);
  const [summary, setSummary] = useState<ExecutionSummary | null>(null);

  // Execution history pagination state
  const [historyPage, setHistoryPage] = useState(1);
  const [historyLimit, setHistoryLimit] = useState(5);
  const [historyTotal, setHistoryTotal] = useState(0);

  // Timer for live duration display of running records
  // Check if current todo is executing (has any running task)
  const isExecuting = Object.values(runningTasks).some(
    t => t.todoId === selectedTodoId && t.status === 'running'
  );

  const [tick, setTick] = useState(0);
  useEffect(() => {
    if (!isExecuting) return;
    const interval = setInterval(() => {
      setTick(t => t + 1);
    }, 1000);
    return () => clearInterval(interval);
  }, [isExecuting]);
  // Access tick to prevent unused warning (triggers re-render awareness)
  useEffect(() => { void tick; }, [tick]);

  const records = selectedTodoId ? executionRecords[selectedTodoId] || [] : [];

  // 执行历史状态筛选
  const [historyStatusFilter, setHistoryStatusFilter] = useState<'all' | 'running' | 'success' | 'failed'>('all');

  // 懒加载：点击记录时才获取完整详情（含分页 logs）
  const [selectedHistoryRecordDetail, setSelectedHistoryRecordDetail] = useState<ExecutionRecord | null>(null);
  const [isLoadingDetail, setIsLoadingDetail] = useState(false);

  // 分页日志状态
  const [paginatedLogs, setPaginatedLogs] = useState<LogEntry[]>([]);
  const [logsTotal, setLogsTotal] = useState(0);
  const [logsPage, setLogsPage] = useState(1);
  const [logsPerPage] = useState(200);
  const [isLoadingLogs, setIsLoadingLogs] = useState(false);
  const activeRecordIdRef = useRef<number | null>(null);

  // 加载分页日志
  const loadLogs = async (recordId: number, page: number) => {
    setIsLoadingLogs(true);
    try {
      const result = await db.getExecutionLogs(recordId, page, logsPerPage);
      if (activeRecordIdRef.current !== recordId) return;
      setPaginatedLogs(result.logs);
      setLogsTotal(result.total);
      setLogsPage(result.page);
    } catch {
      if (activeRecordIdRef.current === recordId) setPaginatedLogs([]);
    } finally {
      if (activeRecordIdRef.current === recordId) setIsLoadingLogs(false);
    }
  };

  // 当选择的记录变化时，懒加载详情
  useEffect(() => {
    activeRecordIdRef.current = selectedHistoryRecordId;
    if (!selectedHistoryRecordId) {
      setSelectedHistoryRecordDetail(null);
      setPaginatedLogs([]);
      setLogsTotal(0);
      setLogsPage(1);
      return;
    }
    const requestId = selectedHistoryRecordId;
    // 先从 records 中找到基本记录
    const basicRecord = records.find(r => r.id === requestId);

    // 懒加载完整记录（即使 basicRecord 不存在也触发）
    setIsLoadingDetail(true);
    db.getExecutionRecord(requestId)
      .then(detail => {
        if (activeRecordIdRef.current !== requestId) return;
        setSelectedHistoryRecordDetail(detail);
        if (selectedTodoId) {
          dispatch({
            type: 'UPDATE_EXECUTION_RECORD',
            payload: { todoId: selectedTodoId, record: detail }
          });
        }
      })
      .catch(() => {
        if (activeRecordIdRef.current !== requestId) return;
        if (basicRecord) {
          setSelectedHistoryRecordDetail(basicRecord);
        }
      })
      .finally(() => {
        if (activeRecordIdRef.current === requestId) setIsLoadingDetail(false);
      });

    // 同时加载第一页日志
    loadLogs(requestId, 1);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedHistoryRecordId]);

  // selectedHistoryRecord 优先使用懒加载的详情，否则用列表中的基本记录
  const selectedHistoryRecord = selectedHistoryRecordDetail || (selectedHistoryRecordId
    ? records.find(r => r.id === selectedHistoryRecordId) || null
    : null);

  // Find the running task that matches a specific execution record by task_id
  const getRunningTaskForRecord = (record: ExecutionRecord) => {
    if (record.task_id) {
      return runningTasks[record.task_id] || null;
    }
    // Fallback: match by todoId for records without task_id
    return Object.values(runningTasks).find(t => t.todoId === record.todo_id) || null;
  };

  // Helper to resolve execution stats from record or running task
  const resolveExecutionStats = (record: ExecutionRecord, isRunning: boolean) => {
    if (isRunning) {
      const task = getRunningTaskForRecord(record);
      if (task?.executionStats) return task.executionStats;
    }
    return record.execution_stats;
  };

  const loadExecutionRecords = async (page = 1, limit = historyLimit) => {
    if (!selectedTodo) return;
    try {
      const statusFilter = historyStatusFilter === 'all' ? undefined : historyStatusFilter;
      const pageData = await db.getExecutionRecords(selectedTodo.id, page, limit, statusFilter);

      // 只加载当前页的记录，不再预加载会话链（会话链在点击查看详情时单独加载）
      dispatch({
        type: 'SET_EXECUTION_RECORDS',
        payload: { todoId: selectedTodo.id, records: pageData.records }
      });
      setHistoryPage(pageData.page);
      setHistoryLimit(pageData.limit);
      setHistoryTotal(pageData.total);
    } catch {
      // ignore: interceptor already shows error
    }
  };

  const refreshSingleRecord = async (recordId: number) => {
    if (!selectedTodo) return;
    try {
      const record = await db.getExecutionRecord(recordId);
      dispatch({
        type: 'UPDATE_EXECUTION_RECORD',
        payload: { todoId: selectedTodo.id, record }
      });
    } catch {
      // ignore
    }
  };

  useEffect(() => {
    let cancelled = false;
    if (selectedTodo) {
      setHistoryPage(1);

      const statusFilter = historyStatusFilter === 'all' ? undefined : historyStatusFilter;
      db.getExecutionRecords(selectedTodo.id, 1, historyLimit, statusFilter).then(pageData => {
        if (cancelled) return;
        dispatch({
          type: 'SET_EXECUTION_RECORDS',
          payload: { todoId: selectedTodo.id, records: pageData.records }
        });
        setHistoryPage(pageData.page);
        setHistoryLimit(pageData.limit);
        setHistoryTotal(pageData.total);
      }).catch(() => {});

      db.getExecutionSummary(selectedTodo.id).then(sum => {
        if (!cancelled) setSummary(sum);
      }).catch(() => {});
    } else {
      setTodoDrawerOpen(false);
    }
    return () => { cancelled = true; };
  }, [selectedTodoId, selectedTodo, dispatch, historyLimit, historyStatusFilter]);

  useEffect(() => {
    setSelectedHistoryRecordId(null);
  }, [selectedTodoId]);

  useEffect(() => {
    if (!isWide || records.length === 0) return;
    if (selectedHistoryRecordId !== null && records.find(r => r.id === selectedHistoryRecordId)) return;
    setSelectedHistoryRecordId(records[0].id);
  }, [isWide, records, selectedHistoryRecordId]);

  const handleExecute = async () => {
    if (!selectedTodo) return;
    try {
      await db.executeTodo(
        selectedTodo.id,
        selectedTodo.executor || undefined,
        undefined
      );
      message.success('任务已开始执行');
    } catch (error) {
      message.error('执行失败: ' + (error instanceof Error ? error.message : String(error)));
    }
  };

  // 带参执行相关状态
  const [executeWithArgsModalOpen, setExecuteWithArgsModalOpen] = useState(false);
  const [executeArgs, setExecuteArgs] = useState('');
  const [executeWithArgsLoading, setExecuteWithArgsLoading] = useState(false);

  const handleOpenExecuteWithArgs = () => {
    setExecuteArgs('');
    setExecuteWithArgsModalOpen(true);
  };

  const handleExecuteWithArgs = async () => {
    if (!selectedTodo) return;
    setExecuteWithArgsLoading(true);
    try {
      const params = executeArgs.trim() ? { message: executeArgs.trim() } : undefined;
      await db.executeTodo(
        selectedTodo.id,
        selectedTodo.executor || undefined,
        params
      );
      message.success('任务已开始执行');
      setExecuteWithArgsModalOpen(false);
      setExecuteArgs('');
    } catch (error) {
      message.error('执行失败: ' + (error instanceof Error ? error.message : String(error)));
    } finally {
      setExecuteWithArgsLoading(false);
    }
  };

  const handleStopExecution = async (recordId: number) => {
    try {
      await db.stopExecution(recordId);
      message.info('已发送停止指令');
      await loadExecutionRecords(historyPage, historyLimit);
    } catch (error) {
      message.error('停止失败: ' + (error instanceof Error ? error.message : String(error)));
    }
  };

  // Resume conversation state & handlers
  const [resumeModalOpen, setResumeModalOpen] = useState(false);
  const [resumeRecordId, setResumeRecordId] = useState<number | null>(null);
  const [resumeMessage, setResumeMessage] = useState('');
  const [resumeLoading, setResumeLoading] = useState(false);

  // Always group execution records by session_id
  const sessionGroups = useMemo(() => groupBySession(records), [records]);

  const handleOpenResume = (record: ExecutionRecord) => {
    setResumeRecordId(record.id);
    setResumeMessage('');
    setResumeModalOpen(true);
  };

  const handleResumeConfirm = async () => {
    if (!resumeRecordId) return;
    setResumeLoading(true);
    try {
      await db.resumeExecutionRecord(resumeRecordId, resumeMessage);
      message.success('已继续对话，任务开始执行');
      setResumeModalOpen(false);
      setResumeMessage('');
      await loadExecutionRecords(historyPage, historyLimit);
    } catch (error) {
      message.error('继续对话失败: ' + (error instanceof Error ? error.message : String(error)));
    } finally {
      setResumeLoading(false);
    }
  };

  const parseRecordLogs = (_record: ExecutionRecord): LogEntry[] => {
    return [];
  };

  const hasLogs = (record: ExecutionRecord): boolean => hasLogsStatic(record);

  const handleExportMarkdown = async (record: ExecutionRecord) => {
    let logs: LogEntry[] = [];
    try {
      const result = await db.getExecutionLogs(record.id, 1, 100000);
      logs = result.logs;
    } catch {
      // ignore
    }
    const messages = parseLogsToMessages(logs);
    const executorLabel = record.executor ? getExecutorOption(record.executor).label : undefined;
    const content = conversationToYaml(messages, {
      title: selectedTodo?.title,
      executor: executorLabel,
      model: record.model || undefined,
      startedAt: record.started_at,
      status: record.status,
    });
    const blob = new Blob([content], { type: 'application/x-yaml;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
    a.download = `exec-${record.id}-${timestamp}.yaml`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
    message.success('导出成功');
  };

  const handleStatusChange = async (newStatus: string) => {
    if (!selectedTodo) return;
    try {
      const updated = await db.updateTodo(selectedTodo.id, selectedTodo.title, selectedTodo.prompt || '', newStatus);
      dispatch({ type: 'UPDATE_TODO', payload: updated });
      message.success('状态已更新');
    } catch {
      // ignore: interceptor already shows error
    }
  };

  const handleDelete = async () => {
    if (!selectedTodo) return;
    try {
      await db.deleteTodo(selectedTodo.id);
      dispatch({ type: 'DELETE_TODO', payload: selectedTodo.id });
      dispatch({ type: 'SELECT_TODO', payload: null });
      message.success('删除成功');
    } catch {
      // ignore: interceptor already shows error
    }
  };

  if (!selectedTodo) {
    return (
      <div className="detail-panel" style={{ display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
        <div className="empty-state">
          <div className="empty-state-icon">
            <CheckCircleOutlined />
          </div>
          <Empty
            description={
              <div style={{ color: 'var(--color-text-tertiary)', fontSize: 14 }}>
                选择一个任务查看详情
              </div>
            }
            image={Empty.PRESENTED_IMAGE_SIMPLE}
          />
        </div>
      </div>
    );
  }

  const executor = selectedTodo.executor || 'claudecode';

  // Resolve current todo progress for header widget — follows selected execution record
  const currentTodoProgress = (() => {
    // Try to find the record by selectedHistoryRecordId first, then fallback to first record
    const source = selectedHistoryRecord
      || (selectedHistoryRecordId ? records.find(r => r.id === selectedHistoryRecordId) : null)
      || (records.length > 0 ? records[0] : null);
    if (!source) return null;
    if (source.status === 'running') {
      const task = getRunningTaskForRecord(source);
      if (task?.todoProgress?.length) return task.todoProgress;
    }
    if (source.todo_progress) {
      try {
        const parsed = JSON.parse(source.todo_progress);
        if (Array.isArray(parsed) && parsed.length > 0) return parsed;
      } catch { /* ignore */ }
    }
    return null;
  })();

  return (
    <div className={`detail-panel${isWide ? ' detail-panel-wide' : ''}`}>
      {/* Mobile Back Button */}
      {isMobile && (
        <Button
          type="text"
          icon={<ArrowLeftOutlined />}
          onClick={() => {
            if (onBack) {
              onBack();
            } else {
              dispatch({ type: 'SELECT_TODO', payload: null });
            }
          }}
          style={{ marginBottom: 8, marginLeft: -4 }}
        >
          返回
        </Button>
      )}
      {/* Unified Header: Title + Stats + Execute */}
      <div className="detail-card header-card">
        {/* Row 1: Title + Action Buttons */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6 }}>
          <StatusPicker value={selectedTodo.status} onChange={handleStatusChange} disabled={isExecuting} />
          <h2 className="card-title" style={{ margin: 0, flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{selectedTodo.title}</h2>
          <div style={{ display: 'flex', gap: 4, flexShrink: 0 }}>
            <Button type="text" icon={<EditOutlined />} onClick={() => setTodoDrawerOpen(true)} className="icon-btn" aria-label="编辑任务" />
            <Popconfirm title="删除任务" description="确定要删除吗？" onConfirm={handleDelete}>
              <Button type="text" danger icon={<DeleteOutlined />} className="icon-btn" aria-label="删除任务" />
            </Popconfirm>
          </div>
        </div>
        {/* Row 2: Tags + Inline Token Stats + Progress Widget */}
        <div style={{ display: 'flex', alignItems: 'flex-start', gap: 10, flexWrap: 'wrap' }}>
          {/* Tags & Meta */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
            <ExecutorBadge executor={executor} />
            {selectedTodo.scheduler_enabled ? (
              <Tag color="var(--color-primary)" style={{ fontWeight: 600, fontSize: 11 }}>
                调度: {selectedTodo.scheduler_config}
              </Tag>
            ) : (
              <Tag style={{ fontWeight: 600, fontSize: 11, color: 'var(--color-text-tertiary)', borderColor: 'var(--color-border)' }}>
                调度: 关闭
              </Tag>
            )}
            {records.length > 0 && (
              <span style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>
                上次: {formatLocalDateTime(records[0].started_at)}
              </span>
            )}
            {selectedTodo.scheduler_next_run_at && (
              <span style={{ fontSize: 11, color: 'var(--color-success)' }}>
                下次: {formatLocalDateTime(selectedTodo.scheduler_next_run_at)}
              </span>
            )}
            {isExecuting && (
              <>
                <span style={{ color: 'var(--color-border)' }}>|</span>
                <Badge status="processing" />
                <span style={{ fontSize: 12, color: 'var(--color-primary)', fontWeight: 500 }}>执行中...</span>
              </>
            )}
          </div>
          {/* Inline Token Stats */}
          {summary && summary.total_executions > 0 && (() => {
            const input = summary.total_input_tokens;
            const output = summary.total_output_tokens;
            const cacheRead = (summary as any).total_cache_read_tokens ?? 0;
            const cacheCreate = (summary as any).total_cache_creation_tokens ?? 0;
            const totalTokens = input + output + cacheRead + cacheCreate;
            return (
              <InlineTokenStats input={input} output={output} cacheRead={cacheRead} cacheCreate={cacheCreate} totalTokens={totalTokens} summary={summary} />
            );
          })()}
          {/* Progress Widget (rightmost) */}
          {currentTodoProgress && (
            <div style={{ marginLeft: 'auto', flexShrink: 0 }}>
              <ProgressWidget items={currentTodoProgress} />
            </div>
          )}
        </div>
        {selectedTodo.prompt && <PromptDisplay content={selectedTodo.prompt} />}
        {/* Execute Button Row */}
        <div style={{ display: 'flex', gap: 8 }}>
          <Button
            type="primary"
            icon={<PlayCircleOutlined />}
            onClick={handleExecute}
            block
            className="btn-execute btn-execute-compact"
          >
            直接执行
          </Button>
          <Button
            type="primary"
            icon={<ThunderboltOutlined style={{ color: '#ffffff' }} />}
            onClick={handleOpenExecuteWithArgs}
            block
            className="btn-execute btn-execute-compact"
          >
            带参执行
          </Button>
        </div>
      </div>

      {/* Execution History */}
      <div
        style={isWide
          ? { flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden', minHeight: 0 }
          : { paddingBottom: 20, flexShrink: 0 }
        }
      >
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 12, ...(isWide ? { flexShrink: 0 } : {}) }}>
          <h4 style={{ margin: 0, fontSize: 15, fontWeight: 700, color: 'var(--color-text)' }}>执行历史</h4>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <Select
              size="small"
              value={historyStatusFilter}
              onChange={setHistoryStatusFilter}
              style={{ width: 100 }}
              options={[
                { value: 'all', label: '全部' },
                { value: 'running', label: '进行中' },
                { value: 'success', label: '成功' },
                { value: 'failed', label: '失败' },
              ]}
            />
            <Button
              type="text"
              size="small"
              icon={<ReloadOutlined />}
              onClick={() => loadExecutionRecords(historyPage, historyLimit)}
              loading={isExecuting}
            >
              刷新
            </Button>
          </div>
        </div>
        {records.length === 0 ? (
          <Empty description="暂无执行记录" image={Empty.PRESENTED_IMAGE_SIMPLE} />
        ) : isWide ? (
          <div style={{ flex: 1, display: 'flex', gap: 16, overflow: 'hidden', minHeight: 0 }}>
            {/* Left: History List */}
            <div style={{ width: 320, flexShrink: 0, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
              <div className="history-list-column">
                {sessionGroups.map(group => {
                  const isSingle = group.records.length === 1 || !group.records[0].session_id;
                  if (isSingle) {
                    return group.records.map(record => {
                      const isSelected = selectedHistoryRecordId === record.id;
                      return (
                        <div
                          key={record.id}
                          className={`history-item-compact${isSelected ? ' selected' : ''}${record.status === 'failed' ? ' failed' : record.status === 'running' ? ' running' : ''}`}
                          onClick={() => setSelectedHistoryRecordId(record.id)}
                        >
                          <CompactHistoryItem record={record} onOpenResume={handleOpenResume} onExport={handleExportMarkdown} />
                        </div>
                      );
                    });
                  }
                  // Chain group: main record + indented continuations
                  const mainRecord = group.records[0];
                  const continuations = group.records.slice(1);
                  const mainSelected = selectedHistoryRecordId === mainRecord.id;
                  return (
                    <div key={group.sessionId} style={{ marginBottom: 6 }}>
                      {/* Main record */}
                      <div
                        className={`history-item-compact${mainSelected ? ' selected' : ''}`}
                        onClick={() => setSelectedHistoryRecordId(mainRecord.id)}
                      >
                        <CompactHistoryItem record={mainRecord} onOpenResume={handleOpenResume} onExport={handleExportMarkdown} />
                      </div>
                      {/* Indented continuations */}
                      {continuations.map((record, idx) => {
                        const isSelected = selectedHistoryRecordId === record.id;
                        const isLast = idx === continuations.length - 1;
                        return (
                          <div
                            key={record.id}
                            onClick={() => setSelectedHistoryRecordId(record.id)}
                            style={{
                              marginLeft: 12,
                              padding: '6px 8px',
                              borderLeft: '2px solid var(--color-primary)',
                              borderBottom: '1px solid var(--color-border-light)',
                              cursor: 'pointer',
                              background: isSelected ? 'var(--color-primary-bg)' : 'var(--color-bg-elevated)',
                              transition: 'background 0.15s',
                              marginBottom: 1,
                            }}
                          >
                            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 2 }}>
                              <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 10, color: 'var(--color-primary)', fontWeight: 500 }}>
                                <LinkOutlined style={{ fontSize: 10 }} />
                                {record.resume_message ? (
                                  <span style={{ color: 'var(--color-text-secondary)', fontWeight: 400 }}>{String(record.resume_message).length > 30 ? String(record.resume_message).substring(0, 30) + '...' : record.resume_message}</span>
                                ) : (
                                  <span>继续对话</span>
                                )}
                              </span>
                              <span style={{
                                fontSize: 9, padding: '1px 6px', borderRadius: 8,
                                backgroundColor: record.status === 'success' ? 'var(--color-success)' : record.status === 'failed' ? 'var(--color-error)' : 'var(--color-info)',
                                color: '#fff', fontWeight: 600,
                              }}>
                                {record.status === 'success' ? '✓' : record.status === 'failed' ? '✗' : '...'}
                              </span>
                            </div>
                            <div style={{ display: 'flex', gap: 4, alignItems: 'center', flexWrap: 'wrap' }}>
                              <span style={{ fontSize: 9, color: 'var(--color-text-tertiary)' }}>
                                {formatLocalDateTime(record.started_at)}
                              </span>
                              {record.status !== 'running' && record.usage?.duration_ms && (
                                <span style={{ fontSize: 9, color: 'var(--color-success)', fontWeight: 600 }}>
                                  {formatDuration(record.usage.duration_ms / 1000)}
                                </span>
                              )}
                              {record.status === 'running' && (
                                <span style={{ fontSize: 9, color: 'var(--color-info)', fontWeight: 600 }}>
                                  {formatDuration(getElapsedSeconds(record.started_at))}
                                </span>
                              )}
                              {record.execution_stats && (
                                <span style={{ fontSize: 9, color: 'var(--color-text-tertiary)' }}>
                                  🔧{record.execution_stats.tool_calls}
                                </span>
                              )}
                            </div>
                            {isLast && record.status !== 'running' && supportsResume(record) && (
                              <MessageOutlined
                                style={{ fontSize: 11, color: 'var(--color-primary)', cursor: 'pointer', marginTop: 3 }}
                                title="继续对话"
                                onClick={(e) => { e.stopPropagation(); handleOpenResume(record); }}
                              />
                            )}
                          </div>
                        );
                      })}
                    </div>
                  );
                })}
              </div>
              {historyTotal > historyLimit && (
                <div style={{ flexShrink: 0, display: 'flex', justifyContent: 'center', padding: '8px 0 0', borderTop: '1px solid var(--color-border-light)' }}>
                  <Pagination
                    simple
                    current={historyPage}
                    pageSize={historyLimit}
                    total={historyTotal}
                    onChange={(page, pageSize) => {
                      if (pageSize !== historyLimit) {
                        setHistoryLimit(pageSize);
                        loadExecutionRecords(1, pageSize);
                      } else {
                        loadExecutionRecords(page, historyLimit);
                      }
                    }}
                    size="small"
                  />
                </div>
              )}
            </div>
            {/* Divider */}
            <div style={{ width: 1, background: 'var(--color-border-light)', flexShrink: 0 }} />
            {/* Right: Record Detail */}
            <div className="history-detail-column">
              {isLoadingDetail ? (
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%', gap: 8, color: 'var(--color-text-secondary)' }}>
                  <LoadingOutlined style={{ fontSize: 20, color: 'var(--color-primary)' }} />
                  <span>加载执行详情...</span>
                </div>
              ) : selectedHistoryRecord ? (() => {
                const record = selectedHistoryRecord;
                const isRunning = record.status === 'running';
                const runningTask = isRunning ? getRunningTaskForRecord(record) : null;
                const liveLogs = runningTask ? runningTask.logs : null;
                const displayLogs = liveLogs && liveLogs.length > 0 ? liveLogs : paginatedLogs;
                return (
                  <>
                    {/* Chain breadcrumb — when viewing a continuation record */}
                    {(() => {
                      const group = sessionGroups.find(g => g.records.some(r => r.id === record.id));
                      if (!group || group.records.length <= 1 || !group.records[0].session_id) return null;
                      const idx = group.records.findIndex(r => r.id === record.id);
                      if (idx <= 0) return null;
                      return (
                        <div style={{
                          display: 'flex', alignItems: 'center', gap: 6,
                          marginBottom: 10, padding: '4px 10px', borderRadius: 6,
                          background: 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)',
                          fontSize: 11, color: 'var(--color-text-tertiary)',
                        }}>
                          <LinkOutlined style={{ color: 'var(--color-primary)', fontSize: 11 }} />
                          <span>继续自</span>
                          <span
                            onClick={() => setSelectedHistoryRecordId(group.records[0].id)}
                            style={{ cursor: 'pointer', color: 'var(--color-primary)', fontWeight: 500 }}
                          >
                            {formatLocalDateTime(group.records[0].started_at)}
                          </span>
                          {record.resume_message && (
                            <>
                              <span style={{ color: 'var(--color-border)' }}>·</span>
                              <span style={{ color: 'var(--color-text-secondary)', fontStyle: 'italic' }}>
                                "{String(record.resume_message).length > 40 ? String(record.resume_message).substring(0, 40) + '...' : record.resume_message}"
                              </span>
                            </>
                          )}
                          <span style={{ marginLeft: 'auto', color: 'var(--color-text-quaternary)' }}>
                            第{idx + 1}轮 / 共{group.records.length}轮
                          </span>
                        </div>
                      );
                    })()}
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12, flexWrap: 'wrap', gap: 8 }}>
                      <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
                        {record.executor && <ExecutorBadge executor={record.executor} />}
                        {record.model && <Tag color="#3b82f6">{record.model}</Tag>}
                        <span style={{ fontSize: 13, color: 'var(--color-text-secondary)', fontWeight: 500 }}>
                          {formatLocalDateTime(record.started_at)}
                        </span>
                        <span style={{
                          fontSize: 11,
                          padding: '3px 12px',
                          borderRadius: 12,
                          backgroundColor: record.status === 'success' ? 'var(--color-success)' : record.status === 'failed' ? 'var(--color-error)' : 'var(--color-info)',
                          color: '#fff',
                          fontWeight: 600,
                        }}>
                          {record.status === 'success' ? '成功' : record.status === 'failed' ? '失败' : '进行中'}
                        </span>
                        {record.status !== 'running' && record.usage?.duration_ms && (
                          <span style={{ fontSize: 12, color: 'var(--color-success)', fontWeight: 600 }}>
                            {formatDuration(record.usage.duration_ms / 1000)}
                          </span>
                        )}
                        {record.status === 'running' && (
                          <span style={{ fontSize: 12, color: 'var(--color-info)', fontWeight: 600 }}>
                            {formatDuration(getElapsedSeconds(record.started_at))}
                          </span>
                        )}
                      </div>
                      <div style={{ display: 'flex', gap: 8 }}>
                        {record.status !== 'running' && supportsResume(record) && (
                          <Button type="primary" size="small" icon={<MessageOutlined />} onClick={() => handleOpenResume(record)}>继续对话</Button>
                        )}
                        {hasLogs(record) && (
                          <Button size="small" icon={<FileTextOutlined />} onClick={() => handleExportMarkdown(record)}>导出YAML</Button>
                        )}
                        {record.status === 'running' && (
                          <Popconfirm
                            title="确定强制停止该任务？"
                            okText="停止"
                            cancelText="取消"
                            onConfirm={async () => { await handleStopExecution(record.id); }}
                          >
                            <Button type="primary" danger size="small" icon={<StopOutlined />}>停止</Button>
                          </Popconfirm>
                        )}
                      </div>
                    </div>
                    {record.command && (
                      <Tooltip title="点击复制命令">
                        <div
                          onClick={() => { navigator.clipboard.writeText(record.command || '').then(() => message.success('已复制')); }}
                          style={{ fontSize: 11, color: 'var(--color-text-quaternary)', marginBottom: 12, fontFamily: 'var(--font-mono)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', cursor: 'pointer' }}
                        >
                          {record.command}
                        </div>
                      </Tooltip>
                    )}
                    {record.result !== null && record.result !== '' && (
                      <div className={`history-result ${record.status === 'success' ? 'history-result-success' : 'history-result-failed'}`} style={{ marginBottom: 12 }}>
                        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 4 }}>
                          <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--color-text)' }}>结论</span>
                          <Button
                            type="text"
                            size="small"
                            icon={<CopyOutlined />}
                            onClick={async () => {
                              try {
                                await navigator.clipboard.writeText(record.result || '');
                                message.success('已复制到剪贴板');
                              } catch {
                                message.error('复制失败');
                              }
                            }}
                          />
                        </div>
                        <XMarkdown content={record.result} />
                      </div>
                    )}
                    {record.usage && (
                      <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 12, display: 'flex', gap: 12, flexWrap: 'wrap' }}>
                        <span>Input: {record.usage.input_tokens.toLocaleString()}</span>
                        <span>Output: {record.usage.output_tokens.toLocaleString()}</span>
                        {record.usage.total_cost_usd !== null && (
                          <span style={{ color: 'var(--color-warning)', fontWeight: 600 }}>${record.usage.total_cost_usd.toFixed(6)}</span>
                        )}
                      </div>
                    )}
                    {(() => {
                      const stats = resolveExecutionStats(record, isRunning);
                      if (!stats) return null;
                      return (
                        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 12, display: 'flex', gap: 12, flexWrap: 'wrap' }}>
                          <span>工具调用: <b style={{ color: 'var(--color-primary)' }}>{stats.tool_calls}</b></span>
                          <span>对话轮次: <b style={{ color: 'var(--color-primary)' }}>{stats.conversation_turns}</b></span>
                          {stats.thinking_count > 0 && (
                            <span>思考次数: <b style={{ color: 'var(--color-primary)' }}>{stats.thinking_count}</b></span>
                          )}
                        </div>
                      );
                    })()}
                    {(() => {
                      if (!isRunning && displayLogs.length === 0) return null;
                      if (viewMode === 'chat') {
                        return (
                          <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
                            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8, flexShrink: 0 }}>
                              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                                <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--color-primary)' }}>
                                  对话视图 ({displayLogs.length} 条){isRunning && liveLogs && liveLogs.length > 0 ? ' · 实时' : ''}
                                </span>
                                <RefreshBtn onClick={() => refreshSingleRecord(record.id)} />
                              </div>
                              <Segmented
                                size="small"
                                value={viewMode}
                                onChange={(value) => setViewMode(value as 'log' | 'chat')}
                                options={[
                                  { value: 'log', icon: <UnorderedListOutlined />, label: '日志' },
                                  { value: 'chat', icon: <MessageOutlined />, label: '对话' },
                                ]}
                              />
                            </div>
                            <ChatView logs={displayLogs as LogEntry[]} isRunning={isRunning} />
                          </div>
                        );
                      }
                      return (
                        <div>
                          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
                            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                              <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--color-primary)' }}>
                                执行过程 ({isRunning ? displayLogs.length : logsTotal} 条{isRunning && liveLogs && liveLogs.length > 0 ? ' · 实时' : ''})
                              </span>
                              <RefreshBtn onClick={() => {
                                refreshSingleRecord(record.id);
                                loadLogs(record.id, logsPage);
                              }} />
                            </div>
                            <Segmented
                              size="small"
                              value={viewMode}
                              onChange={(value) => setViewMode(value as 'log' | 'chat')}
                              options={[
                                { value: 'log', icon: <UnorderedListOutlined />, label: '日志' },
                                { value: 'chat', icon: <MessageOutlined />, label: '对话' },
                              ]}
                            />
                          </div>
                          <div style={{
                            background: 'var(--log-bg)',
                            color: 'var(--log-text)',
                            padding: 12,
                            borderRadius: 8,
                            fontFamily: 'var(--font-mono)',
                            fontSize: 11,
                            overflow: 'auto',
                          }}>
                            {displayLogs.length === 0 ? (
                              <div style={{ color: 'var(--log-text-muted)' }}>{isRunning ? '等待输出...' : (isLoadingLogs ? '加载中...' : '暂无日志')}</div>
                            ) : (
                              displayLogs.map((log, idx) => (
                                <div key={idx} style={{ marginBottom: 4, display: 'flex', gap: 8 }}>
                                  <span style={{ color: 'var(--log-text-muted)', flexShrink: 0 }}>{formatLogTime(log.timestamp || '')}</span>
                                  <span style={{ color: logTypeColors[log.type || ''] || 'var(--log-text)' }}>
                                    [{logTypeLabels[log.type || ''] || log.type}]
                                  </span>
                                  <span>{log.content}</span>
                                </div>
                              ))
                            )}
                          </div>
                          {!isRunning && logsTotal > logsPerPage && (
                            <Pagination
                              simple
                              current={logsPage}
                              pageSize={logsPerPage}
                              total={logsTotal}
                              onChange={(page) => loadLogs(record.id, page)}
                              size="small"
                              style={{ marginTop: 8, textAlign: 'center' }}
                            />
                          )}
                        </div>
                      );
                    })()}
                  </>
                );
              })() : (
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%' }}>
                  <Empty description="选择一条执行记录查看详情" image={Empty.PRESENTED_IMAGE_SIMPLE} />
                </div>
              )}
            </div>
          </div>
        ) : (
          <>
            {sessionGroups.map(group => {
              const isSingle = group.records.length === 1 || !group.records[0].session_id;
              if (isSingle) {
                return group.records.map(record => (
                  <NarrowHistoryCard
                    key={record.id}
                    record={record}
                    viewMode={viewMode}
                    onOpenResume={handleOpenResume}
                    onExport={handleExportMarkdown}
                    onStop={handleStopExecution}
                    onRefresh={refreshSingleRecord}
                    getRunningTask={getRunningTaskForRecord}
                    resolveStats={resolveExecutionStats}
                    parseLogs={parseRecordLogs}
                    messageApi={message}
                    onViewModeChange={setViewMode}
                  />
                ));
              }
              return (
                <ChainGroupCard
                  key={group.sessionId}
                  group={group}
                  onOpenResume={handleOpenResume}
                  onExport={handleExportMarkdown}
                  onStop={handleStopExecution}
                  messageApi={message}
                  viewMode={viewMode}
                  parseLogs={parseRecordLogs}
                  onRefresh={refreshSingleRecord}
                  resolveStats={resolveExecutionStats}
                  onViewModeChange={setViewMode}
                />
              );
            })}
            {historyTotal > historyLimit && (
              <div style={{ display: 'flex', justifyContent: 'center', marginTop: 16 }}>
                <Pagination
                  current={historyPage}
                  pageSize={historyLimit}
                  total={historyTotal}
                  onChange={(page, pageSize) => {
                    if (pageSize !== historyLimit) {
                      setHistoryLimit(pageSize);
                      loadExecutionRecords(1, pageSize);
                    } else {
                      loadExecutionRecords(page, historyLimit);
                    }
                  }}
                  size="small"
                  showSizeChanger
                  pageSizeOptions={['5', '10', '20']}
                />
              </div>
            )}
          </>
        )}
      </div>

      <TodoDrawer
        open={todoDrawerOpen}
        todo={selectedTodo}
        tags={state.tags}
        onClose={() => setTodoDrawerOpen(false)}
        onSaved={() => {
          db.getAllTodos().then(todos => {
            dispatch({ type: 'SET_TODOS', payload: todos });
          });
          if (selectedTodoId) {
            db.getExecutionSummary(selectedTodoId).then(sum => {
              setSummary(sum);
            });
          }
        }}
      />

      <Modal
        title="继续对话"
        open={resumeModalOpen}
        onOk={handleResumeConfirm}
        onCancel={() => {
          setResumeModalOpen(false);
          setResumeMessage('');
        }}
        confirmLoading={resumeLoading}
        okText="开始执行"
        cancelText="取消"
      >
        <p style={{ marginBottom: 12, color: 'var(--color-text-secondary)' }}>
          将复用之前的会话上下文继续对话，你可以修改下方消息：
        </p>
        <Input.TextArea
          value={resumeMessage}
          onChange={(e) => setResumeMessage(e.target.value)}
          rows={4}
          placeholder="输入要继续发送的消息..."
        />
      </Modal>

      <Modal
        title={<><ThunderboltOutlined style={{ marginRight: 8 }} />带参执行</>}
        open={executeWithArgsModalOpen}
        onOk={handleExecuteWithArgs}
        onCancel={() => {
          setExecuteWithArgsModalOpen(false);
          setExecuteArgs('');
        }}
        confirmLoading={executeWithArgsLoading}
        okText="开始执行"
        cancelText="取消"
      >
        <p style={{ marginBottom: 12, color: 'var(--color-text-secondary)' }}>
          输入补充信息，将与任务原有内容一起执行：
        </p>
        <Input.TextArea
          value={executeArgs}
          onChange={(e) => setExecuteArgs(e.target.value)}
          rows={4}
          placeholder="输入补充信息..."
        />
      </Modal>
    </div>
  );
}

/** Narrow mode: single history card */
function NarrowHistoryCard({ record, viewMode, onOpenResume, onExport, onStop, onRefresh, getRunningTask, resolveStats, parseLogs, messageApi, onViewModeChange }: {
  record: ExecutionRecord;
  viewMode: 'log' | 'chat';
  onOpenResume: (r: ExecutionRecord) => void;
  onExport: (r: ExecutionRecord) => void;
  onStop: (id: number) => Promise<void>;
  onRefresh: (id: number) => Promise<void>;
  getRunningTask: (r: ExecutionRecord) => any;
  resolveStats: (r: ExecutionRecord, running: boolean) => ExecutionStats | null | undefined;
  parseLogs: (r: ExecutionRecord) => LogEntry[];
  messageApi: any;
  onViewModeChange: (mode: 'log' | 'chat') => void;
}) {
  const isRunning = record.status === 'running';
  const runningTask = isRunning ? getRunningTask(record) : null;
  const liveLogs = runningTask ? runningTask.logs : null;
  const restLogs = parseLogs(record);

  // 懒加载日志（新记录没有旧字段数据时从新表加载）
  const [loadedLogs, setLoadedLogs] = useState<LogEntry[] | null>(null);
  useEffect(() => {
    if (restLogs.length > 0 || loadedLogs !== null) return;
    db.getExecutionLogs(record.id, 1, 200)
      .then(r => setLoadedLogs(r.logs))
      .catch(() => setLoadedLogs([]));
  }, [record.id, restLogs.length, loadedLogs]);

  const displayLogs = liveLogs && liveLogs.length > 0 ? liveLogs :
    restLogs.length > 0 ? restLogs :
    loadedLogs || [];

  return (
    <div className={`history-card history-card-${record.status}`}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8, flexWrap: 'wrap', gap: 8 }}>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
          <span style={{ fontSize: 12, color: 'var(--color-text-tertiary)' }}>
            {formatLocalDateTime(record.started_at)}
          </span>
          {record.executor && <ExecutorBadge executor={record.executor} />}
          {record.model && <Tag color="#3b82f6">{record.model}</Tag>}
          <Tag color={record.trigger_type === 'cron' ? '#8b5cf6' : '#6b7280'} style={{ fontSize: 10 }}>
            {record.trigger_type === 'cron' ? 'Cron' : '手动'}
          </Tag>
          {record.status !== 'running' && record.usage?.duration_ms && (
            <span style={{ fontSize: 11, color: 'var(--color-success)', fontWeight: 600 }}>
              {formatDuration(record.usage.duration_ms / 1000)}
            </span>
          )}
          {record.status === 'running' && (
            <span style={{ fontSize: 11, color: 'var(--color-info)', fontWeight: 600 }}>
              {formatDuration(getElapsedSeconds(record.started_at))}
            </span>
          )}
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
          <span style={{
            fontSize: 11, padding: '3px 12px', borderRadius: 12,
            backgroundColor: record.status === 'success' ? 'var(--color-success)' : record.status === 'failed' ? 'var(--color-error)' : 'var(--color-info)',
            color: '#fff', fontWeight: 600,
          }}>
            {record.status === 'success' ? '成功' : record.status === 'failed' ? '失败' : '进行中'}
          </span>
          {!isRunning && supportsResume(record) && (
            <Button type="primary" size="small" icon={<MessageOutlined />} onClick={() => onOpenResume(record)}>继续对话</Button>
          )}
          {hasLogsStatic(record) && (
            <Button size="small" icon={<FileTextOutlined />} onClick={() => onExport(record)}>导出YAML</Button>
          )}
          {isRunning && (
            <Popconfirm title="确定强制停止该任务？" okText="停止" cancelText="取消" onConfirm={() => onStop(record.id)}>
              <Button type="primary" danger size="middle" icon={<StopOutlined />} style={{ fontSize: 14, fontWeight: 600, height: '32px', display: 'flex', alignItems: 'center', gap: '6px' }}>停止任务</Button>
            </Popconfirm>
          )}
        </div>
      </div>
      {record.command && (
        <Tooltip title="点击复制命令">
          <div
            onClick={() => { navigator.clipboard.writeText(record.command || '').then(() => messageApi.success('已复制')); }}
            style={{ fontSize: 11, color: 'var(--color-text-quaternary)', marginBottom: 8, fontFamily: 'var(--font-mono)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', cursor: 'pointer' }}
          >
            {record.command}
          </div>
        </Tooltip>
      )}
      {record.result !== null && record.result !== '' && (
        <div className={`history-result ${record.status === 'success' ? 'history-result-success' : 'history-result-failed'}`}>
          <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 4 }}>
            <Button type="text" size="small" icon={<CopyOutlined />} onClick={async () => {
              try { await navigator.clipboard.writeText(record.result || ''); messageApi.success('已复制到剪贴板'); }
              catch { messageApi.error('复制失败'); }
            }} />
          </div>
          <XMarkdown content={record.result} />
        </div>
      )}
      {record.usage && (
        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginTop: 8, display: 'flex', gap: 12, flexWrap: 'wrap' }}>
          <span>Input: {record.usage.input_tokens.toLocaleString()}</span>
          <span>Output: {record.usage.output_tokens.toLocaleString()}</span>
          {record.usage.total_cost_usd !== null && (
            <span style={{ color: 'var(--color-warning)', fontWeight: 600 }}>${record.usage.total_cost_usd.toFixed(6)}</span>
          )}
        </div>
      )}
      {(() => {
        const stats = resolveStats(record, isRunning);
        if (!stats) return null;
        return (
          <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginTop: 8, display: 'flex', gap: 12, flexWrap: 'wrap' }}>
            <span>工具调用: <b style={{ color: 'var(--color-primary)' }}>{stats.tool_calls}</b></span>
            <span>对话轮次: <b style={{ color: 'var(--color-primary)' }}>{stats.conversation_turns}</b></span>
            {stats.thinking_count > 0 && (
              <span>思考次数: <b style={{ color: 'var(--color-primary)' }}>{stats.thinking_count}</b></span>
            )}
          </div>
        );
      })()}
      <NarrowLogView
        record={record}
        isRunning={isRunning}
        displayLogs={displayLogs}
        liveLogs={liveLogs}
        viewMode={viewMode}
        onRefresh={onRefresh}
        onViewModeChange={onViewModeChange}
      />
    </div>
  );
}

/** Narrow mode: chain group card — main record with indented continuations */
/** Lazy-load logs for a continuation record in ChainGroupCard */
function ContinuationLogsLoader({ record, viewMode, onRefresh, onViewModeChange }: {
  record: ExecutionRecord;
  viewMode: 'log' | 'chat';
  onRefresh: (id: number) => Promise<void>;
  onViewModeChange: (mode: 'log' | 'chat') => void;
}) {
  const [logs, setLogs] = useState<LogEntry[] | null>(null);
  const [isExpanded, setIsExpanded] = useState(viewMode === 'chat');
  useEffect(() => {
    db.getExecutionLogs(record.id, 1, 200)
      .then(r => setLogs(r.logs))
      .catch(() => setLogs([]));
  }, [record.id]);
  if (logs === null) return null;
  if (logs.length === 0) return null;
  const title = viewMode === 'chat' ? `对话 (${logs.length})` : `日志 (${logs.length})`;
  return (
    <details style={{ marginTop: 6 }} open={isExpanded} onToggle={(e) => setIsExpanded((e.target as HTMLDetailsElement).open)}>
      <summary style={{ cursor: 'pointer', color: 'var(--color-primary)', fontSize: 10, fontWeight: 600, display: 'flex', alignItems: 'center', gap: 8 }}>
        <span>{title}</span>
        <LogViewHeader
          title=""
          viewMode={viewMode}
          onViewModeChange={onViewModeChange}
          onRefresh={() => onRefresh(record.id)}
          fontSize={10}
        />
      </summary>
      {viewMode === 'chat' ? (
        <div style={{ maxHeight: 300, overflow: 'auto' }}>
          <ChatView logs={logs as LogEntry[]} isRunning={false} />
        </div>
      ) : (
        <div style={{
          background: 'var(--log-bg)', color: 'var(--log-text)', padding: 6, borderRadius: 6,
          fontFamily: 'var(--font-mono)', fontSize: 10, maxHeight: 200, overflow: 'auto',
        }}>
          {logs.map((log, i) => (
            <div key={i} style={{ marginBottom: 3, display: 'flex', gap: 6 }}>
              <span style={{ color: 'var(--log-text-muted)', flexShrink: 0 }}>{formatLogTime(log.timestamp || '')}</span>
              <span>{log.content ?? ''}</span>
            </div>
          ))}
        </div>
      )}
    </details>
  );
}

/** 内联日志视图组件 (用于 ChainGroupCard 内部) */
function ContinuationLogView({ logs, isRunning, viewMode, onRefresh, onViewModeChange }: {
  logs: LogEntry[];
  isRunning: boolean;
  viewMode: 'log' | 'chat';
  onRefresh: () => void;
  onViewModeChange: (mode: 'log' | 'chat') => void;
}) {
  const defaultOpen = isRunning || viewMode === 'chat';
  const [isExpanded, setIsExpanded] = useState(defaultOpen);
  const title = viewMode === 'chat' ? `对话 (${logs.length})` : `日志 (${logs.length})`;
  return (
    <details style={{ marginTop: 6 }} open={isExpanded} onToggle={(e) => setIsExpanded((e.target as HTMLDetailsElement).open)}>
      <summary style={{ cursor: 'pointer', color: 'var(--color-primary)', fontSize: 10, fontWeight: 600, display: 'flex', alignItems: 'center', gap: 8 }}>
        <span>{title}</span>
        <LogViewHeader
          title=""
          viewMode={viewMode}
          onViewModeChange={onViewModeChange}
          onRefresh={onRefresh}
          fontSize={10}
        />
      </summary>
      {viewMode === 'chat' ? (
        <div style={{ maxHeight: 300, overflow: 'auto' }}>
          <ChatView logs={logs as LogEntry[]} isRunning={isRunning} />
        </div>
      ) : (
        <div style={{
          background: 'var(--log-bg)', color: 'var(--log-text)', padding: 6, borderRadius: 6,
          fontFamily: 'var(--font-mono)', fontSize: 10, maxHeight: 200, overflow: 'auto',
        }}>
          {logs.length === 0 ? (
            <div style={{ color: 'var(--log-text-muted)' }}>等待输出...</div>
          ) : (
            logs.map((log, i) => (
              <div key={i} style={{ marginBottom: 3, display: 'flex', gap: 6 }}>
                <span style={{ color: 'var(--log-text-muted)', flexShrink: 0 }}>{formatLogTime(log.timestamp || '')}</span>
                <span style={{ color: logTypeColors[log.type || ''] || 'var(--log-text)' }}>
                  [{logTypeLabels[log.type || ''] || log.type}]
                </span>
                <span>{log.content ?? ''}</span>
              </div>
            ))
          )}
        </div>
      )}
    </details>
  );
}

function ChainGroupCard({ group, onOpenResume, onExport, onStop, messageApi, viewMode, parseLogs, onRefresh, resolveStats, onViewModeChange }: {
  group: SessionGroup;
  onOpenResume: (r: ExecutionRecord) => void;
  onExport: (r: ExecutionRecord) => void;
  onStop: (id: number) => Promise<void>;
  messageApi: any;
  viewMode: 'log' | 'chat';
  parseLogs: (r: ExecutionRecord) => LogEntry[];
  onRefresh: (id: number) => Promise<void>;
  resolveStats: (r: ExecutionRecord, running: boolean) => ExecutionStats | null | undefined;
  onViewModeChange: (mode: 'log' | 'chat') => void;
}) {
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const mainRecord = group.records[0];
  const continuations = group.records.slice(1);

  // 懒加载主记录日志
  const mainRestLogs = parseLogs(mainRecord);
  const [mainLoadedLogs, setMainLoadedLogs] = useState<LogEntry[] | null>(null);
  useEffect(() => {
    if (mainRestLogs.length > 0 || mainLoadedLogs !== null) return;
    db.getExecutionLogs(mainRecord.id, 1, 200)
      .then(r => setMainLoadedLogs(r.logs))
      .catch(() => setMainLoadedLogs([]));
  }, [mainRecord.id, mainRestLogs.length, mainLoadedLogs]);
  const mainDisplayLogs = mainRestLogs.length > 0 ? mainRestLogs : mainLoadedLogs || [];

  return (
    <div>
      {/* Main record card */}
      <div className={`history-card history-card-${mainRecord.status}`}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8, flexWrap: 'wrap', gap: 8 }}>
          <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
            <span style={{ fontSize: 12, color: 'var(--color-text-tertiary)' }}>
              {formatLocalDateTime(mainRecord.started_at)}
            </span>
            {mainRecord.executor && <ExecutorBadge executor={mainRecord.executor} />}
            {mainRecord.model && <Tag color="#3b82f6">{mainRecord.model}</Tag>}
            <Tag color={mainRecord.trigger_type === 'cron' ? '#8b5cf6' : '#6b7280'} style={{ fontSize: 10 }}>
              {mainRecord.trigger_type === 'cron' ? 'Cron' : '手动'}
            </Tag>
            {mainRecord.status !== 'running' && mainRecord.usage?.duration_ms && (
              <span style={{ fontSize: 11, color: 'var(--color-success)', fontWeight: 600 }}>
                {formatDuration(mainRecord.usage.duration_ms / 1000)}
              </span>
            )}
            {mainRecord.status === 'running' && (
              <span style={{ fontSize: 11, color: 'var(--color-info)', fontWeight: 600 }}>
                {formatDuration(getElapsedSeconds(mainRecord.started_at))}
              </span>
            )}
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <span style={{
              fontSize: 11, padding: '3px 12px', borderRadius: 12,
              backgroundColor: mainRecord.status === 'success' ? 'var(--color-success)' : mainRecord.status === 'failed' ? 'var(--color-error)' : 'var(--color-info)',
              color: '#fff', fontWeight: 600,
            }}>
              {mainRecord.status === 'success' ? '成功' : mainRecord.status === 'failed' ? '失败' : '进行中'}
            </span>
            {mainRecord.status !== 'running' && supportsResume(mainRecord) && (
              <Button type="primary" size="small" icon={<MessageOutlined />} onClick={() => onOpenResume(mainRecord)}>继续对话</Button>
            )}
            {hasLogsStatic(mainRecord) && (
              <Button size="small" icon={<FileTextOutlined />} onClick={() => onExport(mainRecord)}>导出YAML</Button>
            )}
            {mainRecord.status === 'running' && (
              <Popconfirm title="确定强制停止该任务？" okText="停止" cancelText="取消" onConfirm={() => onStop(mainRecord.id)}>
                <Button type="primary" danger size="small" icon={<StopOutlined />}>停止</Button>
              </Popconfirm>
            )}
          </div>
        </div>
        {mainRecord.command && (
          <Tooltip title="点击复制命令">
            <div
              onClick={() => { navigator.clipboard.writeText(mainRecord.command || '').then(() => messageApi.success('已复制')); }}
              style={{ fontSize: 11, color: 'var(--color-text-quaternary)', marginBottom: 8, fontFamily: 'var(--font-mono)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', cursor: 'pointer' }}
            >
              {mainRecord.command}
            </div>
          </Tooltip>
        )}
        {mainRecord.result && (
          <div className={`history-result ${mainRecord.status === 'success' ? 'history-result-success' : 'history-result-failed'}`}>
            <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 4 }}>
              <Button type="text" size="small" icon={<CopyOutlined />} onClick={async () => {
                try { await navigator.clipboard.writeText(mainRecord.result || ''); messageApi.success('已复制到剪贴板'); }
                catch { messageApi.error('复制失败'); }
              }} />
            </div>
            <XMarkdown content={mainRecord.result} />
          </div>
        )}
        {(() => {
          const stats = resolveStats(mainRecord, mainRecord.status === 'running');
          if (!stats) return null;
          return (
            <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginTop: 8, display: 'flex', gap: 12, flexWrap: 'wrap' }}>
              <span>工具调用: <b style={{ color: 'var(--color-primary)' }}>{stats.tool_calls}</b></span>
              <span>对话轮次: <b style={{ color: 'var(--color-primary)' }}>{stats.conversation_turns}</b></span>
              {stats.thinking_count > 0 && (
                <span>思考次数: <b style={{ color: 'var(--color-primary)' }}>{stats.thinking_count}</b></span>
              )}
            </div>
          );
        })()}
        <NarrowLogView
          record={mainRecord}
          isRunning={mainRecord.status === 'running'}
          displayLogs={mainDisplayLogs}
          liveLogs={null}
          viewMode={viewMode}
          onRefresh={onRefresh}
          onViewModeChange={onViewModeChange}
        />
      </div>

      {/* Indented continuation entries */}
      {continuations.map((record, idx) => {
        const isLast = idx === continuations.length - 1;
        const isExpanded = expandedId === record.id;
        const logs = parseLogs(record);
        const isRunning = record.status === 'running';
        return (
          <div key={record.id} style={{
            marginLeft: 14,
            borderLeft: '2px solid var(--color-primary)',
            paddingLeft: 10,
            marginTop: 4,
          }}>
            {/* Continuation header — clickable to expand */}
            <div
              onClick={() => setExpandedId(isExpanded ? null : record.id)}
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
                padding: '6px 8px',
                borderRadius: 6,
                background: isExpanded ? 'var(--color-primary-bg)' : 'var(--color-bg-elevated)',
                cursor: 'pointer',
                border: '1px solid var(--color-border-light)',
                transition: 'background 0.15s',
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', gap: 6, minWidth: 0 }}>
                <LinkOutlined style={{ fontSize: 11, color: 'var(--color-primary)', flexShrink: 0 }} />
                <span style={{ fontSize: 11, color: 'var(--color-text-secondary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {record.resume_message || '继续对话'}
                </span>
              </div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 4, flexShrink: 0 }}>
                <span style={{ fontSize: 9, color: 'var(--color-text-tertiary)' }}>
                  {formatLocalDateTime(record.started_at).split(' ')[1] || formatLocalDateTime(record.started_at)}
                </span>
                {record.status !== 'running' && record.usage?.duration_ms && (
                  <span style={{ fontSize: 9, color: 'var(--color-success)', fontWeight: 600 }}>
                    {formatDuration(record.usage.duration_ms / 1000)}
                  </span>
                )}
                {record.status === 'running' && (
                  <span style={{ fontSize: 9, color: 'var(--color-info)', fontWeight: 600 }}>
                    {formatDuration(getElapsedSeconds(record.started_at))}
                  </span>
                )}
                <span style={{
                  fontSize: 9, padding: '1px 6px', borderRadius: 8,
                  backgroundColor: record.status === 'success' ? 'var(--color-success)' : record.status === 'failed' ? 'var(--color-error)' : 'var(--color-info)',
                  color: '#fff', fontWeight: 600,
                }}>
                  {record.status === 'success' ? '✓' : record.status === 'failed' ? '✗' : '...'}
                </span>
                {isExpanded ? <UpOutlined style={{ fontSize: 9, color: 'var(--color-text-tertiary)' }} /> : <DownOutlined style={{ fontSize: 9, color: 'var(--color-text-tertiary)' }} />}
              </div>
            </div>
            {/* Expanded detail */}
            {isExpanded && (
              <div style={{
                marginTop: 4, padding: '8px 10px',
                background: 'var(--color-bg-elevated)', borderRadius: 6,
                border: '1px solid var(--color-border-light)',
              }}>
                {record.result && (
                  <div className={`history-result ${record.status === 'success' ? 'history-result-success' : 'history-result-failed'}`} style={{ marginBottom: 6 }}>
                    <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 4 }}>
                      <Button type="text" size="small" icon={<CopyOutlined />} onClick={async () => {
                        try { await navigator.clipboard.writeText(record.result || ''); messageApi.success('已复制'); }
                        catch { messageApi.error('复制失败'); }
                      }} />
                    </div>
                    <XMarkdown content={record.result} />
                  </div>
                )}
                {record.usage && (
                  <div style={{ fontSize: 10, color: 'var(--color-text-tertiary)', marginBottom: 4, display: 'flex', gap: 8 }}>
                    <span>In: {record.usage.input_tokens.toLocaleString()}</span>
                    <span>Out: {record.usage.output_tokens.toLocaleString()}</span>
                    {record.usage.total_cost_usd !== null && (
                      <span style={{ color: 'var(--color-warning)', fontWeight: 600 }}>${record.usage.total_cost_usd.toFixed(6)}</span>
                    )}
                  </div>
                )}
                <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>
                  {hasLogsStatic(record) && (
                    <Button size="small" icon={<FileTextOutlined />} onClick={() => onExport(record)}>导出</Button>
                  )}
                  {record.status === 'running' && (
                    <Popconfirm title="确定停止？" okText="停止" cancelText="取消" onConfirm={() => onStop(record.id)}>
                      <Button type="primary" danger size="small" icon={<StopOutlined />}>停止</Button>
                    </Popconfirm>
                  )}
                </div>
                {!isRunning && logs.length === 0 ? (
                  <ContinuationLogsLoader record={record} viewMode={viewMode} onRefresh={onRefresh} onViewModeChange={onViewModeChange} />
                ) : (
                  <ContinuationLogView
                    logs={logs}
                    isRunning={isRunning}
                    viewMode={viewMode}
                    onRefresh={() => onRefresh(record.id)}
                    onViewModeChange={onViewModeChange}
                  />
                )}
              </div>
            )}
            {/* Continue button on last continuation */}
            {isLast && record.status !== 'running' && supportsResume(record) && (
              <div style={{ marginTop: 6, display: 'flex', justifyContent: 'flex-end' }}>
                <Button type="primary" size="small" icon={<MessageOutlined />} onClick={() => onOpenResume(record)}>继续对话</Button>
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

/** Shared log rendering for narrow mode cards - as a proper component */
function NarrowLogView({ record, isRunning, displayLogs, liveLogs, viewMode, onRefresh, onViewModeChange }: {
  record: ExecutionRecord;
  isRunning: boolean;
  displayLogs: LogEntry[];
  liveLogs: LogEntry[] | null;
  viewMode: 'log' | 'chat';
  onRefresh: (id: number) => Promise<void>;
  onViewModeChange: (mode: 'log' | 'chat') => void;
}) {
  const defaultOpen = isRunning || viewMode === 'chat';
  const [isExpanded, setIsExpanded] = useState(defaultOpen);
  if (!isRunning && displayLogs.length === 0) return null;
  const title = viewMode === 'chat'
    ? `对话视图 (${displayLogs.length} 条)${isRunning && liveLogs && liveLogs.length > 0 ? ' · 实时' : ''}`
    : `查看日志 (${displayLogs.length} 条)${isRunning && liveLogs && liveLogs.length > 0 ? ' · 实时' : ''}`;
  return (
    <details style={{ marginTop: 8 }} open={isExpanded} onToggle={(e) => setIsExpanded((e.target as HTMLDetailsElement).open)}>
      <summary style={{ cursor: 'pointer', color: 'var(--color-primary)', fontSize: 12, fontWeight: 600, display: 'flex', alignItems: 'center', gap: 8 }}>
        <span>{title}</span>
        <LogViewHeader
          title=""
          viewMode={viewMode}
          onViewModeChange={onViewModeChange}
          onRefresh={() => onRefresh(record.id)}
        />
      </summary>
      {viewMode === 'chat' ? (
        <div style={{ maxHeight: 400, overflow: 'auto' }}>
          <ChatView logs={displayLogs as LogEntry[]} isRunning={isRunning} />
        </div>
      ) : (
        <div style={{
          background: 'var(--log-bg)', color: 'var(--log-text)', padding: 8, borderRadius: 8,
          fontFamily: 'var(--font-mono)', fontSize: 11, maxHeight: 250, overflow: 'auto',
        }}>
          {displayLogs.length === 0 ? (
            <div style={{ color: 'var(--log-text-muted)' }}>等待输出...</div>
          ) : (
            displayLogs.map((log, idx) => (
              <div key={idx} style={{ marginBottom: 4, display: 'flex', gap: 8 }}>
                <span style={{ color: 'var(--log-text-muted)', flexShrink: 0 }}>{formatLogTime(log.timestamp || '')}</span>
                <span style={{ color: logTypeColors[log.type || ''] || 'var(--log-text)' }}>
                  [{logTypeLabels[log.type || ''] || log.type}]
                </span>
                <span>{log.content}</span>
              </div>
            ))
          )}
        </div>
      )}
    </details>
  );
}

const logTypeColors: Record<string, string> = {
  info: '#60a5fa',
  text: '#4ade80',
  tool: '#fbbf24',
  tool_use: '#fbbf24',
  tool_call: '#fbbf24',
  tool_result: '#fbbf24',
  step_start: '#c084fc',
  step_finish: '#2dd4bf',
  stdout: '#cbd5e1',
  stderr: '#94a3b8',
  error: '#ef4444',
  system: '#94a3b8',
  assistant: '#a78bfa',
  user: '#22d3ee',
  result: '#4ade80',
  thinking: '#fb923c',
  tokens: '#94a3b8',
};

const logTypeLabels: Record<string, string> = {
  info: 'INFO',
  text: 'TEXT',
  tool: 'TOOL',
  tool_use: 'TOOL',
  tool_call: 'TOOL',
  tool_result: 'RESULT',
  step_start: 'START',
  step_finish: 'END',
  stdout: 'OUT',
  stderr: 'LOG',
  error: 'ERROR',
  system: 'SYS',
  assistant: 'ASST',
  user: 'USER',
  result: 'RESULT',
  thinking: 'THINK',
  tokens: 'INFO',
};

/**
 * 格式化时间戳为短时间格式 (HH:mm:ss)
 */
function formatLogTime(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleTimeString('zh-CN', {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    });
  } catch {
    return iso;
  }
}
