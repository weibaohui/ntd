// 工艺模板只读流程图。
// 使用泛化的 useFlowLayout 做 dagre 自动排布，TemplateStepCard 渲染环节节点，
// 复用 FlowVirtualNodes 的 StartNode/EndNode 虚拟节点。
//
// 与 LoopFlowGraph（环路实例运行时流程图）的差异：
// - 节点无交互（模板不编辑）
// - 边渲染简单（无回环弧线，只有直连或正交折线）
// - 节点样式不同（TemplateStepCard vs FlowStepNode）

import {
  StartNode, EndNode,
} from '@/components/loop-flow/FlowVirtualNodes';
import { TemplateStepCard } from '@/components/process/TemplateStepCard';
import { useFlowLayout } from '@/components/loop-flow/useFlowLayout';
import type { FlowNodeInput, FlowEdgeInput } from '@/components/loop-flow/useFlowLayout';
import { phaseColor } from '@/components/loop-flow/useFlowLayout';
import { NODE_WIDTH, NODE_HEIGHT, START_NODE_ID, END_NODE_ID } from '@/components/loop-flow/flowConstants';
import type { AdaptedLink, TemplateEdge, PhaseGroup } from '@/components/process/processFlowAdapter';

/** 计算反向跳转弧线（源节点底部 → U 形弯 → 目标节点底部）。
 *  只用于 template 工艺流程图：goto 边不参与 dagre 布局，单独绘制弧线。 */
function buildBackArcPath(
  fx: number, fy: number, // 源节点底部中点
  tx: number, ty: number, // 目标节点底部中点
  bendY: number, // 弧线底部 Y（下探距离）
): string {
  return `M ${fx},${fy} C ${fx},${fy + bendY} ${tx},${ty + bendY} ${tx},${ty}`;
}

export interface ProcessFlowGraphProps {
  /** 适配后的链接列表。 */
  links: AdaptedLink[];
  /** dagre 布局用的节点输入。 */
  nodeInputs: FlowNodeInput[];
  /** dagre 布局用的边输入。 */
  edgeInputs: FlowEdgeInput[];
  /** 模板边（含标签，用于边标注）。 */
  templateEdges: TemplateEdge[];
  /** 阶段分组（供阶段标签绘制）。 */
  phaseGroups: PhaseGroup[];
}

