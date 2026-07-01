// 执行记录卡片组件。

import { useCallback, useMemo } from 'react';
import { Tag } from 'antd';
import type { ExecutionRecord } from '@/types';
import { ExecutorBadge } from '@/components/ExecutorBadge';
import { formatRelativeTime } from '@/utils/datetime';
import { formatDuration } from '@/utils/format';

interface ExecutionRecordCardProps {
  record: ExecutionRecord;
  todoTitle?: string;
  onSelectTodo?: (id: number) => void;
  onCardClick?: (record: ExecutionRecord) => void;
}

export function ExecutionRecordCard({
  record,
  todoTitle,
  onSelectTodo,
  onCardClick,
}: ExecutionRecordCardProps) {
  const duration = record.usage?.duration_ms || null;
  const cost = record.usage?.total_cost_usd;

  const handleTitleClick = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    onSelectTodo?.(record.todo_id);
  }, [record.todo_id, onSelectTodo]);

  const handleCardClick = useCallback(() => {
    onCardClick?.(record);
  }, [record, onCardClick]);

  const statusTag = useMemo(() => {
    if (record.status === 'running') return <Tag color="orange">运行中</Tag>;
    if (record.status === 'failed') return <Tag color="red">失败</Tag>;
    if (record.last_review_status === 'pending') return <Tag color="cyan">评审中</Tag>;
    if (record.last_review_status === 'success') return <Tag color="green">评审通过</Tag>;
    if (record.last_review_status === 'failed') return <Tag color="red">评审失败</Tag>;
    return <Tag color="green">成功</Tag>;
  }, [record.status, record.last_review_status]);

  return (
    <div className={`running-card execution-card status-${record.status}`} onClick={handleCardClick}>
      <div className="running-card-header">
        <span className="running-card-title" onClick={handleTitleClick}>
          {todoTitle || `Todo #${record.todo_id}`}
        </span>
        {statusTag}
      </div>
      <div className="running-card-meta">
        {record.executor && <ExecutorBadge executor={record.executor} />}
        {record.model && <Tag style={{ marginLeft: 4 }}>{record.model}</Tag>}
        {record.trigger_type && record.trigger_type !== 'manual' && (
          <Tag color="blue" style={{ marginLeft: 4 }}>{record.trigger_type}</Tag>
        )}
      </div>
      <div className="running-card-time">
        {formatRelativeTime(record.started_at)}
        {duration != null && <span className="running-card-duration"> · {formatDuration(duration)}</span>}
      </div>
      {(record.rating != null || cost != null) && (
        <div className="running-card-stats">
          {record.rating != null && (
            <Tag color={record.rating >= 80 ? 'green' : record.rating >= 50 ? 'orange' : 'red'}>
              {record.rating}分
            </Tag>
          )}
          {cost != null && cost > 0 && (
            <span className="running-card-cost">${cost.toFixed(4)}</span>
          )}
        </div>
      )}
    </div>
  );
}
