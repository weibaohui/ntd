import { useEffect, useState, useMemo, useCallback, useRef } from 'react';
import { useApp } from '@/hooks/useApp';
import { useIsMobile } from '@/hooks/useIsMobile';
import { useExecutionHistory } from '@/hooks/useExecutionHistory';
import { Button, Empty, App, Pagination, Modal, Input, Select } from 'antd';
import { CheckCircleOutlined, ReloadOutlined, ThunderboltOutlined } from '@ant-design/icons';
import { TodoDrawer } from './TodoDrawer';
import { parseLogsToMessages } from './ChatView';
import { BREAKPOINTS, EXPORT } from '@/constants';
import * as db from '@/utils/database';
import { conversationToYaml } from '@/utils/markdown';
import { getExecutorOption } from '@/types';
import type { ExecutionRecord, LogEntry } from '@/types';
import { groupBySession } from './todo-detail/helpers';
import { NarrowHistoryCard } from './todo-detail/NarrowHistoryCard';
import { ChainGroupCard } from './todo-detail/ChainGroupCard';
import { DetailHeader } from './todo-detail/DetailHeader';
import { ForumPostList } from './todo-detail/ForumPostList';

interface TodoDetailProps {
  hideTitleRow?: boolean;
  onOpenPost?: (todoId: number, recordId: number) => void;
}