export function ProcessFlowGraph({
  links, nodeInputs, edgeInputs, templateEdges, phaseGroups,
}: ProcessFlowGraphProps) {
  const { positions, width, height, startX, startY, endX, endY, dagreOffsetY } =
    useFlowLayout(nodeInputs, edgeInputs, false, false);

  // 阶段标签在首个链接上方约 22px，需要额外画布高度避免裁切
  const phaseLabelPad = 30;
  const svgHeight = height + phaseLabelPad;

  if (links.length === 0) {
    return (
      <div style={{ color: '#94a3b8', textAlign: 'center', padding: 60, fontSize: 13 }}>
        该工艺模板暂无环节定义
      </div>
    );
  }

  return (
    <div style={{ overflowX: 'auto', overflowY: 'hidden', padding: '12px 0', minHeight: 120 }}>
      <svg width={width + 40} height={svgHeight} style={{ display: 'block' }}>
        <g transform={`translate(0, ${dagreOffsetY + phaseLabelPad})`}>
          {/* 边 */}
          {templateEdges.map((te, i) => {
            const fromPos = te.fromNumericId === START_NODE_ID
              ? { x: startX, y: startY }
              : positions.get(te.fromNumericId) ?? { x: 0, y: 0 };
            const toPos = te.toNumericId === END_NODE_ID
              ? { x: endX, y: endY }
              : positions.get(te.toNumericId) ?? { x: 0, y: 0 };

            const fromCx = te.fromNumericId === START_NODE_ID
              ? fromPos.x + 20 : fromPos.x + NODE_WIDTH / 2;
            const fromCy = te.fromNumericId === START_NODE_ID
              ? fromPos.y + 20 : fromPos.y + NODE_HEIGHT / 2;
            const toCx = te.toNumericId === END_NODE_ID
              ? toPos.x + 20 : toPos.x + NODE_WIDTH / 2;
            const toCy = te.toNumericId === END_NODE_ID
              ? toPos.y + 20 : toPos.y + NODE_HEIGHT / 2;

            const midX = (fromCx + toCx) / 2;
            // goto 反向边判定：目标索引小于源索引（在大平序列中跳回前面的环节）
            const fromIndex = links.findIndex(l => l.numericId === te.fromNumericId);
            const toIndex = links.findIndex(l => l.numericId === te.toNumericId);
            const isGoto = (te.kind === 'fail-goto' || te.label.includes('goto'))
              && toIndex >= 0 && toIndex < fromIndex;

            if (isGoto) {
              // 反向跳转弧线：从源节点底部 U 形下探到目标节点底部
              const fromBottomX = fromCx;
              const fromBottomY = fromPos.y + NODE_HEIGHT;
              const toBottomX = toCx;
              const toBottomY = toPos.y + NODE_HEIGHT;
              const distance = Math.abs(fromBottomX - toBottomX);
              const bendY = Math.max(20, distance * 0.15);
              const path = buildBackArcPath(fromBottomX, fromBottomY, toBottomX, toBottomY, bendY);
              return (
                <g key={`pe-${i}`}>
                  <path
                    d={path}
                    fill="none"
                    stroke="#f59e0b"
                    strokeWidth={1.5}
                    strokeDasharray="6,3"
                  />
                  <text
                    x={midX} y={Math.max(fromBottomY, toBottomY) + bendY + 12}
                    textAnchor="middle" fontSize={9}
                    fill="#d97706"
                    style={{ fontFamily: 'system-ui' }}
                  >
                    门禁失败 ↺ {te.label.replace('门禁失败 ', '')}
                  </text>
                </g>
              );
            }

            // 普通顺向边（水平直线）
            return (
              <g key={`pe-${i}`}>
                <line
                  x1={fromCx} y1={fromCy}
                  x2={toCx} y2={toCy}
                  stroke="#94a3b8"
                  strokeWidth={1}
                />
                {te.label && (
                  <text
                    x={midX} y={(fromCy + toCy) / 2 - 8}
                    textAnchor="middle"
                    fontSize={9}
                    fill="#64748b"
                    style={{ fontFamily: 'system-ui' }}
                  >
                    {te.label}
                  </text>
                )}
              </g>
            );
          })}

          {/* 虚拟节点 */}
          <StartNode x={startX} y={startY} />
          <EndNode x={endX} y={endY} />

          {/* 阶段标签（每阶段的第一个环节上方，彩色胶囊条 + 阶段名） */}
          {phaseGroups.map((pg, pi) => {
            const firstLink = links[pg.startIndex];
            if (!firstLink) return null;
            const pos = positions.get(firstLink.numericId) ?? { x: 0, y: 0 };
            const color = phaseColor(pg.startIndex * 7 + 3);
            return (
              <g key={`ph-${pg.startIndex}`}>
                {/* 淡色背景横条 */}
                <rect
                  x={pos.x - 2} y={pos.y - 22}
                  width={NODE_WIDTH + 4} height={18}
                  rx={4} ry={4}
                  fill={color} fillOpacity={0.1}
                  stroke={color} strokeWidth={0.5}
                />
                <text
                  x={pos.x + NODE_WIDTH / 2} y={pos.y - 10}
                  textAnchor="middle"
                  fontSize={10} fontWeight={600}
                  fill={color}
                  style={{ fontFamily: 'system-ui' }}
                >
                  ▸ {pg.phaseName}
                </text>
                {/* 阶段间分隔竖线（非首个阶段） */}
                {pi > 0 && (() => {
                  const prevEnd = phaseGroups[pi - 1];
                  const prevLink = links[Math.min(prevEnd.endIndex - 1, links.length - 1)];
                  if (!prevLink) return null;
                  const pp = positions.get(prevLink.numericId) ?? { x: 0, y: 0 };
                  const dividerX = pp.x + NODE_WIDTH + 16;
                  return (
                    <line
                      x1={dividerX} y1={pos.y - 30}
                      x2={dividerX} y2={pos.y + NODE_HEIGHT + 4}
                      stroke={color} strokeWidth={1}
                      strokeDasharray="4,4"
                      opacity={0.3}
                    />
                  );
                })()}
              </g>
            );
          })}

          {/* 环节节点卡片 */}
          {links.map((link) => {
            const pos = positions.get(link.numericId) ?? { x: 0, y: 0 };
            return (
              <TemplateStepCard
                key={`tsc-${link.numericId}`}
                link={link}
                x={pos.x}
                y={pos.y}
              />
            );
          })}
        </g>
      </svg>
    </div>
  );
}
