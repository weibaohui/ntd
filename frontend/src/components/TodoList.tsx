import { useState, useEffect, useMemo } from 'react';
import { useApp } from '../hooks/useApp';
import { Button, Empty, Tooltip } from 'antd';
import { PlusOutlined, ThunderboltOutlined, ClockCircleOutlined, InboxOutlined, DashboardOutlined, ReadOutlined, SettingOutlined, SunOutlined, MoonOutlined } from '@ant-design/icons';
import { useTheme } from '../hooks/useTheme';
import { StatusPicker } from './StatusPicker';
import * as db from '../utils/database';
import { ExecutorBadge } from './ExecutorBadge';
import { formatRelativeTime, formatLocalDateTime } from '../utils/datetime';

interface TodoListProps {
  onOpenCreateModal: () => void;
  onOpenSmartCreate: () => void;
  onSelectTodo?: (todoId: string | number) => void;
  onShowDashboard?: () => void;
  onShowMemorial?: () => void;
  onShowSettings?: () => void;
}

function SkeletonRow() {
  return <div className="skeleton-row" />;
}

function SkeletonList() {
  return (
    <div style={{ padding: '12px 16px' }}>
      {Array.from({ length: 6 }).map((_, i) => (
        <SkeletonRow key={i} />
      ))}
    </div>
  );
}

export function TodoList({ onOpenCreateModal, onOpenSmartCreate, onSelectTodo, onShowDashboard, onShowMemorial, onShowSettings }: TodoListProps) {
  const { state, dispatch } = useApp();
  const { themeMode, toggleTheme } = useTheme();
  const { todos, selectedTodoId, selectedTagId, tags } = state;
  const [isMobile, setIsMobile] = useState(false);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    const checkMobile = () => setIsMobile(window.innerWidth < 768);
    checkMobile();
    window.addEventListener('resize', checkMobile);
    return () => window.removeEventListener('resize', checkMobile);
  }, []);

  useEffect(() => {
    const timer = setTimeout(() => setIsLoading(false), 400);
    return () => clearTimeout(timer);
  }, []);

  const filteredTodos = useMemo(() =>
    selectedTagId
      ? todos.filter(t => (t as any).tag_ids?.includes(selectedTagId))
      : todos,
    [todos, selectedTagId]
  );

  if (isLoading) {
    return (
      <div className="todo-list-container">
        <SkeletonList />
      </div>
    );
  }

  return (
    <div className="todo-list-container">
      {/* Header */}
      <div className="todo-list-header">
        {/* NTD Logo */}
        <div className="ntd-logo" aria-label="NTD Logo">NTD</div>
        <div className="header-actions">
          <Button
            type="text"
            size="small"
            icon={<DashboardOutlined />}
            onClick={onShowDashboard}
            className="tag-btn"
            aria-label="查看仪表盘"
          />
          <Tooltip title="看板">
            <Button
              type="text"
              size="small"
              icon={<ReadOutlined />}
              onClick={() => onShowMemorial?.()}
              className="tag-btn"
              aria-label="看板"
            />
          </Tooltip>
          <Tooltip title={themeMode === 'light' ? '切换暗色主题' : '切换亮色主题'}>
            <Button
              type="text"
              size="small"
              icon={themeMode === 'light' ? <MoonOutlined /> : <SunOutlined />}
              onClick={toggleTheme}
              className="tag-btn"
              aria-label="切换主题"
            />
          </Tooltip>
          <Button
            type="text"
            size="small"
            icon={<SettingOutlined />}
            onClick={onShowSettings}
            className="tag-btn"
            aria-label="配置管理"
          />
          {!isMobile && (
            <>
              <span className="header-actions-divider" />
              <Tooltip title="智能新建">
                <Button
                  type="text"
                  size="small"
                  icon={<ThunderboltOutlined />}
                  className="smart-create-btn"
                  onClick={onOpenSmartCreate}
                  aria-label="智能新建"
                />
              </Tooltip>
              <Tooltip title="新建任务">
                <Button
                  type="text"
                  size="small"
                  icon={<PlusOutlined />}
                  className="create-btn"
                  onClick={onOpenCreateModal}
                  aria-label="新建任务"
                />
              </Tooltip>
            </>
          )}
        </div>
      </div>

      {/* Tag filter chips */}
      {tags.length > 0 && (
        <div className="tag-filter-bar">
          <button
            className={`tag-chip ${selectedTagId === null ? 'active' : ''}`}
            onClick={() => dispatch({ type: 'SELECT_TAG', payload: null })}
          >
            全部
          </button>
          {tags.map(tag => (
            <button
              key={tag.id}
              className={`tag-chip ${selectedTagId === tag.id ? 'active' : ''}`}
              style={{ '--tag-color': tag.color } as React.CSSProperties}
              onClick={() => dispatch({ type: 'SELECT_TAG', payload: tag.id })}
            >
              <span className="tag-dot" style={{ backgroundColor: tag.color }} />
              {tag.name}
            </button>
          ))}
        </div>
      )}

      {/* Todo list */}
      <div className="todo-list-content">
        {filteredTodos.length === 0 ? (
          <div className="empty-state">
            <div className="empty-state-icon">
              <InboxOutlined />
            </div>
            <Empty
              description={
                <div style={{ color: 'var(--color-text-tertiary)', fontSize: 14 }}>
                  {selectedTagId ? '该标签下暂无任务' : '暂无任务'}
                  <br />
                  <span style={{ fontSize: 13, marginTop: 4, display: 'inline-block' }}>
                    点击右上角新建按钮创建第一个任务
                  </span>
                </div>
              }
              image={Empty.PRESENTED_IMAGE_SIMPLE}
            />
          </div>
        ) : (
          filteredTodos.map(todo => {
            const todoTags = tags.filter(t => (todo as any).tag_ids?.includes(t.id));
            const primaryTag = todoTags[0];
            const isCompleted = todo.status === 'completed';

            return (
              <div
                key={todo.id}
                onClick={() => {
                  dispatch({ type: 'SELECT_TODO', payload: todo.id });
                  onSelectTodo?.(todo.id);
                }}
                className={`todo-item ${selectedTodoId === todo.id ? 'selected' : ''}`}
                style={{
                  cursor: 'pointer',
                  borderLeftColor: primaryTag?.color || '#cbd5e1',
                  borderLeftWidth: 4,
                  borderLeftStyle: 'solid',
                }}
                role="button"
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    dispatch({ type: 'SELECT_TODO', payload: todo.id });
                    onSelectTodo?.(todo.id);
                  }
                }}
              >
                <div className="todo-item-content">
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
                      </div>
                      <span
                        style={{
                          fontSize: 11,
                          color: 'var(--color-text-quaternary)',
                          flexShrink: 0,
                          marginLeft: 8,
                        }}
                        title={formatLocalDateTime(todo.updated_at)}
                      >
                        {formatRelativeTime(todo.updated_at)}
                      </span>
                    </div>
                  </div>
                  <div
                    className="todo-item-status"
                    aria-label="更改任务状态"
                  >
                    <StatusPicker
                      value={todo.status}
                      onChange={async (newStatus) => {
                        try {
                          const updated = await db.updateTodo(
                            todo.id,
                            todo.title,
                            todo.prompt || '',
                            newStatus
                          );
                          dispatch({
                            type: 'UPDATE_TODO',
                            payload: updated
                          });
                        } catch {
                          // ignore: interceptor already shows error
                        }
                      }}
                    />
                  </div>
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
