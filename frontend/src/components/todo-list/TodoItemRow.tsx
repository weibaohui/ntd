// Todo 列表项行组件。

import { Checkbox } from 'antd';
import type { Todo, Tag } from '@/types';
import { ExecutorBadge } from '@/components/ExecutorBadge';
import { StatusPicker } from '@/components/StatusPicker';
import { formatRelativeTime } from '@/utils/datetime';
import { ClockCircleOutlined } from '@ant-design/icons';

interface TodoItemRowProps {
  todo: Todo;
  tags: Tag[];
  selectedTodoId: number | null;
  selectedIds: number[];
  onSelectTodo: (id: number) => void;
  onSelect: (id: number) => void;
  onToggleSelect: (id: number) => void;
  onStatusChange: (todoId: number, title: string, prompt: string, newStatus: string) => void;
}

/**
 * Todo 列表项行组件。
 * 抽离平铺与分组两个模式共用，避免重复代码。
 */
export function TodoItemRow({
  todo,
  tags,
  selectedTodoId,
  selectedIds,
  onSelectTodo,
  onSelect,
  onToggleSelect,
  onStatusChange,
}: TodoItemRowProps) {
  // tag_ids 在 Todo 类型中是必填 number[]，但历史接口偶发返回缺失字段，
  // 所以用可选链 + 空数组兜底，避免运行时崩溃。
  const todoTags = todo.tag_ids?.map(id => tags.find(t => t.id === id)).filter((t): t is Tag => !!t) ?? [];
  const primaryTag = todoTags[0];
  const isCompleted = todo.status === 'completed';
  const relativeTime = formatRelativeTime(todo.updated_at);
  const isChecked = selectedIds.includes(todo.id);

  return (
    <div
      key={todo.id}
      onClick={() => {
        onSelectTodo(todo.id);
        onSelect(todo.id);
      }}
      className={`todo-item ${selectedTodoId === todo.id ? 'selected' : ''}`}
      style={{
        cursor: 'pointer',
        borderLeftColor: primaryTag?.color || '#cbd5e1',
        borderLeftWidth: 4,
        borderLeftStyle: 'solid',
        // 工具栏的多选复选框用 position: absolute 浮在卡片左上；
        // 若 .todo-item 不设 position: relative，复选框会逃逸到上层容器，
        // 所有卡片的复选框都叠在同一个屏幕坐标，点击会命中最后渲染的那个。
        position: 'relative',
      }}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onSelectTodo(todo.id);
          onSelect(todo.id);
        }
      }}
    >
      {/* 多选复选框：position absolute 浮在卡片左上，避免打乱原本的 layout。
          stopPropagation 阻止冒泡到卡片的 onClick（不会触发详情选中）。 */}
      <Checkbox
        checked={isChecked}
        onChange={(e) => { e.stopPropagation(); onToggleSelect(todo.id); }}
        onClick={(e) => e.stopPropagation()}
        data-testid={`todo-row-checkbox-${todo.id}`}
        style={{ position: 'absolute', top: 12, left: 12, zIndex: 1 }}
      />
      <div className="todo-item-content" style={{ paddingLeft: 28 }}>
        <div className="todo-item-main">
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
            <div
              className="todo-item-title"
              style={{ opacity: isCompleted ? 0.6 : 1 }}
            >
              <span style={{ color: '#999', marginRight: 4, fontSize: 13 }}>#{todo.id}</span>{todo.title}
            </div>
            <ExecutorBadge executor={todo.executor || 'claudecode'} />
          </div>
          {todo.prompt && (
            <div className="todo-item-desc">
              {todo.prompt.length > 60 ? todo.prompt.substring(0, 60) + '...' : todo.prompt}
            </div>
          )}
          <div className="todo-item-tags" style={{ justifyContent: 'space-between' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 4, flexWrap: 'wrap' }}>
              {todoTags.map(t => (
                <span
                  key={t.id}
                  className="todo-tag-badge"
                  style={{
                    backgroundColor: t.color + '18',
                    color: t.color,
                    border: `1px solid ${t.color}30`,
                  }}
                >
                  {t.name}
                </span>
              ))}
              {todo.scheduler_config && (
                <ClockCircleOutlined
                  style={{
                    fontSize: 12,
                    color: todo.scheduler_enabled ? 'var(--color-warning)' : 'var(--color-text-tertiary)',
                    marginLeft: todoTags.length > 0 ? 4 : 0,
                  }}
                />
              )}
              {/* todo_type === 1 已废弃：评审模板自 V15 起迁出至 review_templates 表。 */}
              {todo.todo_type === 2 && (
                <span
                  className="todo-tag-badge"
                  style={{
                    backgroundColor: '#13c2c218',
                    color: '#13c2c2',
                    border: '1px solid #13c2c230',
                  }}
                  title={`评审实例 (原 todo #${todo.parent_todo_id ?? '?'})`}
                >
                  [评审]
                </span>
              )}
            </div>
            <span
              style={{
                fontSize: 11,
                color: 'var(--color-text-quaternary)',
                flexShrink: 0,
                marginLeft: 8,
              }}
              title={relativeTime}
            >
              {relativeTime}
            </span>
          </div>
        </div>
        <div
          className="todo-item-status"
          aria-label="更改任务状态"
        >
          <StatusPicker
            value={todo.status}
            onChange={(newStatus) => onStatusChange(todo.id, todo.title, todo.prompt || '', newStatus)}
          />
        </div>
      </div>
    </div>
  );
}
