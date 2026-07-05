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
}: TodoItemRowProps) {
  // tag_ids 在 Todo 类型中是必填 number[]，但历史接口偶发返回缺失字段，
  // 所以用可选链 + 空数组兜底，避免运行时崩溃。
  const todoTags = todo.tag_ids?.map(id => tags.find(t => t.id === id)).filter((t): t is Tag => !!t) ?? [];
  const primaryTag = todoTags[0];
  const isCompleted = todo.status === 'completed';
  const relativeTime = formatRelativeTime(todo.updated_at);
  const isChecked = selectedIds.includes(todo.id);
  const isSelected = selectedTodoId === todo.id;

  // 底部 3px 颜色条映射：按 todo 状态着色，与 LoopCard 的 progressBarColor 设计一致
  const statusBarColor: Record<string, string> = {
    pending: 'var(--color-text-quaternary, #94a3b8)',
    running: 'var(--color-info, #3b82f6)',
    completed: 'var(--color-success, #22c55e)',
    failed: 'var(--color-error, #ef4444)',
  };
  // pending 状态用半透明表示"未开始"，其他状态全透明
  const barColor = statusBarColor[todo.status] || statusBarColor.pending;
  const barOpacity = todo.status === 'pending' ? 0.35 : 1;

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
        // 选中态双层阴影：外发光 + 顶部高光，营造精致层次感；
        // 外层柔光让卡片"浮起来"，内层高光增加立体感
        boxShadow: isSelected
          ? [
              '0 0 0 1px color-mix(in srgb, var(--color-primary, #0891b2) 30%, transparent)',
              '0 2px 8px color-mix(in srgb, var(--color-primary, #0891b2) 15%, transparent)',
              'inset 0 1px 0 color-mix(in srgb, var(--color-primary, #0891b2) 20%, white)',
            ].join(', ')
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
      </div>
      {/* 底部 3px 颜色条：按 todo 状态着色，与 LoopCard 底部进度条设计一致 */}
      <div
        style={{
          position: 'absolute', left: 0, right: 0, bottom: 0, height: 3,
          background: barColor,
          opacity: barOpacity,
          borderRadius: '0 0 10px 10px',
        }}
      />
    </div>
  );
}
