import { useRef, useEffect, useState } from 'react';
import { ExpandOutlined, CompressOutlined, InfoCircleOutlined, StopOutlined } from '@ant-design/icons';
import { Popconfirm, Popover, App } from 'antd';
import { useApp } from '@/hooks/useApp';
import { useTheme } from '@/hooks/useTheme';
import { getExecutorOption } from '@/types';
import { stopExecution } from '@/utils/database';
import { formatLocalDateTime, formatDurationSec } from '@/utils/datetime';

// Light theme log colors
const lightLogTypeColors: Record<string, string> = {
  info: '#3b82f6',
  text: '#22c55e',
  tool: '#f59e0b',
  step_start: '#8b5cf6',
  step_finish: '#14b8a6',
  stdout: '#64748b',
  stderr: '#ef4444',
  error: '#dc2626',
  system: '#6b7280',
  assistant: '#7c3aed',
  user: '#0891b2',
  result: '#22c55e',
  thinking: '#f97316',
};

// Dark theme log colors - Catppuccin Mocha inspired
const darkLogTypeColors: Record<string, string> = {
  info: '#89b4fa',
  text: '#a6e3a1',
  tool: '#f9e2af',
  step_start: '#cba6f7',
  step_finish: '#94e2d5',
  stdout: '#cdd6f4',
  stderr: '#f38ba8',
  error: '#f38ba8',
  system: '#6c7086',
  assistant: '#cba6f7',
  user: '#89dceb',
  result: '#a6e3a1',
  thinking: '#fab387',
};

const logTypeLabels: Record<string, string> = {
  info: 'INFO',
  text: 'TEXT',
  tool: 'TOOL',
  step_start: 'START',
  step_finish: 'END',
  stdout: 'OUT',
  stderr: 'ERR',
  error: 'ERROR',
  system: 'SYS',
  assistant: 'ASST',
  user: 'USER',
  result: 'RESULT',
  thinking: 'THINK',
};

interface ExecutionPanelProps {
  collapsed: boolean;
  onToggleCollapse: () => void;
}

function formatShortTime(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleTimeString('zh-CN', {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    });
  } catch {
    return iso;
  }
}

