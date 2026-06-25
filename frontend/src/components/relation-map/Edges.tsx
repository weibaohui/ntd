import {
  BaseEdge,
  getBezierPath,
  type EdgeProps,
} from '@xyflow/react';

// todo hook 已整块移除（plan `purring-forging-petal`），不再导出 HookEdge。
// 关联图保留 feishu / scheduler 两类边。

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
