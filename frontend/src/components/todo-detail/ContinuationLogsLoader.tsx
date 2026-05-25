import { useState, useEffect } from 'react';
import { ChatView } from '../ChatView';
import { LogViewHeader } from './LogViewHeader';
import { formatLogTime } from './helpers';
import * as db from '../../utils/database';
import type { LogEntry, ExecutionRecord } from '../../types';

/** Lazy-load logs for a continuation record in ChainGroupCard */
export function ContinuationLogsLoader({ record, viewMode, onRefresh, onViewModeChange }: {
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
