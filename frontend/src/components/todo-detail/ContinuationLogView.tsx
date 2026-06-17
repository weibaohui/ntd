import { useState, useEffect } from 'react';
import { ChatView } from '@/components/ChatView';
import { CommandPanel } from '@/components/CommandPanel';
import { LogViewHeader } from './LogViewHeader';
import { formatLogTime } from './helpers';
import { LOG_TYPE_COLORS, LOG_TYPE_LABELS } from '@/constants';
import type { LogEntry, ExecutionRecord } from '@/types';

/**
 * 内联日志视图组件（用于 ChainGroupCard 内部）。
 *
 * 三种 viewMode 与 NarrowLogView 对齐：
 * - 'log'：原始日志列表
 * - 'chat'：对话视图
 * - 'command'：命令视图（CommandPanel，从 logs 中提取并按执行器协议展示）
 */
export function ContinuationLogView({ record, logs, isRunning, viewMode, onRefresh, onViewModeChange }: {
  record: ExecutionRecord;
  logs: LogEntry[];
  isRunning: boolean;
  viewMode: 'log' | 'chat' | 'command';
  onRefresh: () => void;
  onViewModeChange: (mode: 'log' | 'chat' | 'command') => void;
}) {
  // 用户主动切到「命令」时默认展开，让命令面板直接可见。
  const defaultOpen = isRunning || viewMode === 'chat' || viewMode === 'command';
  const [isExpanded, setIsExpanded] = useState(defaultOpen);
  // PR #657 复查 C1 修复：useState 初始值只读一次，viewMode 后续变化不会触发展开。
  // 显式同步「切到 chat/command 必展开」这条约束；用户后续手动 collapse 仍可生效。
  useEffect(() => {
    if (viewMode === 'chat' || viewMode === 'command') {
      setIsExpanded(true);
    }
  }, [viewMode]);
  // 抽 titleMap 替代三元嵌套：新增视图模式只需改这张表，不重写条件。
  const titleMap = { log: `日志 (${logs.length})`, chat: `对话 (${logs.length})`, command: `命令 (${logs.length})` } as const;
  const title = titleMap[viewMode];
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
      ) : viewMode === 'command' ? (
        <div style={{ maxHeight: 300, overflow: 'auto' }}>
          <CommandPanel logs={logs} executor={record.executor} />
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
                <span style={{ color: LOG_TYPE_COLORS[log.type || ''] || 'var(--log-text)' }}>
                  [{LOG_TYPE_LABELS[log.type || ''] || log.type}]
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
