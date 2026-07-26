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
import { NODE_WIDTH, NODE_HEIGHT, START_NODE_ID, END_NODE_ID } from '@/components/loop-flow/flowConstants';
import type { AdaptedLink, TemplateEdge } from '@/components/process/processFlowAdapter';

export interface ProcessFlowGraphProps {
  /** 适配后的链接列表。 */
  links: AdaptedLink[];
  /** dagre 布局用的节点输入。 */
  nodeInputs: FlowNodeInput[];
  /** dagre 布局用的边输入。 */
  edgeInputs: FlowEdgeInput[];
  /** 模板边（含标签，用于边标注）。 */
  templateEdges: TemplateEdge[];
}

export function ProcessFlowGraph({
  links, nodeInputs, edgeInputs, templateEdges,
}: ProcessFlowGraphProps) {
  const { positions, width, height, startX, startY, endX, endY, dagreOffsetY } =
    useFlowLayout(nodeInputs, edgeInputs, false, false);

  if (links.length === 0) {
    return (
      <div style={{ color: '#94a3b8', textAlign: 'center', padding: 60, fontSize: 13 }}>
        该工艺模板暂无环节定义
      </div>
    );
  }

  return (
    <div style={{ overflowX: 'auto', overflowY: 'hidden', padding: '12px 0', minHeight: 120 }}>
      <svg width={width} height={height} style={{ display: 'block' }}>
        <g transform={`translate(0, ${dagreOffsetY})`}>
          {/* 边 */}
          {templateEdges.map((te, i) => {
            const fromPos = te.fromNumericId === START_NODE_ID
              ? { x: startX, y: startY }
              : positions.get(te.fromNumericId) ?? { x: 0, y: 0 };
            const toPos = te.toNumericId === END_NODE_ID
              ? { x: endX, y: endY }
              : positions.get(te.toNumericId) ?? { x: 0, y: 0 };

            // 计算中心点
            const fromCx = te.fromNumericId === START_NODE_ID
              ? fromPos.x + 20 : fromPos.x + NODE_WIDTH / 2;
            const fromCy = te.fromNumericId === START_NODE_ID
              ? fromPos.y + 20 : fromPos.y + NODE_HEIGHT / 2;
            const toCx = te.toNumericId === END_NODE_ID
              ? toPos.x + 20 : toPos.x + NODE_WIDTH / 2;
            const toCy = te.toNumericId === END_NODE_ID
              ? toPos.y + 20 : toPos.y + NODE_HEIGHT / 2;

            // 简单水平连线（所有环节 LR 排布）
            const midX = (fromCx + toCx) / 2;
            const isGoto = te.kind === 'fail-goto' || te.label.includes('goto');

            return (
              <g key={`pe-${i}`}>
                <line
                  x1={fromCx} y1={fromCy}
                  x2={toCx} y2={toCy}
                  stroke={isGoto ? '#f59e0b' : '#94a3b8'}
                  strokeWidth={isGoto ? 1.5 : 1}
                  strokeDasharray={isGoto ? '6,3' : undefined}
                />
                {te.label && (
                  <text
                    x={midX} y={(fromCy + toCy) / 2 - 8}
                    textAnchor="middle"
                    fontSize={9}
                    fill={isGoto ? '#d97706' : '#64748b'}
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
