// 定时 Todo 卡片组件。

import { useCallback } from 'react';
import { Tag } from 'antd';
import type { ScheduledTodo } from '@/types';
import { ExecutorBadge } from '@/components/ExecutorBadge';
import { formatNextRunAt } from './helpers';

interface ScheduledTodoCardProps {
  todo: ScheduledTodo;
  onSelectTodo?: (id: number) => void;
}

export function ScheduledTodoCard({ todo, onSelectTodo }: ScheduledTodoCardProps) {
  const handleClick = useCallback(() => {
    onSelectTodo?.(todo.id);
  }, [todo.id, onSelectTodo]);

  return (
    <div className="running-card scheduled-card">
      <div className="running-card-header">
        <span className="running-card-title" onClick={handleClick}>{todo.title}</span>
      </div>
      <div className="running-card-meta">
        {todo.executor && <ExecutorBadge executor={todo.executor} />}
        <Tag color="purple" style={{ marginLeft: 4 }}>
          {todo.scheduler_config || 'cron'}
        </Tag>
        {todo.scheduler_timezone && (
          <Tag style={{ marginLeft: 4 }}>{todo.scheduler_timezone}</Tag>
        )}
      </div>
      <div className="running-card-footer">
        <span className="running-card-next-run">
          下次: {formatNextRunAt(todo.scheduler_next_run_at)}
        </span>
      </div>
    </div>
  );
}
