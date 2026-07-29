// 工艺模板只读流程图。
// 使用泛化的 useFlowLayout 做 dagre 自动排布，TemplateStepCard 渲染环节节点，
// 复用 FlowVirtualNodes 的 StartNode/EndNode 虚拟节点。
//
// 边渲染学习 FlowEdge 的三要素：
// 1) S 形贝塞尔曲线（锚点：源右边中点 → 目标左边中点）
// 2) SVG <marker> 三角箭头
// 3) goto 反向弧线用正交折线（下 → 左 → 上），参照 FlowEdge 回环模式

import {
  StartNode, EndNode,
} from '@/components/loop-flow/FlowVirtualNodes';
import { TemplateStepCard } from '@/components/process/TemplateStepCard';
import { useFlowLayout } from '@/components/loop-flow/useFlowLayout';
import type { FlowNodeInput, FlowEdgeInput } from '@/components/loop-flow/useFlowLayout';
import { phaseColor } from '@/components/loop-flow/useFlowLayout';
import { NODE_WIDTH, NODE_HEIGHT, START_NODE_ID, END_NODE_ID, VIRTUAL_NODE_RADIUS } from '@/components/loop-flow/flowConstants';
import type { AdaptedLink, TemplateEdge, PhaseGroup } from '@/components/process/processFlowAdapter';

/** 边锚点坐标 */
interface Anchor {
  x: number;
  y: number;
}

/** 从 positions 取出节点左上角位置，再计算指定侧边的中点锚点。 */
function getAnchor(
  nodeId: number,
  side: 'right' | 'left' | 'bottom',
  positions: Map<number, { x: number; y: number }>,
  startX: number, startY: number, endX: number, endY: number,
): Anchor | null {
  if (nodeId === START_NODE_ID) {
    return { x: startX + (side === 'right' ? VIRTUAL_NODE_RADIUS : -VIRTUAL_NODE_RADIUS), y: startY };
  }
  if (nodeId === END_NODE_ID) {
    return { x: endX + (side === 'right' ? VIRTUAL_NODE_RADIUS : -VIRTUAL_NODE_RADIUS), y: endY };
  }
  const pos = positions.get(nodeId);
  if (!pos) return null;
  if (side === 'bottom') {
    return { x: pos.x + NODE_WIDTH / 2, y: pos.y + NODE_HEIGHT };
  }
  return {
    x: side === 'right' ? pos.x + NODE_WIDTH : pos.x,
    y: pos.y + NODE_HEIGHT / 2,
  };
}

/** 顺向边 S 形贝塞尔曲线。 */
function buildSCurve(from: Anchor, to: Anchor): string {
  const dx = Math.abs(to.x - from.x);
  const cx1 = from.x + dx * 0.4;
  const cx2 = to.x - dx * 0.4;
  return `M ${from.x} ${from.y} C ${cx1} ${from.y}, ${cx2} ${to.y}, ${to.x} ${to.y}`;
}

/** goto 反向弧线：正交折线（源底边中点↓ → 水平走 → 目标底边中点↑）。 */
function buildGotoPath(from: Anchor, to: Anchor, offsetY: number): string {
  return `M ${from.x} ${from.y} V ${from.y + offsetY} H ${to.x} V ${to.y}`;
}

/** 按字符粗略估算标签宽度（中文约 10px、ASCII 约 6px）。 */
function labelWidth(text: string): number {
  let w = 0;
  for (const ch of text) {
    w += ch.charCodeAt(0) > 0x7F ? 10 : 6;
  }
  return w;
}

export interface ProcessFlowGraphProps {
  links: AdaptedLink[];
  nodeInputs: FlowNodeInput[];
  edgeInputs: FlowEdgeInput[];
  templateEdges: TemplateEdge[];
  phaseGroups: PhaseGroup[];
}