export function ExecutionPanel({ collapsed, onToggleCollapse }: ExecutionPanelProps) {
  const { state, dispatch } = useApp();
  const { themeMode } = useTheme();
  const { runningTasks, activeTaskId, executionRecords } = state;
  const { message } = App.useApp();
  const logsEndRef = useRef<HTMLDivElement>(null);
  const [fullscreen, setFullscreen] = useState(false);

  const logTypeColors = themeMode === 'dark' ? darkLogTypeColors : lightLogTypeColors;

  const taskIds = Object.keys(runningTasks);
  const activeTask = activeTaskId ? runningTasks[activeTaskId] : null;

  // Tick for elapsed time display - only runs when tasks are active
  const hasRunningTasks = taskIds.some(id => runningTasks[id]?.status === 'running');
  const [, setTick] = useState(0);
  useEffect(() => {
    if (!hasRunningTasks || collapsed) return;
    const interval = setInterval(() => setTick(t => t + 1), 1000);
    return () => clearInterval(interval);
  }, [hasRunningTasks, collapsed]);

  useEffect(() => {
    if (logsEndRef.current && !collapsed && activeTask) {
      logsEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [activeTask?.logs, collapsed, activeTask]);

  // Finished tasks auto-remove after 5s
  useEffect(() => {
    const timers: ReturnType<typeof setTimeout>[] = [];
    Object.entries(runningTasks).forEach(([id, task]) => {
      if (task.status === 'finished' && task.finishedAt) {
        const elapsed = Date.now() - new Date(task.finishedAt).getTime();
        const delay = Math.max(0, 5000 - elapsed);
        timers.push(setTimeout(() => {
          dispatch({ type: 'REMOVE_RUNNING_TASK', payload: id });
        }, delay));
      }
    });
    return () => timers.forEach(clearTimeout);
  }, [runningTasks, dispatch]);

  // Get elapsed seconds for a task
  const getElapsedSeconds = (startedAt: string) => {
    const start = new Date(startedAt).getTime();
    const now = Date.now();
    return Math.floor((now - start) / 1000);
  };

  // Find execution record by task_id for stopping
  const findRecordByTaskId = (taskId: string) => {
    for (const records of Object.values(executionRecords)) {
      const found = records.find(r => r.task_id === taskId);
      if (found) return found;
    }
    return null;
  };

  // Handle stop execution
  const handleStop = async (taskId: string) => {
    const record = findRecordByTaskId(taskId);
    if (!record) {
      message.error('找不到对应的执行记录');
      return;
    }
    try {
      await stopExecution(record.id);
      message.success('已停止执行');
    } catch (err) {
      message.error(`停止失败: ${err}`);
    }
  };

  if (taskIds.length === 0) return null;

  return (
    <div className={`execution-panel ${collapsed ? 'collapsed' : ''} ${fullscreen ? 'fullscreen' : ''}`}>
      {/* Tab Bar */}
      <div className="execution-panel-tabs">
        <div className="execution-panel-tabs-scroll">
          {taskIds.map((taskId) => {
            const task = runningTasks[taskId];
            const opt = getExecutorOption(task.executor);
            const isActive = taskId === activeTaskId;
            return (
              <div
                key={taskId}
                className={`execution-tab ${isActive ? 'active' : ''} ${task.status}`}
                onClick={() => {
                  dispatch({ type: 'SET_ACTIVE_TASK', payload: taskId });
                  if (collapsed) onToggleCollapse();
                }}
                role="button"
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    dispatch({ type: 'SET_ACTIVE_TASK', payload: taskId });
                  }
                }}
              >
                <span className="tab-icon">{opt.icon}</span>
                <span className="tab-title" title={task.todoTitle}>
                  {task.todoTitle}
                </span>
                {task.status === 'running' && <span className="tab-spinner" />}
                {task.status === 'running' && (
                  <>
                    <Popconfirm
                      title="确定停止该任务？"
                      onConfirm={() => handleStop(taskId)}
                      onCancel={(e) => e?.stopPropagation()}
                      okText="停止"
                      cancelText="取消"
                    >
                      <StopOutlined
                        style={{ fontSize: 12, marginLeft: 4, color: 'var(--color-error)', cursor: 'pointer' }}
                        onClick={(e) => e.stopPropagation()}
                        title="停止"
                      />
                    </Popconfirm>
                    <Popover
                      trigger="click"
                      placement="bottom"
                      content={
                        <div style={{ minWidth: 200 }} onClick={(e) => e.stopPropagation()}>
                          <div style={{ fontSize: 12, marginBottom: 8 }}><strong>{task.todoTitle}</strong></div>
                          <div style={{ fontSize: 11, color: 'var(--color-text-secondary)', marginBottom: 4 }}>
                            <span style={{ fontWeight: 600 }}>执行器:</span> {task.executor}
                          </div>
                          <div style={{ fontSize: 11, color: 'var(--color-text-secondary)', marginBottom: 4 }}>
                            <span style={{ fontWeight: 600 }}>开始时间:</span> {formatLocalDateTime(task.startedAt)}
                          </div>
                          <div style={{ fontSize: 11, color: 'var(--color-info)', fontWeight: 600 }}>
                            <span style={{ fontWeight: 600 }}>已运行:</span> {formatDurationSec(getElapsedSeconds(task.startedAt))}
                          </div>
                        </div>
                      }
                    >
                      <InfoCircleOutlined
                        style={{ fontSize: 12, marginLeft: 4, color: 'var(--color-text-secondary)', cursor: 'pointer' }}
                        onClick={(e) => e.stopPropagation()}
                      />
                    </Popover>
                  </>
                )}
              </div>
            );
          })}
        </div>
        <div className="execution-panel-actions">
          <span className="task-count">{taskIds.length} 个任务</span>
          <button
            className="panel-toggle-btn"
            onClick={() => {
              if (fullscreen) {
                setFullscreen(false);
              } else {
                setFullscreen(true);
                if (collapsed) onToggleCollapse();
              }
            }}
            aria-label={fullscreen ? '退出全屏' : '全屏'}
            title={fullscreen ? '退出全屏' : '全屏'}
          >
            {fullscreen ? <CompressOutlined /> : <ExpandOutlined />}
          </button>
          <button
            className="panel-toggle-btn"
            onClick={() => {
              if (fullscreen) setFullscreen(false);
              onToggleCollapse();
            }}
            aria-label={collapsed ? '展开' : '收起'}
          >
            {collapsed ? '▲' : '▼'}
          </button>
        </div>
      </div>

      {/* Log Area */}
      {!collapsed && activeTask && (
        <div className="execution-panel-logs">
          {activeTask.logs.length === 0 ? (
            <div className="execution-panel-empty">等待输出...</div>
          ) : (
            <>
              {activeTask.logs.map((log, idx) => (
                <div key={idx} className="log-line">
                  <span className="log-timestamp">{formatShortTime(log.timestamp)}</span>
                  <span
                    className="log-type-badge"
                    style={{
                      color: logTypeColors[log.type] || '#cbd5e1',
                      background: `${logTypeColors[log.type] || '#cbd5e1'}20`,
                    }}
                  >
                    {logTypeLabels[log.type] || log.type}
                  </span>
                  <span className="log-content">{log.content}</span>
                </div>
              ))}
              {activeTask.status === 'finished' && activeTask.result && (
                <div
                  className={`log-result ${activeTask.success ? 'log-result-success' : 'log-result-error'}`}
                >
                  {activeTask.result}
                </div>
              )}
              <div ref={logsEndRef} />
            </>
          )}
        </div>
      )}
    </div>
  );
}
