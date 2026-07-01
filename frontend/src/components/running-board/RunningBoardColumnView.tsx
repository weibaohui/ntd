// RunningBoard 列视图组件。

import type { ExecutionRecord, ScheduledTodo, RunningBoardColumn } from '@/types';
import { ScheduledTodoCard } from './ScheduledTodoCard';
import { ExecutionRecordCard } from './ExecutionRecordCard';
import { COLUMN_ICONS } from './helpers';

interface RunningBoardColumnViewProps {
  columnKey: RunningBoardColumn;
  label: string;
  color: string;
  records: ExecutionRecord[];
  scheduledTodos: ScheduledTodo[];
  onSelectTodo?: (id: number) => void;
  onCardClick?: (record: ExecutionRecord) => void;
  getTodoTitle?: (id: number) => string | undefined;
}

export function RunningBoardColumnView({
  columnKey,
  label,
  color,
  records,
  scheduledTodos,
  onSelectTodo,
  onCardClick,
  getTodoTitle,
}: RunningBoardColumnViewProps) {
  const count = records.length + scheduledTodos.length;

  return (
    <div className="running-column">
      <div className="running-column-header" style={{ borderBottomColor: color }}>
        <div className="running-column-title">
          <div className="running-column-dot" style={{ backgroundColor: color }} />
          <span className="running-column-icon" style={{ color }}>{COLUMN_ICONS[columnKey]}</span>
          <span>{label}</span>
          <span className="running-column-count">{count}</span>
        </div>
      </div>
      <div className="running-column-body">
        {count === 0 ? (
          <div className="running-column-empty">暂无</div>
        ) : (
          <>
            {scheduledTodos.map(todo => (
              <ScheduledTodoCard key={`scheduled-${todo.id}`} todo={todo} onSelectTodo={onSelectTodo} />
            ))}
            {records.map(record => (
              <ExecutionRecordCard
                key={`record-${record.id}`}
                record={record}
                todoTitle={getTodoTitle?.(record.todo_id)}
                onSelectTodo={onSelectTodo}
                onCardClick={onCardClick}
              />
            ))}
          </>
        )}
      </div>
    </div>
  );
}
