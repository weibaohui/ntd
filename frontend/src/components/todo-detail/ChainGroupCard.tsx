import { useState, useEffect } from 'react';
import { Button, Popconfirm, Tag, Tooltip } from 'antd';
import { MessageOutlined, FileTextOutlined, StopOutlined, CopyOutlined, LinkOutlined, UpOutlined, DownOutlined } from '@ant-design/icons';
import { XMarkdown } from '@ant-design/x-markdown';
import { ExecutorBadge } from '@/components/ExecutorBadge';
import { supportsResume } from '@/types';
import { formatLocalDateTime, formatDurationSec } from '@/utils/datetime';
import * as db from '@/utils/database';
import { getElapsedSeconds, hasLogsStatic } from './helpers';
import { NarrowLogView } from './NarrowLogView';
import { ContinuationLogView } from './ContinuationLogView';
import { ContinuationLogsLoader } from './ContinuationLogsLoader';
import { getHookTriggerLabel } from '@/utils/database/hooks';
import type { SessionGroup } from './helpers';
import type { ExecutionRecord, ExecutionStats, LogEntry } from '@/types';
import { copyToClipboard } from '@/utils/clipboard';

function ChainGroupCard({ group, onOpenResume, onExport, onStop, messageApi, viewMode, parseLogs, onRefresh, resolveStats, onViewModeChange }: {
  group: SessionGroup;
  onOpenResume: (r: ExecutionRecord) => void;
  onExport: (r: ExecutionRecord) => void;
  onStop: (id: number) => Promise<void>;
  messageApi: any;
  viewMode: 'log' | 'chat' | 'command';
  parseLogs: (r: ExecutionRecord) => LogEntry[];
  onRefresh: (id: number) => Promise<void>;
  resolveStats: (r: ExecutionRecord, running: boolean) => ExecutionStats | null | undefined;
  onViewModeChange: (mode: 'log' | 'chat' | 'command') => void;
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
            <Tag color={mainRecord.trigger_type === 'cron' ? '#8b5cf6' : mainRecord.trigger_type.startsWith('hook:') ? '#a855f7' : '#6b7280'} style={mainRecord.trigger_type.startsWith('hook:') ? { fontSize: 10, border: '1px solid #a855f7' } : { fontSize: 10 }}>
              {mainRecord.trigger_type === 'cron' ? 'Cron' : mainRecord.trigger_type.startsWith('hook:') ? 'Hook' : '手动'}
            </Tag>
            {(() => {
              const label = getHookTriggerLabel(mainRecord.trigger_type);
              if (!label || mainRecord.source_todo_id == null) return null;
              return (
                <Tag color="purple" icon={<LinkOutlined />} style={{ fontSize: 10 }}>
                  被 #{mainRecord.source_todo_id} {mainRecord.source_todo_title ?? '?'} 的「{label}」hook 触发
                </Tag>
              );
            })()}
            {mainRecord.status !== 'running' && mainRecord.usage?.duration_ms && (
              <span style={{ fontSize: 11, color: 'var(--color-success)', fontWeight: 600 }}>
                {formatDurationSec(mainRecord.usage.duration_ms / 1000)}
              </span>
            )}
            {mainRecord.status === 'running' && (
              <span style={{ fontSize: 11, color: 'var(--color-info)', fontWeight: 600 }}>
                {formatDurationSec(getElapsedSeconds(mainRecord.started_at))}
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
        {/* 点击命令文本即可复制，不需要额外的复制按钮 */}
        {/* 使用 copyToClipboard 统一处理，兼容 HTTP 环境（通过 fallback 到 execCommand） */}
        {mainRecord.command && (
          <Tooltip title="点击复制命令">
            <div
              onClick={async () => {
                try {
                  // 调用统一的复制工具（内置 fallback，兼容 HTTP 环境）
                  const ok = await copyToClipboard(mainRecord.command || '');
                  // 根据返回结果提示用户
                  if (ok) {
                    messageApi.success('已复制');
                  } else {
                    messageApi.error('复制失败');
                  }
                } catch {
                  messageApi.error('复制失败');
                }
              }}
              style={{ fontSize: 11, color: 'var(--color-text-quaternary)', marginBottom: 8, fontFamily: 'var(--font-mono)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', cursor: 'pointer' }}
            >
              {mainRecord.command}
            </div>
          </Tooltip>
        )}
        {mainRecord.result && (
          <div className={`history-result ${mainRecord.status === 'success' ? 'history-result-success' : 'history-result-failed'}`}>
            <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 4 }}>
              {/* 复制结论文本：使用 copyToClipboard 统一处理，兼容 HTTP 环境 */}
              <Button type="text" size="small" icon={<CopyOutlined />} onClick={async () => {
                try {
                  // 调用统一的复制工具（内置 fallback，兼容 HTTP 环境）
                  const ok = await copyToClipboard(mainRecord.result || '');
                  // 根据返回结果提示用户
                  if (ok) {
                    messageApi.success('已复制到剪贴板');
                  } else {
                    messageApi.error('复制失败');
                  }
                } catch {
                  messageApi.error('复制失败');
                }
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
                    {formatDurationSec(record.usage.duration_ms / 1000)}
                  </span>
                )}
                {record.status === 'running' && (
                  <span style={{ fontSize: 9, color: 'var(--color-info)', fontWeight: 600 }}>
                    {formatDurationSec(getElapsedSeconds(record.started_at))}
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
                      {/* 复制续轮结论文本：使用 copyToClipboard 统一处理，兼容 HTTP 环境 */}
                      <Button type="text" size="small" icon={<CopyOutlined />} onClick={async () => {
                        try {
                          // 调用统一的复制工具（内置 fallback，兼容 HTTP 环境）
                          const ok = await copyToClipboard(record.result || '');
                          // 根据返回结果提示用户
                          if (ok) {
                            messageApi.success('已复制');
                          } else {
                            messageApi.error('复制失败');
                          }
                        } catch {
                          messageApi.error('复制失败');
                        }
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

export { ChainGroupCard };
