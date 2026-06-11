import { useCallback } from 'react';
import {
  BaseEdge,
  getBezierPath,
  type EdgeProps,
  EdgeLabelRenderer,
} from '@xyflow/react';

/** Hook 边：带触发条件标签和评分闸门 */
export function HookEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  data,
  selected,
}: EdgeProps & { data?: { trigger?: string; hookId?: number; enabled?: boolean; minRating?: number | null; unratedPolicy?: string } }) {
  const [edgePath, labelX, labelY] = getBezierPath({
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
  });

  const trigger = data?.trigger || '';
  const minRating = data?.minRating;
  const unratedPolicy = data?.unratedPolicy;
  const hasRatingGate = minRating != null && minRating > 0;

  const getTriggerColor = useCallback((t: string) => {
    switch (t) {
      case 'state_changed_to_completed': return '#52c41a';
      case 'state_changed_to_failed': return '#ff4d4f';
      case 'state_changed_to_pending': return '#8c8c8c';
      case 'state_changed_to_in_progress': return '#1677ff';
      default: return '#999';
    }
  }, []);

  const getTriggerLabel = useCallback((t: string) => {
    switch (t) {
      case 'state_changed_to_completed': return '已完成';
      case 'state_changed_to_failed': return '失败';
      case 'state_changed_to_pending': return '待执行';
      case 'state_changed_to_in_progress': return '执行中';
      default: return t;
    }
  }, []);

  const color = getTriggerColor(trigger);

  // 构建闸门标签
  const gateLabel = hasRatingGate
    ? `≥${minRating}${unratedPolicy === 'pass' ? '·未评过' : '·未评跳'}`
    : null;

  return (
    <>
      <BaseEdge
        id={id}
        path={edgePath}
        style={{
          stroke: selected ? color : '#555',
          strokeWidth: selected ? 2.5 : 1.5,
          strokeDasharray: data?.enabled === false ? '5 5' : undefined,
        }}
      />
      {trigger && (
        <EdgeLabelRenderer>
          <div
            style={{
              position: 'absolute',
              transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)`,
              pointerEvents: 'all',
              fontSize: 10,
              background: 'var(--color-bg-elevated, #1e1e2e)',
              border: `1px solid ${color}`,
              borderRadius: 4,
              padding: '1px 6px',
              color,
              whiteSpace: 'nowrap',
            }}
            className="nodrag nopan"
          >
            {getTriggerLabel(trigger)} → {gateLabel || ''}
          </div>
        </EdgeLabelRenderer>
      )}
    </>
  );
}

/** Webhook 边：紫色虚线 */
export function WebhookEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  selected,
}: EdgeProps) {
  const [edgePath] = getBezierPath({
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
  });

  return (
    <BaseEdge
      id={id}
      path={edgePath}
      style={{
        stroke: selected ? '#9254de' : '#722ed1',
        strokeWidth: selected ? 2.5 : 1.5,
        strokeDasharray: '6 3',
      }}
    />
  );
}

/** 飞书消息边：蓝色点线 */
export function FeishuEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  selected,
}: EdgeProps) {
  const [edgePath] = getBezierPath({
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
  });

  return (
    <BaseEdge
      id={id}
      path={edgePath}
      style={{
        stroke: selected ? '#40a9ff' : '#1890ff',
        strokeWidth: selected ? 2.5 : 1.5,
        strokeDasharray: '3 3',
      }}
    />
  );
}

/** 调度器边：橙色虚线 */
export function SchedulerEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  selected,
}: EdgeProps) {
  const [edgePath] = getBezierPath({
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
  });

  return (
    <BaseEdge
      id={id}
      path={edgePath}
      style={{
        stroke: selected ? '#ffa940' : '#fa8c16',
        strokeWidth: selected ? 2.5 : 1.5,
        strokeDasharray: '8 4',
      }}
    />
  );
}
