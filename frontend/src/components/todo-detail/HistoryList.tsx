import { Pagination } from 'antd';
import { LinkOutlined, MessageOutlined } from '@ant-design/icons';
import { CompactHistoryItem } from './CompactHistoryItem';
import { formatLocalDateTime, formatDurationSec } from '@/utils/datetime';
import { getElapsedSeconds } from './helpers';
import type { SessionGroup } from './helpers';
import { supportsResume } from '@/types';
import type { ExecutionRecord } from '@/types';

export function HistoryList({
  sessionGroups, selectedHistoryRecordId, onSelectRecord,
  historyTotal, historyLimit, historyPage, onPageChange,
  onOpenResume, onExportMarkdown,
}: {
  sessionGroups: SessionGroup[];
  selectedHistoryRecordId: number | null;
  onSelectRecord: (id: number) => void;
  historyTotal: number;
  historyLimit: number;
  historyPage: number;
  onPageChange: (page: number, pageSize: number) => void;
  onOpenResume: (record: ExecutionRecord) => void;
  onExportMarkdown: (record: ExecutionRecord) => Promise<void>;
}) {
  return (
    <div style={{ width: 320, flexShrink: 0, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
      <div className="history-list-column">
        {sessionGroups.map(group => {
          const isSingle = group.records.length === 1 || !group.records[0].session_id;
          if (isSingle) {
            return group.records.map(record => {
              const isSelected = selectedHistoryRecordId === record.id;
              return (
                <div
                  key={record.id}
                  className={`history-item-compact${isSelected ? ' selected' : ''}${record.status === 'failed' ? ' failed' : record.status === 'running' ? ' running' : ''}`}
                  onClick={() => onSelectRecord(record.id)}
                >
                  <CompactHistoryItem record={record} onOpenResume={onOpenResume} onExport={onExportMarkdown} />
                </div>
              );
            });
          }
          const mainRecord = group.records[0];
          const continuations = group.records.slice(1);
          const mainSelected = selectedHistoryRecordId === mainRecord.id;
          return (
            <div key={group.sessionId} style={{ marginBottom: 6 }}>
              <div
                className={`history-item-compact${mainSelected ? ' selected' : ''}`}
                onClick={() => onSelectRecord(mainRecord.id)}
              >
                <CompactHistoryItem record={mainRecord} onOpenResume={onOpenResume} onExport={onExportMarkdown} />
              </div>
              {continuations.map((record, idx) => {
                const isSelected = selectedHistoryRecordId === record.id;
                const isLast = idx === continuations.length - 1;
                return (
                  <div
                    key={record.id}
                    onClick={() => onSelectRecord(record.id)}
                    style={{
                      marginLeft: 12,
                      padding: '6px 8px',
                      borderLeft: '2px solid var(--color-primary)',
                      borderBottom: '1px solid var(--color-border-light)',
                      cursor: 'pointer',
                      background: isSelected ? 'var(--color-primary-bg)' : 'var(--color-bg-elevated)',
                      transition: 'background 0.15s',
                      marginBottom: 1,
                    }}
                  >
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 2 }}>
                      <span style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 10, color: 'var(--color-primary)', fontWeight: 500 }}>
                        <LinkOutlined style={{ fontSize: 10 }} />
                        {record.resume_message ? (
                          <span style={{ color: 'var(--color-text-secondary)', fontWeight: 400 }}>{String(record.resume_message).length > 30 ? String(record.resume_message).substring(0, 30) + '...' : record.resume_message}</span>
                        ) : (
                          <span>继续对话</span>
                        )}
                      </span>
                      <span style={{
                        fontSize: 9, padding: '1px 6px', borderRadius: 8,
                        backgroundColor: record.status === 'success' ? 'var(--color-success)' : record.status === 'failed' ? 'var(--color-error)' : 'var(--color-info)',
                        color: '#fff', fontWeight: 600,
                      }}>
                        {record.status === 'success' ? '✓' : record.status === 'failed' ? '✗' : '...'}
                      </span>
                    </div>
                    <div style={{ display: 'flex', gap: 4, alignItems: 'center', flexWrap: 'wrap' }}>
                      <span style={{ fontSize: 9, color: 'var(--color-text-tertiary)' }}>
                        {formatLocalDateTime(record.started_at)}
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
                      {record.execution_stats && (
                        <span style={{ fontSize: 9, color: 'var(--color-text-tertiary)' }}>
                          🔧{record.execution_stats.tool_calls}
                        </span>
                      )}
                    </div>
                    {isLast && record.status !== 'running' && supportsResume(record) && (
                      <MessageOutlined
                        style={{ fontSize: 11, color: 'var(--color-primary)', cursor: 'pointer', marginTop: 3 }}
                        title="继续对话"
                        onClick={(e) => { e.stopPropagation(); onOpenResume(record); }}
                      />
                    )}
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>
      {historyTotal > historyLimit && (
        <div style={{ flexShrink: 0, display: 'flex', justifyContent: 'center', padding: '8px 0 0', borderTop: '1px solid var(--color-border-light)' }}>
          <Pagination
            current={historyPage}
            pageSize={historyLimit}
            total={historyTotal}
            onChange={onPageChange}
            size="small"
            showSizeChanger
            pageSizeOptions={[5, 10, 20, 50]}
          />
        </div>
      )}
    </div>
  );
}
