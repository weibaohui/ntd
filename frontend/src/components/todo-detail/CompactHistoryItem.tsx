import { useState, useEffect } from 'react';
import { Tag } from 'antd';
import { MessageOutlined, FileTextOutlined } from '@ant-design/icons';
import { ExecutorBadge } from '@/components/ExecutorBadge';
import { supportsResume } from '@/types';
import { formatLocalDateTime, formatDurationSec } from '@/utils/datetime';
import { getElapsedSeconds, hasLogsStatic } from './helpers';
import type { ExecutionRecord } from '@/types';

/** 紧凑历史列表项的内容（不含外层容器样式） */
export function CompactHistoryItem({ record, onOpenResume, onExport }: {
  record: ExecutionRecord;
  onOpenResume: (r: ExecutionRecord) => void;
  onExport: (r: ExecutionRecord) => void;
}) {
  const isRunning = record.status === 'running';
  const [elapsedSec, setElapsedSec] = useState(isRunning ? getElapsedSeconds(record.started_at) : 0);

  useEffect(() => {
    if (!isRunning) return;
    const tick = () => setElapsedSec(getElapsedSeconds(record.started_at));
    tick();
    const timer = setInterval(tick, 1000);
    return () => clearInterval(timer);
  }, [isRunning, record.started_at]);

  return (
    <>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 4 }}>
        <span style={{ fontSize: 12, color: 'var(--color-text-tertiary)' }}>
          {formatLocalDateTime(record.started_at)}
        </span>
        <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
          {record.status !== 'running' && supportsResume(record) && (
            <MessageOutlined
              style={{ fontSize: 12, color: 'var(--color-primary)', cursor: 'pointer' }}
              title="继续对话"
              onClick={(e) => { e.stopPropagation(); onOpenResume(record); }}
            />
          )}
          {hasLogsStatic(record) && (
            <FileTextOutlined
              style={{ fontSize: 12, color: 'var(--color-text-tertiary)', cursor: 'pointer' }}
              title="导出为YAML"
              onClick={(e) => { e.stopPropagation(); onExport(record); }}
            />
          )}
          <span style={{
            fontSize: 10,
            padding: '2px 8px',
            borderRadius: 10,
            backgroundColor: record.status === 'success' ? 'var(--color-success)' : record.status === 'failed' ? 'var(--color-error)' : 'var(--color-info)',
            color: '#fff',
            fontWeight: 600,
          }}>
            {record.status === 'success' ? '成功' : record.status === 'failed' ? '失败' : '进行中'}
          </span>
        </div>
      </div>
      <div style={{ display: 'flex', gap: 6, alignItems: 'center', flexWrap: 'wrap' }}>
        {record.executor && <ExecutorBadge executor={record.executor} />}
        {record.model && <Tag color="#3b82f6" style={{ fontSize: 10, padding: '0 6px', lineHeight: '18px' }}>{record.model}</Tag>}
        <Tag color={record.trigger_type === 'cron' ? '#8b5cf6' : record.trigger_type.startsWith('hook:') ? '#a855f7' : '#6b7280'} style={record.trigger_type.startsWith('hook:') ? { fontSize: 10, padding: '0 6px', lineHeight: '18px', border: '1px solid #a855f7' } : { fontSize: 10, padding: '0 6px', lineHeight: '18px' }}>
          {record.trigger_type === 'cron' ? 'Cron' : record.trigger_type.startsWith('hook:') ? 'Hook' : '手动'}
        </Tag>
        {record.usage?.duration_ms && (
          <span style={{ fontSize: 10, color: 'var(--color-success)', fontWeight: 600 }}>
            {formatDurationSec(record.usage.duration_ms / 1000)}
          </span>
        )}
        {isRunning && elapsedSec > 0 && (
          <span style={{ fontSize: 10, color: 'var(--color-info)', fontWeight: 600 }}>
            {formatDurationSec(elapsedSec)}
          </span>
        )}
        {record.execution_stats && (
          <span style={{ fontSize: 10, color: 'var(--color-text-tertiary)' }}>
            🔧{record.execution_stats.tool_calls} 💬{record.execution_stats.conversation_turns}
          </span>
        )}
      </div>
    </>
  );
}
