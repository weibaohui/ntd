import { useState, useEffect } from 'react';
import { ChatView } from '@/components/ChatView';
import { CommandPanel } from '@/components/CommandPanel';
import { LogViewHeader } from './LogViewHeader';
import { formatLogTime } from './helpers';
import * as db from '@/utils/database';
import type { LogEntry, ExecutionRecord } from '@/types';

/**
 * 续轮记录懒加载日志视图。
 *
 * 与 ContinuationLogView 互斥使用：当前 chain group 内若已有 logs 用前者，
 * 否则用本组件按需拉取一次。三种 viewMode 与 NarrowLogView 对齐：
 * - 'log'：原始日志列表
 * - 'chat'：对话视图
 * - 'command'：命令视图（CommandPanel）
 */
export function ContinuationLogsLoader({
  record,
  logs: initialLogs,
  viewMode,
  onRefresh,
  onViewModeChange,
}: {
  // 允许 mount/测试场景直接传入 logs 跳过懒加载，避免在没有后端时整个组件返回 null。
  record: ExecutionRecord;
  logs?: LogEntry[];
  viewMode: 'log' | 'chat' | 'command';
  onRefresh: (id: number) => Promise<void>;
  onViewModeChange: (mode: 'log' | 'chat' | 'command') => void;
}) {
  const [logs, setLogs] = useState<LogEntry[] | null>(initialLogs ?? null);
  // 切到「对话/命令」视图时直接展开，避免用户多次点击。
  const [isExpanded, setIsExpanded] = useState(viewMode === 'chat' || viewMode === 'command');
  // PR #657 复查 C1 修复：useState 初始值只读一次，viewMode 后续变化不会触发展开。
  // 显式同步「切到 chat/command 必展开」这条约束；用户后续手动 collapse 仍可生效。
  useEffect(() => {
    if (viewMode === 'chat' || viewMode === 'command') {
      setIsExpanded(true);
    }
  }, [viewMode]);
  useEffect(() => {
    // 显式传入 logs 时直接跳过网络请求，让父组件（如 mount harness / 单测）完全控制数据。
    if (initialLogs !== undefined) return;
    db.getExecutionLogs(record.id, 1, 200)
      .then(r => setLogs(r.logs))
      .catch(() => setLogs([]));
    // 把 record.executor 加入 deps：执行器协议切换时让 CommandPanel 看到的是同一份最新 logs。
  }, [record.id, record.executor, initialLogs]);
  if (logs === null) return null;
  // PR #657 复查 C2 修复：去掉"懒加载 + 空 logs 就 return null"的旧分支。
  // 旧逻辑下，ChainGroupCard 在 logs.length === 0 && !isRunning 的场景渲染本组件，
  // 懒加载 fetch 完仍是空数组会让整个组件消失，命令面板永远渲染不出来。
  // 现在由 CommandPanel 自带的"未捕获到可提取的 Bash 命令"空态兜底。
  // 抽 titleMap 替代三元嵌套：新增视图模式只需改这张表。
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
          onRefresh={() => onRefresh(record.id)}
          fontSize={10}
        />
      </summary>
      {viewMode === 'chat' ? (
        <div style={{ maxHeight: 300, overflow: 'auto' }}>
          <ChatView logs={logs as LogEntry[]} isRunning={false} />
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
