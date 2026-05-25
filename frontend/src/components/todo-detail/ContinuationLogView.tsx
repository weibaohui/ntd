import { useState } from 'react';
import { ChatView } from '../ChatView';
import { LogViewHeader } from './LogViewHeader';
import { formatLogTime, logTypeColors, logTypeLabels } from './helpers';
import type { LogEntry } from '../../types';

/** 内联日志视图组件 (用于 ChainGroupCard 内部) */
export function ContinuationLogView({ logs, isRunning, viewMode, onRefresh, onViewModeChange }: {
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
