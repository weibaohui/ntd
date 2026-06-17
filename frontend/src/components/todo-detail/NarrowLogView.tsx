import { useState } from 'react';
import { ChatView } from '@/components/ChatView';
import { LogViewHeader } from './LogViewHeader';
import { formatLogTime } from './helpers';
import { LOG_TYPE_COLORS, LOG_TYPE_LABELS } from '@/constants';
import type { LogEntry, ExecutionRecord } from '@/types';

/** Shared log rendering for narrow mode cards - as a proper component */
export function NarrowLogView({ record, isRunning, displayLogs, liveLogs, viewMode, onRefresh, onViewModeChange }: {
  record: ExecutionRecord;
  isRunning: boolean;
  displayLogs: LogEntry[];
  liveLogs: LogEntry[] | null;
  viewMode: 'log' | 'chat' | 'command';
  onRefresh: (id: number) => Promise<void>;
  onViewModeChange: (mode: 'log' | 'chat' | 'command') => void;
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
                <span style={{ color: LOG_TYPE_COLORS[log.type || ''] || 'var(--log-text)' }}>
                  [{LOG_TYPE_LABELS[log.type || ''] || log.type}]
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