export function ProcessFlowGraph({
  links, nodeInputs, edgeInputs, templateEdges, phaseGroups,
}: ProcessFlowGraphProps) {
  const { positions, width, height, startX, startY, endX, endY, dagreOffsetY } =
    useFlowLayout(nodeInputs, edgeInputs, false, false);

  const phaseLabelPad = 30;
  const svgHeight = height + phaseLabelPad;

  if (links.length === 0) {
    return (
      // 主题三级文字色，暗色下不再用写死的 slate-400
      <div style={{ color: 'var(--color-text-tertiary)', textAlign: 'center', padding: 60, fontSize: 13 }}>
        该工艺模板暂无环节定义
      </div>
    );
  }

  return (
    <div style={{ overflowX: 'auto', overflowY: 'hidden', padding: '12px 0', minHeight: 120 }}>
      <svg width={width + 40} height={svgHeight} style={{ display: 'block' }}>
        {/* arrow marker 只需定义一次，按 index 命名复用 */}
        <defs>
          {templateEdges.map((te, i) => {
            const isGoto = (te.kind === 'fail-goto' || te.label.includes('goto'))
              && (() => {
                const fi = links.findIndex(l => l.numericId === te.fromNumericId);
                const ti = links.findIndex(l => l.numericId === te.toNumericId);
                return ti >= 0 && ti < fi;
              })();
            return (
              <marker
                key={`am-${i}`}
                id={`parrow-${i}`}
                viewBox="0 0 10 10" refX={10} refY={5}
                markerWidth={6} markerHeight={6} orient="auto"
              >
                {/* goto 橙是语义色两种主题通用；顺向灰用主题变量随主题切换 */}
                <path d="M 0 0 L 10 5 L 0 10 z" fill={isGoto ? '#d97706' : 'var(--color-text-tertiary)'} />
              </marker>
            );
          })}
        </defs>

        <g transform={`translate(0, ${dagreOffsetY + phaseLabelPad})`}>
          {/* 边 */}
          {templateEdges.map((te, i) => {
            const fromAnchor = getAnchor(te.fromNumericId, 'right', positions, startX, startY, endX, endY);
            const toAnchor = getAnchor(te.toNumericId, 'left', positions, startX, startY, endX, endY);
            if (!fromAnchor || !toAnchor) return null;

            // goto 反向边判定
            const fromIndex = links.findIndex(l => l.numericId === te.fromNumericId);
            const toIndex = links.findIndex(l => l.numericId === te.toNumericId);
            const isGoto = (te.kind === 'fail-goto' || te.label.includes('goto'))
              && toIndex >= 0 && toIndex < fromIndex;

            if (isGoto) {
              // ── goto 反向折线（底边锚点 + 正交下探） ──
              const fb = getAnchor(te.fromNumericId, 'bottom', positions, startX, startY, endX, endY);
              const tb = getAnchor(te.toNumericId, 'bottom', positions, startX, startY, endX, endY);
              if (!fb || !tb) return null;
              const dist = Math.abs(fb.x - tb.x);
              const offsetY = Math.max(18, dist * 0.12);
              const path = buildGotoPath(fb, tb, offsetY);
              const midX = (fb.x + tb.x) / 2;
              const lbl = te.label.replace('门禁失败 ', '');
              const lw = labelWidth(lbl);
              const labelY = fb.y + offsetY;
              return (
                <g key={`pe-${i}`}>
                  <path
                    d={path} fill="none" stroke="#d97706"
                    strokeWidth={1.5} strokeDasharray="6,3"
                    markerEnd={`url(#parrow-${i})`}
                  />
                  {/* 标签圆角矩形：底色用主题容器色而非写死白色，暗色下不刺眼 */}
                  <rect
                    x={midX - lw / 2 - 6} y={labelY - 10}
                    width={lw + 12} height={18} rx={4}
                    fill="var(--color-bg-elevated)" stroke="#d97706" strokeWidth={1}
                  />
                  <text
                    x={midX} y={labelY + 2}
                    textAnchor="middle" fontSize={10} fontWeight={600}
                    fill="#d97706" style={{ fontFamily: 'system-ui' }}
                  >
                    {lbl}
                  </text>
                </g>
              );
            }

            // ── 顺向边（S 形贝塞尔 + 箭头） ──
            const path = buildSCurve(fromAnchor, toAnchor);
            const midX = (fromAnchor.x + toAnchor.x) / 2;
            const midY = (fromAnchor.y + toAnchor.y) / 2;
            return (
              <g key={`pe-${i}`}>
                <path
                  d={path} fill="none" stroke="var(--color-text-tertiary)"
                  strokeWidth={1.5} markerEnd={`url(#parrow-${i})`}
                />
                {te.label && (
                  <text
                    x={midX} y={midY - 8}
                    textAnchor="middle" fontSize={10}
                    fill="var(--color-text-secondary)"
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

          {/* 阶段标签 */}
          {phaseGroups.map((pg, pi) => {
            const firstLink = links[pg.startIndex];
            if (!firstLink) return null;
            const pos = positions.get(firstLink.numericId) ?? { x: 0, y: 0 };
            const color = phaseColor(pg.startIndex * 7 + 3);
            return (
              <g key={`ph-${pg.startIndex}`}>
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
