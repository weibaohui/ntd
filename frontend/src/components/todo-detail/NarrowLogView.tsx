import { useState, useEffect } from 'react';
import { ChatView } from '@/components/ChatView';
import { LogViewHeader } from './LogViewHeader';
import { formatLogTime } from './helpers';
import { LOG_TYPE_COLORS, LOG_TYPE_LABELS } from '@/constants';
import type { LogEntry, ExecutionRecord } from '@/types';
import { CommandPanel } from '@/components/CommandPanel';

/**
 * 窄屏模式卡片内的日志渲染组件。
 *
 * 三种 viewMode：
 * - 'log'：原始日志列表
 * - 'chat'：对话视图（ChatView）
 * - 'command'：命令视图（CommandPanel，从 logs 提取命令并按执行器协议展示）
 *
 * 视图模式与桌面端 RecordDetailView、续轮组件 ContinuationLogView/ContinuationLogsLoader 保持一致。
 */
export function NarrowLogView({ record, isRunning, displayLogs, liveLogs, viewMode, onRefresh, onViewModeChange }: {
  record: ExecutionRecord;
  isRunning: boolean;
  displayLogs: LogEntry[];
  liveLogs: LogEntry[] | null;
  viewMode: 'log' | 'chat' | 'command';
  onRefresh: (id: number) => Promise<void>;
  onViewModeChange: (mode: 'log' | 'chat' | 'command') => void;
}) {
  // 用户主动切到「对话/命令」时直接展开，否则只对运行中的记录展开（更符合直觉）。
  const defaultOpen = isRunning || viewMode === 'chat' || viewMode === 'command';
  const [isExpanded, setIsExpanded] = useState(defaultOpen);
  // PR #657 复查 C1 修复：useState 初始值只读一次，viewMode 后续变化不会触发展开。
  // 显式同步「切到 chat/command 必展开」这条约束；用户后续手动 collapse 仍可生效。
  useEffect(() => {
    if (viewMode === 'chat' || viewMode === 'command') {
      setIsExpanded(true);
    }
  }, [viewMode]);
  // 抽 liveTag：避免在三个分支里复制同一表达式；新增视图模式只需扩 titleMap。
  const liveTag = isRunning && liveLogs && liveLogs.length > 0 ? ' · 实时' : '';
  const titleMap = {
    log: `查看日志 (${displayLogs.length} 条)`,
    chat: `对话视图 (${displayLogs.length} 条)`,
    command: `命令视图 (${displayLogs.length} 条)`,
  } as const;
  const title = `${titleMap[viewMode]}${liveTag}`;
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
      ) : viewMode === 'command' ? (
        <div style={{ maxHeight: 400, overflow: 'auto' }}>
          <CommandPanel logs={displayLogs} executor={record.executor} />
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
