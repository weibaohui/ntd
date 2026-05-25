import { useEffect, useState, useMemo, useRef, useCallback } from 'react';
import { useApp } from '../hooks/useApp';
import { useIsMobile } from '../hooks/useIsMobile';
import { Button, Empty, App, Popconfirm, Tag, Badge, Pagination, Segmented, Modal, Input, Tooltip, Select } from 'antd';
import { PlayCircleOutlined, EditOutlined, DeleteOutlined, CheckCircleOutlined, ReloadOutlined, CopyOutlined, ArrowLeftOutlined, StopOutlined, UnorderedListOutlined, MessageOutlined, FileTextOutlined, LinkOutlined, LoadingOutlined, ThunderboltOutlined } from '@ant-design/icons';
import { StatusPicker } from './StatusPicker';
import { TodoDrawer } from './TodoDrawer';
import { ChatView } from './ChatView';
import { parseLogsToMessages } from './ChatView';
import * as db from '../utils/database';
import { formatLocalDateTime, formatDuration } from '../utils/datetime';
import { conversationToYaml } from '../utils/markdown';
import { getExecutorOption, supportsResume } from '../types';
import { ExecutorBadge } from './ExecutorBadge';
import XMarkdown from '@ant-design/x-markdown';
import type { ExecutionSummary, ExecutionRecord, LogEntry } from '../types';
import { RefreshBtn } from './todo-detail/LogViewHeader';
import { getElapsedSeconds, groupBySession, hasLogsStatic, formatLogTime, logTypeColors, logTypeLabels } from './todo-detail/helpers';
import { PromptDisplay } from './todo-detail/PromptDisplay';
import { InlineTokenStats } from './todo-detail/InlineTokenStats';
import { ProgressWidget } from './todo-detail/ProgressWidget';
import { CompactHistoryItem } from './todo-detail/CompactHistoryItem';
import { NarrowHistoryCard } from './todo-detail/NarrowHistoryCard';
import { ChainGroupCard } from './todo-detail/ChainGroupCard';




/** 任务详情面板，包含执行、编辑、历史记录等功能 */
export function TodoDetail({ onBack }: { onBack?: () => void }) {
  const { state, dispatch } = useApp();
  const { message } = App.useApp();
  const { todos, selectedTodoId, executionRecords, runningTasks } = state;
  const isMobile = useIsMobile();
  const isWide = !useIsMobile(1440);
  const [selectedHistoryRecordId, setSelectedHistoryRecordId] = useState<number | null>(null);
  const [viewMode, setViewMode] = useState<'log' | 'chat'>('log');
  const selectedTodo = todos.find(t => t.id === selectedTodoId);

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

  const [, setTick] = useState(0);
  useEffect(() => {
    if (!isExecuting) return;
    const interval = setInterval(() => {
      setTick(t => t + 1);
    }, 1000);
    return () => clearInterval(interval);
  }, [isExecuting]);

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

  // Note: historyLimit is intentionally kept in deps — when user changes page size via pagination,
  // we want to refetch with the new limit. The setter (setHistoryLimit) is NOT called here to avoid
  // triggering extra renders; the page size comes from the pagination component's local state.
  const cancelledRef = useRef(false);
  useEffect(() => {
    cancelledRef.current = false;
    if (selectedTodoId) {
      setHistoryPage(1);

      const statusFilter = historyStatusFilter === 'all' ? undefined : historyStatusFilter;
      db.getExecutionRecords(selectedTodoId, 1, historyLimit, statusFilter).then(pageData => {
        if (cancelledRef.current) return;
        dispatch({
          type: 'SET_EXECUTION_RECORDS',
          payload: { todoId: selectedTodoId, records: pageData.records }
        });
        setHistoryPage(pageData.page);
        setHistoryTotal(pageData.total);
      }).catch(() => {});

      db.getExecutionSummary(selectedTodoId).then(sum => {
        if (!cancelledRef.current) setSummary(sum);
      }).catch(() => {});
    } else {
      setSummary(null);
    }
    return () => { cancelledRef.current = true; };
  }, [selectedTodoId, historyLimit, historyStatusFilter]); // dispatch intentionally excluded - React guarantees useReducer dispatch is stable

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

  const handleStatusChange = useCallback(async (newStatus: string) => {
    if (!selectedTodo) return;
    try {
      const updated = await db.updateTodo(selectedTodo.id, selectedTodo.title, selectedTodo.prompt || '', newStatus);
      dispatch({ type: 'UPDATE_TODO', payload: updated });
      message.success('状态已更新');
    } catch {
      // ignore: interceptor already shows error
    }
  }, [selectedTodo, dispatch]);

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



