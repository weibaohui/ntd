// Todo 列表项行组件 — 简洁卡片风格。
//
// 对齐环路列表 LoopCard 的视觉风格：去掉 prompt 显示，改用左 3px 色条 +
// 标题行 + meta 行的紧凑布局，不再用 4px 左边框和高胖的 desc 区域。
//
// 设计取舍：
// - 用 inline style 处理 hover 的 transform/boxShadow 过渡，避免依赖额外 CSS class。
// - 选中态用 inset box-shadow 模拟边框，不改变元素实际尺寸，防止 layout 跳动。

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
 * Todo 列表项行组件 — 简洁卡片风格。
 * 去掉了 prompt 显示；左 3px 色条 + 标题行 + meta 行，对齐 LoopCard 紧凑布局。
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
  const isSelected = selectedTodoId === todo.id;

  return (
    <div
      key={todo.id}
      onClick={() => {
        onSelectTodo(todo.id);
        onSelect(todo.id);
      }}
      className={`todo-item ${isSelected ? 'selected' : ''}`}
      style={{
        cursor: 'pointer',
        position: 'relative',
        // 选中态用 inset box-shadow 模拟边框，避免 border 撑大尺寸导致 layout 跳动
        boxShadow: isSelected
          ? 'inset 0 0 0 1px var(--color-primary, #0891b2)'
          : '0 1px 2px color-mix(in srgb, var(--color-text, #0f172a) 6%, transparent)',
        // 与 LoopCard 一致的过渡动画
        transition: 'background 200ms, border-color 200ms, box-shadow 200ms, transform 200ms',
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
      // hover 效果：非选中态时加深 border + 微抬升，与 LoopCard 行为一致
      onMouseEnter={(e) => {
        if (!isSelected) {
          e.currentTarget.style.borderColor = 'var(--color-text-tertiary, #94a3b8)';
          e.currentTarget.style.boxShadow = '0 4px 10px color-mix(in srgb, var(--color-text, #0f172a) 10%, transparent)';
          e.currentTarget.style.transform = 'translateY(-1px)';
        }
      }}
      onMouseLeave={(e) => {
        if (!isSelected) {
          e.currentTarget.style.borderColor = 'var(--color-border, #e2e8f0)';
          e.currentTarget.style.boxShadow = '0 1px 2px color-mix(in srgb, var(--color-text, #0f172a) 6%, transparent)';
          e.currentTarget.style.transform = 'translateY(0)';
        }
      }}
    >
      {/* 左侧 3px 颜色条：标签色兜底到 primary 色 */}
      <span
        style={{
          position: 'absolute', left: 0, top: 0, bottom: 0, width: 3,
          background: primaryTag?.color || 'var(--color-primary, #0891b2)',
          borderRadius: '3px 0 0 3px',
        }}
      />

      {/* 多选复选框：position absolute 浮在卡片左上，避免打乱 layout */}
      <Checkbox
        checked={isChecked}
        onChange={(e) => { e.stopPropagation(); onToggleSelect(todo.id); }}
        onClick={(e) => e.stopPropagation()}
        data-testid={`todo-row-checkbox-${todo.id}`}
        style={{ position: 'absolute', top: 12, left: 12, zIndex: 1 }}
      />
      <div className="todo-item-content" style={{ paddingLeft: 28 }}>
        <div className="todo-item-main">
          {/* 标题行：#id + 标题 + ExecutorBadge（紧凑排列，对齐 LoopCard 标题行） */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
            <span style={{ color: 'var(--color-text-tertiary, #94a3b8)', fontSize: 11, fontFamily: 'monospace' }}>
              #{todo.id}
            </span>
            <div
              className="todo-item-title"
              style={{
                opacity: isCompleted ? 0.6 : 1,
                flex: 1, minWidth: 0,
                overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
              }}
            >
              {todo.title}
            </div>
            {/* ExecutorBadge 放在标题右侧，类似 LoopCard 的 status Tag */}
            <ExecutorBadge executor={todo.executor || 'claudecode'} />
          </div>

          {/* meta 行：标签 + 调度器图标 + 评审徽章 + 相对时间（紧凑、对齐 LoopCard meta 行） */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)' }}>
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
                  fontSize: 11,
                  color: todo.scheduler_enabled ? 'var(--color-warning)' : 'var(--color-text-tertiary)',
                }}
              />
            )}
            {todo.todo_type === 2 && (
              <span
                className="todo-tag-badge"
                style={{
                  backgroundColor: '#13c2c218',
                  color: '#13c2c2',
                  border: '1px solid #13c2c230',
                }}
              >
                评审
              </span>
            )}
            <span style={{ marginLeft: 'auto' }}>{relativeTime}</span>
          </div>
        </div>
        {/* StatusPicker 保持原有位置，不改变交互方式 */}
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