export function TodoDetail({ hideTitleRow = false, onOpenPost }: TodoDetailProps) {
  const { state, dispatch } = useApp();
  const { message } = App.useApp();
  const { todos, selectedTodoId, executionRecords, runningTasks } = state;
  const isWide = !useIsMobile(BREAKPOINTS.wide);
  const selectedTodo = todos.find(t => t.id === selectedTodoId);

  const [todoDrawerOpen, setTodoDrawerOpen] = useState(false);
  // viewMode 仅窄屏模式使用（PostDetailDrawer 内部自己管理）
  const [viewMode, setViewMode] = useState<'log' | 'chat' | 'command'>('log');

  // 使用 useExecutionHistory hook 获取执行历史相关的状态和操作
  const {
    selectedHistoryRecordId,
    setSelectedHistoryRecordId,
    records,
    historyPage,
    historyLimit,
    historyTotal,
    historyStatusFilter,
    setHistoryStatusFilter,
    summary,
    selectedHistoryRecord,
    loadExecutionRecords,
    loadLogs,
    refreshSingleRecord,
    handleHistoryPageChange,
  } = useExecutionHistory({
    selectedTodoId,
    storeRecords: selectedTodoId ? executionRecords[selectedTodoId] : [],
    dispatch,
  });

  // Timer for live duration display of running records
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

  // 当执行结束时，刷新执行记录和日志
  const prevIsExecutingRef = useRef(isExecuting);
  useEffect(() => {
    const prev = prevIsExecutingRef.current;
    // 当 isExecuting 从 true 变为 false 时，表示执行刚结束
    if (prev && !isExecuting && selectedTodoId) {
      // 刷新执行记录列表（包含结论）
      loadExecutionRecords(historyPage, historyLimit);
      // 如果有选中的记录，刷新单条记录详情（包含 result）和日志
      if (selectedHistoryRecordId) {
        refreshSingleRecord(selectedHistoryRecordId);
        loadLogs(selectedHistoryRecordId, 1);
      }
    }
    prevIsExecutingRef.current = isExecuting;
  }, [isExecuting, selectedTodoId, selectedHistoryRecordId, historyPage, historyLimit, loadExecutionRecords, refreshSingleRecord, loadLogs]);

  const getRunningTaskForRecord = (record: ExecutionRecord) => {
    if (record.task_id) {
      return runningTasks[record.task_id] || null;
    }
    return Object.values(runningTasks).find(t => t.todoId === record.todo_id) || null;
  };

  const resolveExecutionStats = (record: ExecutionRecord, isRunning: boolean) => {
    if (isRunning) {
      const task = getRunningTaskForRecord(record);
      if (task?.executionStats) return task.executionStats;
    }
    return record.execution_stats;
  };

  useEffect(() => {
    if (!isWide || records.length === 0) return;
    if (selectedHistoryRecordId !== null && records.find(r => r.id === selectedHistoryRecordId)) return;
    setSelectedHistoryRecordId(records[0].id);
  }, [isWide, records, selectedHistoryRecordId]);

  const handleExecute = async () => {
    if (!selectedTodo) return;
    try {
      const result = await db.executeTodo(
        selectedTodo.id,
        selectedTodo.executor || undefined,
        undefined
      );
      message.success('任务已开始执行');
      // 获取新创建的执行记录并立即添加到状态中
      try {
        const newRecord = await db.getExecutionRecord(result.record_id);
        dispatch({
          type: 'ADD_EXECUTION_RECORD',
          payload: { todoId: selectedTodo.id, record: newRecord }
        });
        // 选中新记录
        setSelectedHistoryRecordId(result.record_id);
      } catch {
        // 获取新记录失败时回退到刷新列表
        await loadExecutionRecords(1, historyLimit);
      }
    } catch (error) {
      message.error('执行失败: ' + (error instanceof Error ? error.message : String(error)));
    }
  };

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
      const result = await db.executeTodo(
        selectedTodo.id,
        selectedTodo.executor || undefined,
        params
      );
      message.success('任务已开始执行');
      setExecuteWithArgsModalOpen(false);
      setExecuteArgs('');
      // 获取新创建的执行记录并立即添加到状态中
      try {
        const newRecord = await db.getExecutionRecord(result.record_id);
        dispatch({
          type: 'ADD_EXECUTION_RECORD',
          payload: { todoId: selectedTodo.id, record: newRecord }
        });
        // 选中新记录
        setSelectedHistoryRecordId(result.record_id);
      } catch {
        // 获取新记录失败时回退到刷新列表
        await loadExecutionRecords(1, historyLimit);
      }
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

  /**
   * 给一条执行结果评分或清除评分。
   * 后端会返回更新后的 record；刷新单条后 dispatcher 会同步本地缓存。
   */
  const handleRateExecution = async (recordId: number, rating: number | null) => {
    await db.rateExecutionRecord(recordId, rating);
    await refreshSingleRecord(recordId);
    message.success(rating == null ? '已清除评分' : `已评分 ${rating}`);
  };

  const sessionGroups = useMemo(() => groupBySession(records), [records]);

  const handleReply = async (record: ExecutionRecord, replyMessage: string) => {
    if (!replyMessage.trim()) return;
    try {
      await db.resumeExecutionRecord(record.id, replyMessage);
      message.success('回复成功，开始执行');
      await loadExecutionRecords(historyPage, historyLimit);
    } catch (error) {
      message.error('回复失败: ' + (error instanceof Error ? error.message : String(error)));
    }
  };

  // 窄屏模式兼容：NarrowHistoryCard / ChainGroupCard 仍期望 onOpenResume 回调。
  // 窄屏暂不做论坛式改造，保留简单 prompt 交互。
  const handleOpenResume = (record: ExecutionRecord) => {
    const input = prompt('输入要继续发送的消息：');
    if (input && input.trim()) {
      handleReply(record, input.trim());
    }
  };

  const parseRecordLogs = (_record: ExecutionRecord): LogEntry[] => {
    return [];
  };

  const handleExportMarkdown = async (record: ExecutionRecord) => {
    let logs: LogEntry[] = [];
    try {
      const result = await db.getExecutionLogs(record.id, 1, EXPORT.maxLogs);
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

  // 升级/降级已移除：环节与 Todo 合一，无需 promote 流程

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

  const currentTodoProgress = (() => {
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
      <DetailHeader
        selectedTodo={selectedTodo}
        executor={executor}
        isExecuting={isExecuting}
        summary={summary}
        currentTodoProgress={currentTodoProgress}
        records={records}
        onDelete={handleDelete}
        onTodoDrawerOpen={() => setTodoDrawerOpen(true)}
        onOpenExecuteWithArgs={handleOpenExecuteWithArgs}
        onExecute={handleExecute}
        onStatusChange={handleStatusChange}
        hideTitleRow={hideTitleRow}
      />

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
          <div style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden', minHeight: 0 }}>
            <ForumPostList
              sessionGroups={sessionGroups}
              selectedRecordId={selectedHistoryRecordId}
              onSelectRecord={(id) => {
                setSelectedHistoryRecordId(id);
                if (selectedTodoId && onOpenPost) {
                  onOpenPost(selectedTodoId, id);
                }
              }}
              historyTotal={historyTotal}
              historyLimit={historyLimit}
              historyPage={historyPage}
              onPageChange={handleHistoryPageChange}
            />
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
                    onRate={handleRateExecution}
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
                  onChange={handleHistoryPageChange}
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
          // 只刷新当前 workspace 桶：抽屉保存的 todo 必然属于该 workspace。
          const wid = state.selectedWorkspace;
          if (wid != null) {
            db.getAllTodos(wid).then(todos => {
              dispatch({ type: 'SET_TODOS_BY_WORKSPACE', workspaceId: wid, payload: todos });
            });
          }
          if (selectedTodoId) {
            loadExecutionRecords(1, historyLimit);
          }
        }}
      />

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
