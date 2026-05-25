import { useState, useEffect } from 'react';
import { Button, Popconfirm, Tag, Tooltip } from 'antd';
import { MessageOutlined, FileTextOutlined, StopOutlined, CopyOutlined } from '@ant-design/icons';
import { ExecutorBadge } from '../ExecutorBadge';
import { XMarkdown } from '@ant-design/x-markdown';
import { supportsResume } from '../../types';
import { formatLocalDateTime, formatDuration } from '../../utils/datetime';
import * as db from '../../utils/database';
import { getElapsedSeconds, hasLogsStatic } from './helpers';
import { NarrowLogView } from './NarrowLogView';
import type { ExecutionRecord, ExecutionStats, LogEntry } from '../../types';

/** Narrow mode: single history card */
export function NarrowHistoryCard({ record, viewMode, onOpenResume, onExport, onStop, onRefresh, getRunningTask, resolveStats, parseLogs, messageApi, onViewModeChange }: {
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

  // 懒加载日志
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
