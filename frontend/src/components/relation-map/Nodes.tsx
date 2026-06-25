import { Handle, Position, type NodeProps } from '@xyflow/react';
import { Tag } from 'antd';
import {
  CheckCircleOutlined,
  ClockCircleOutlined,
  ExclamationCircleOutlined,
  PlayCircleOutlined,
  MessageOutlined,
  ScheduleOutlined,
} from '@ant-design/icons';
import { getExecutorColor } from '@/types';
import type { TodoNodeData, FeishuNodeData, SchedulerNodeData } from './types';

const STATUS_CONFIG: Record<string, { color: string; icon: React.ReactNode; label: string }> = {
  pending: { color: '#8c8c8c', icon: <ClockCircleOutlined />, label: '待执行' },
  in_progress: { color: '#1677ff', icon: <PlayCircleOutlined />, label: '进行中' },
  running: { color: '#1677ff', icon: <PlayCircleOutlined />, label: '执行中' },
  completed: { color: '#52c41a', icon: <CheckCircleOutlined />, label: '已完成' },
  failed: { color: '#ff4d4f', icon: <ExclamationCircleOutlined />, label: '失败' },
};

/** Todo 节点 */
export function TodoNode({ data, selected }: NodeProps & { data: TodoNodeData }) {
  const status = STATUS_CONFIG[data.status] || STATUS_CONFIG.pending;
  const executorColor = getExecutorColor(data.executor);
  const isRunning = data.status === 'running' || data.status === 'in_progress';

  return (
    <div
      className={`relation-map-node todo-node ${selected ? 'selected' : ''} ${isRunning ? 'running' : ''}`}
      style={{ '--status-color': status.color, '--executor-color': executorColor } as React.CSSProperties}
    >
      <Handle type="target" position={Position.Left} className="relation-map-handle" />

      <div className="todo-node-header">
        <span className="todo-node-status-icon" style={{ color: status.color }}>{status.icon}</span>
        <span className="todo-node-title">{data.title}</span>
      </div>

      <div className="todo-node-footer">
        <Tag
          color={executorColor}
          style={{ fontSize: 11, lineHeight: '18px', padding: '0 4px', margin: 0, borderRadius: 4 }}
        >
          {data.executorName || data.executor || 'claudecode'}
        </Tag>
        <span className="todo-node-status" style={{ color: status.color }}>{status.label}</span>
      </div>

      {isRunning && <div className="todo-node-pulse" />}

      <Handle type="source" position={Position.Right} className="relation-map-handle" />
    </div>
  );
}

/** 飞书消息节点 */
export function FeishuNode({ data, selected }: NodeProps & { data: FeishuNodeData }) {
  return (
    <div className={`relation-map-node source-node feishu-node ${selected ? 'selected' : ''}`}>
      <Handle type="source" position={Position.Right} className="relation-map-handle" />

      <div className="source-node-icon" style={{ background: '#1890ff' }}>
        <MessageOutlined style={{ color: '#fff', fontSize: 18 }} />
      </div>
      <div className="source-node-label">{data.label}</div>
      {data.slashCommand && (
        <Tag color="blue" style={{ fontSize: 10, lineHeight: '16px', padding: '0 4px', margin: 0 }}>
          {data.slashCommand}
        </Tag>
      )}
    </div>
  );
}

/** 调度器节点 */
export function SchedulerNode({ data, selected }: NodeProps & { data: SchedulerNodeData }) {
  return (
    <div className={`relation-map-node source-node scheduler-node ${selected ? 'selected' : ''}`}>
      <Handle type="source" position={Position.Right} className="relation-map-handle" />

      <div className="source-node-icon" style={{ background: '#fa8c16' }}>
        <ScheduleOutlined style={{ color: '#fff', fontSize: 18 }} />
      </div>
      <div className="source-node-label">{data.label}</div>
      {data.cron && (
        <Tag color="orange" style={{ fontSize: 10, lineHeight: '16px', padding: '0 4px', margin: 0 }}>
          {data.cron}
        </Tag>
      )}
    </div>
  );
}
