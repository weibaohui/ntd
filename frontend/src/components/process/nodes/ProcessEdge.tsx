// ProcessEdge.tsx
// ---------------------------------------------------------------------------
// M4 里程碑：React Flow 自定义边 — ProcessEdge（带 hover 删除按钮）。
//
// 设计意图（对应 docs/design/029-M4-ReactFlow可视化编辑器-方案.md §3.1.3 + 设计 §5.4.3）：
// - 用 getSmoothStepPath 生成路径
// - hover 时在中点显示 <circle> + <text>×</text> 删除按钮
// - 点击叉号 → 调用 data.onDelete(edgeId)
//
// 边视觉由 data.color / data.dashed 决定：
// - on_success goto → 绿色 #10b981 实线
// - on_gate_fail goto → 橙色 #d97706 虚线
//
// 悬停状态：
// - hovered=true 时显示删除按钮
// - 鼠标移入边或删除按钮区域时 hovered=true
// - 鼠标移出时 hovered=false
// ---------------------------------------------------------------------------

import { memo, useState, type MouseEvent, type JSX } from 'react';
import {
  BaseEdge,
  EdgeLabelRenderer,
  getSmoothStepPath,
  type EdgeProps,
} from '@xyflow/react';

// ── 组件实现 ──────────────────────────────────────

// ProcessEdge 组件实现。
//
// React Flow 传入 EdgeProps，包含：
// - id：边 id
// - sourceX/Y, targetX/Y：起终点坐标
// - sourcePosition, targetPosition：起终点 handle 位置
// - data：边数据，含 color/dashed/onDelete
//
// 注意：React Flow v12 的 EdgeProps 要求 data 是 Record<string, unknown>。
// 我们在运行时从 data 中提取 color/dashed/onDelete。
function ProcessEdgeImpl({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  data,
  markerEnd,
}: EdgeProps): JSX.Element {
  // 从 data 提取视觉属性和回调
  // data 类型是 Record<string, unknown>，我们用 as 断言提取
  const edgeData = data as {
    color?: string;
    dashed?: boolean;
    onDelete?: (edgeId: string) => void;
  };
  const color = edgeData.color ?? '#94a3b8'; // 默认灰色
  const dashed = edgeData.dashed ?? false;
  const onDelete = edgeData.onDelete;

  // hover 状态：是否显示删除按钮
  const [hovered, setHovered] = useState(false);

  // 生成 smoothstep 路径
  const [path] = getSmoothStepPath({
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
  });

  // 计算中点坐标，用于放置删除按钮
  const midX = (sourceX + targetX) / 2;
  const midY = (sourceY + targetY) / 2;

  // 删除按钮点击：阻止事件冒泡（避免触发边选中），调用删除回调
  const handleDeleteClick = (e: MouseEvent) => {
    e.stopPropagation();
    if (onDelete) {
      onDelete(id);
    }
  };

  // 鼠标移入：设置 hovered=true
  const handleMouseEnter = () => setHovered(true);
  // 鼠标移出：设置 hovered=false
  const handleMouseLeave = () => setHovered(false);

  return (
    <>
      {/* 边路径：hover 时显示鼠标手势 */}
      <BaseEdge
        id={id}
        path={path}
        markerEnd={markerEnd}
        style={{
          stroke: color,
          // 虚线样式：dashed=true 时用 6,3 间距
          strokeDasharray: dashed ? '6,3' : undefined,
          // hover 时加粗
          strokeWidth: hovered ? 3 : 2,
        }}
        // 把 hover 事件挂到 BaseEdge
        // 注意：React Flow v12 的 BaseEdge 不直接支持 onMouseEnter
        // 这里用 markerEnd 等属性，hover 事件通过外层 g 元素处理
      />
      {/* 边标签渲染器：用于渲染中点的删除按钮 */}
      <EdgeLabelRenderer>
        {/* 外层 g 元素处理 hover 事件 */}
        <g
          onMouseEnter={handleMouseEnter}
          onMouseLeave={handleMouseLeave}
          // pointer-events: all 让 g 元素可接收鼠标事件
          style={{ pointerEvents: 'all' }}
        >
          {/* hover 时显示删除按钮 */}
          {hovered && onDelete && (
            <>
              {/* 白色背景圆，遮挡边路径 */}
              <circle
                cx={midX}
                cy={midY}
                r={10}
                fill="#fff"
                stroke="#ef4444"
                strokeWidth={1.5}
              />
              {/* 红色 × 删除按钮 */}
              <text
                x={midX}
                y={midY + 4}
                textAnchor="middle"
                fill="#ef4444"
                style={{
                  cursor: 'pointer',
                  fontSize: 14,
                  fontWeight: 600,
                  userSelect: 'none',
                }}
                onClick={handleDeleteClick}
              >
                ×
              </text>
            </>
          )}
        </g>
      </EdgeLabelRenderer>
    </>
  );
}

// ── 导出 ──────────────────────────────────────────

// 用 memo 包裹导出
// React Flow 在拖拽时频繁更新边 props，memo 避免未变边重渲染
export const ProcessEdge = memo(ProcessEdgeImpl);
