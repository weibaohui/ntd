import { useRef, useEffect, useState } from 'react';
import { ExpandOutlined, CompressOutlined, InfoCircleOutlined, StopOutlined, CloseOutlined } from '@ant-design/icons';
import { Popconfirm, Popover, Dropdown, App } from 'antd';
import { useApp } from '@/hooks/useApp';
import { useTheme } from '@/hooks/useTheme';
import { getExecutorOption } from '@/types';
import { stopExecution } from '@/utils/database';
import { formatLocalDateTime, formatDurationSec } from '@/utils/datetime';
import { LOG_TYPE_COLORS_LIGHT, LOG_TYPE_COLORS_DARK, LOG_TYPE_LABELS } from '@/constants';

interface ExecutionPanelProps {
  collapsed: boolean;
  onToggleCollapse: () => void;
  // 开关关闭时由父组件传入 true：在此 return null 不渲染面板，但 hooks 已在上文注册，
  // 故「完成后自动移除任务」的定时器仍会运行，避免隐藏期间运行任务列表泄漏。
  hidden?: boolean;
  // 临时关闭：仅本轮任务期间隐藏，新一轮任务开始或设置重新开启时自动恢复。
  onTemporaryClose?: () => void;
  // 永久关闭：等价于把设置里的开关置 false 并落盘，需用户去设置-界面显示重新开启。
  onPermanentClose?: () => void;
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

export function ExecutionPanel({ collapsed, onToggleCollapse, hidden, onTemporaryClose, onPermanentClose }: ExecutionPanelProps) {
  const { state, dispatch } = useApp();
  const { themeMode } = useTheme();
  const { runningTasks, activeTaskId, executionRecords } = state;
  const { message } = App.useApp();
  const logsEndRef = useRef<HTMLDivElement>(null);
  const [fullscreen, setFullscreen] = useState(false);

  const logTypeColors = themeMode === 'dark' ? LOG_TYPE_COLORS_DARK : LOG_TYPE_COLORS_LIGHT;

  const taskIds = Object.keys(runningTasks);
  const activeTask = activeTaskId ? runningTasks[activeTaskId] : null;

  // Tick for elapsed time display - only runs when tasks are active
  const hasRunningTasks = taskIds.some(id => runningTasks[id]?.status === 'running');
  const [, setTick] = useState(0);
  useEffect(() => {
    // 隐藏时面板 return null、计时数字本就不可见，没必要每秒 tick 触发空重渲染，一并短路。
    if (!hasRunningTasks || collapsed || hidden) return;
    const interval = setInterval(() => setTick(t => t + 1), 1000);
    return () => clearInterval(interval);
  }, [hasRunningTasks, collapsed, hidden]);

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
      // v1 纯 workspace-scoped：stopExecution 需 workspaceId
      await stopExecution(state.selectedWorkspace ?? 0, record.id);
      message.success('已停止执行');
    } catch (err) {
      message.error(`停止失败: ${err}`);
    }
  };

  // 无运行任务或被设置开关隐藏时均不渲染：hooks 在上文已注册，定时器照常运行。
  if (hidden || taskIds.length === 0) return null;

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
          {/* 关闭按钮：下拉两种关闭方式，避免两个相似 X 图标造成歧义。
              临时关闭=本轮隐藏、下次任务自动恢复；永久关闭=落盘关闭设置。 */}
          <Dropdown
            trigger={['click']}
            placement="topRight"
            menu={{
              items: [
                { key: 'temporary', label: '临时关闭（下次执行自动恢复）' },
                { key: 'permanent', label: '永久关闭（设置-界面显示中重新开启）' },
              ],
              onClick: ({ key }) => {
                if (key === 'temporary') onTemporaryClose?.();
                else if (key === 'permanent') onPermanentClose?.();
              },
            }}
          >
            <button
              className="panel-toggle-btn"
              aria-label="关闭"
              title="关闭"
              // 阻止冒泡到 tab 的点击切换，避免关闭面板的同时误切任务。
              onClick={(e) => e.stopPropagation()}
            >
              <CloseOutlined />
            </button>
          </Dropdown>
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
                    {LOG_TYPE_LABELS[log.type] || log.type}
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
