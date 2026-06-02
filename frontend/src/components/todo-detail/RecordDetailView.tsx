import { Button, Tag, Empty, Segmented, Popconfirm, Tooltip, Pagination, message } from 'antd';
import { MessageOutlined, FileTextOutlined, StopOutlined, CopyOutlined, UnorderedListOutlined, LinkOutlined, LoadingOutlined } from '@ant-design/icons';
import XMarkdown from '@ant-design/x-markdown';
import { ExecutorBadge } from '../ExecutorBadge';
import { ChatView } from '../ChatView';
import { RefreshBtn } from './LogViewHeader';
import { formatLocalDateTime, formatDuration } from '../../utils/datetime';
import { getElapsedSeconds, formatLogTime, logTypeColors, logTypeLabels } from './helpers';
import type { SessionGroup } from './helpers';
import { supportsResume } from '../../types';
import type { ExecutionRecord, LogEntry } from '../../types';
import { getHookTriggerLabel } from '../../utils/database/hooks';

export function RecordDetailView({
  isLoadingDetail, record, sessionGroups,
  onSelectRecord, viewMode, onViewModeChange,
  onOpenResume, onExportMarkdown, onStop, onRefreshSingle,
  paginatedLogs, logsTotal, logsPage, logsPerPage, onLoadLogs, isLoadingLogs,
  getRunningTaskForRecord, resolveExecutionStats,
}: {
  isLoadingDetail: boolean;
  record: ExecutionRecord | null;
  sessionGroups: SessionGroup[];
  onSelectRecord: (id: number) => void;
  viewMode: 'log' | 'chat';
  onViewModeChange: (mode: 'log' | 'chat') => void;
  onOpenResume: (record: ExecutionRecord) => void;
  onExportMarkdown: (record: ExecutionRecord) => Promise<void>;
  onStop: (recordId: number) => Promise<void>;
  onRefreshSingle: (recordId: number) => Promise<void>;
  paginatedLogs: LogEntry[];
  logsTotal: number;
  logsPage: number;
  logsPerPage: number;
  onLoadLogs: (recordId: number, page: number) => Promise<void>;
  isLoadingLogs: boolean;
  getRunningTaskForRecord: (record: ExecutionRecord) => any;
  resolveExecutionStats: (record: ExecutionRecord, isRunning: boolean) => any;
}) {
  if (isLoadingDetail) {
    return (
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%', gap: 8, color: 'var(--color-text-secondary)' }}>
        <LoadingOutlined style={{ fontSize: 20, color: 'var(--color-primary)' }} />
        <span>加载执行详情...</span>
      </div>
    );
  }

  if (!record) {
    return (
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%' }}>
        <Empty description="选择一条执行记录查看详情" image={Empty.PRESENTED_IMAGE_SIMPLE} />
      </div>
    );
  }

  const isRunning = record.status === 'running';
  const runningTask = isRunning ? getRunningTaskForRecord(record) : null;
  const liveLogs = runningTask ? runningTask.logs : null;
  const displayLogs = liveLogs && liveLogs.length > 0 ? liveLogs : paginatedLogs;

  return (
    <>
      {(() => {
        const group = sessionGroups.find(g => g.records.some(r => r.id === record!.id));
        if (!group || group.records.length <= 1 || !group.records[0].session_id) return null;
        const idx = group.records.findIndex(r => r.id === record!.id);
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
              onClick={() => onSelectRecord(group.records[0].id)}
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
          {(() => {
            const label = getHookTriggerLabel(record.trigger_type);
            if (!label || record.source_todo_id == null) return null;
            return (
              <Tag color="purple" icon={<LinkOutlined />} style={{ margin: 0 }}>
                被 #{record.source_todo_id} {record.source_todo_title ?? '?'} 的「{label}」hook 触发
              </Tag>
            );
          })()}
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
            <Button type="primary" size="small" icon={<MessageOutlined />} onClick={() => onOpenResume(record)}>继续对话</Button>
          )}
          {record.status !== 'running' && !!record.finished_at && (
            <Button size="small" icon={<FileTextOutlined />} onClick={() => onExportMarkdown(record)}>导出YAML</Button>
          )}
          {record.status === 'running' && (
            <Popconfirm
              title="确定强制停止该任务？"
              okText="停止"
              cancelText="取消"
              onConfirm={async () => { await onStop(record.id); }}
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
                  <RefreshBtn onClick={() => onRefreshSingle(record.id)} />
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
                  onRefreshSingle(record.id);
                  onLoadLogs(record.id, logsPage);
                }} />
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
                displayLogs.map((log: LogEntry, idx: number) => (
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
                onChange={(page) => onLoadLogs(record.id, page)}
                size="small"
                style={{ marginTop: 8, textAlign: 'center' }}
              />
            )}
          </div>
        );
      })()}
    </>
  );
}
