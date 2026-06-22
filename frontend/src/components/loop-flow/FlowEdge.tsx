// Loop 流程图边渲染 + 路径/中点计算。
//
// 边分三类：
// 1) 普通前向边（success-next / success-goto / fail-skip / fail-goto / fail-break / end）：
//    横向 S 形贝塞尔曲线，按 EDGE_STYLES 着色。
// 2) 回环边（isLoopBack=true）：目标 step 排在源 step 之前的 goto。
//    用 U 形向上拱的曲线 + 加重样式 + 白底加粗标签，从普通边里跳出来。
// 3) 自环边（isSelfLoop=true）：goto-self（重试），从环节右下角出、左下角入，
//    底部走 U 形折线，橙色/红色标识。
//
// 之所以独立成文件：让 LoopFlowGraph 主文件保持在 500 行硬限内，
// 边相关的样式/路径/标签测量都是独立关注点。

import type { LoopStepDto } from '@/types/loop';
import {
  START_NODE_ID, END_NODE_ID, VIRTUAL_NODE_RADIUS,
  NODE_WIDTH, NODE_HEIGHT, LOOP_BACK_TOP_PADDING, SELF_LOOP_GAP,
} from '@/components/loop-flow/flowConstants';
import type { LayoutNode, LayoutEdge, EdgeType } from '@/components/loop-flow/flowTypes';

const EDGE_STYLES: Record<EdgeType, { color: string; dash: string; labelColor: string }> = {
  'start-first':  { color: '#94a3b8', dash: '', labelColor: '#94a3b8' },
  'success-next': { color: '#94a3b8', dash: '', labelColor: '#94a3b8' },
  'success-goto': { color: '#22c55e', dash: '', labelColor: '#16a34a' },
  'fail-skip':    { color: '#f97316', dash: '5,3', labelColor: '#ea580c' },
  'fail-goto':    { color: '#ef4444', dash: '', labelColor: '#dc2626' },
  'fail-break':   { color: '#ef4444', dash: '', labelColor: '#dc2626' },
  'end':          { color: '#94a3b8', dash: '', labelColor: '#94a3b8' },
};

export function classifyEdge(
  _step: LoopStepDto,
  _allSteps: LoopStepDto[],
  policy: string,
  _gotoId: number | null,
  isSuccess: boolean,
): EdgeType {
  if (policy === 'end') return 'end';
  if (isSuccess) {
    if (policy === 'goto') return 'success-goto';
    return 'success-next';
  }
  // failure edges
  switch (policy) {
    case 'skip': return 'fail-skip';
    case 'goto': return 'fail-goto';
    case 'break': return 'fail-break';
    default: return 'fail-break';
  }
}

export function resolveTargetStep(
  step: LoopStepDto,
  allSteps: LoopStepDto[],
  policy: string,
  gotoId: number | null,
): number | undefined {
  if (policy === 'next' || policy === 'skip') {
    const idx = allSteps.findIndex(s => s.id === step.id);
    if (idx >= 0 && idx < allSteps.length - 1) {
      return allSteps[idx + 1].id;
    }
    return undefined;
  }
  if (policy === 'goto' && gotoId != null) {
    return gotoId;
  }
  return undefined;
}

// 边的一端锚点：真实环节用矩形边的中点；虚拟节点用圆周上的对应方向点。
// 真实环节的左/右中点与圆节点的水平切点对齐，让 Start→first / last→End
// 这两条边的起止点在视觉上和「同高度的中间」自然衔接。
// side='top' 只用于回环边：起止都在环节顶边中点，正交折线从顶边出来
// 视觉上更顺（不与环节卡片的左右中线争夺起止点）。
function getEdgeAnchor(
  nodeId: number, nodes: LayoutNode[],
  startX: number, startY: number, endX: number, endY: number,
  side: 'left' | 'right' | 'top',
): { x: number; y: number } | null {
  if (nodeId === START_NODE_ID) {
    if (side === 'top') return null;
    return { x: startX + (side === 'right' ? VIRTUAL_NODE_RADIUS : -VIRTUAL_NODE_RADIUS), y: startY };
  }
  if (nodeId === END_NODE_ID) {
    if (side === 'top') return null;
    return { x: endX + (side === 'right' ? VIRTUAL_NODE_RADIUS : -VIRTUAL_NODE_RADIUS), y: endY };
  }
  const node = nodes.find(n => n.id === nodeId);
  if (!node) return null;
  if (side === 'top') {
    return { x: node.x + NODE_WIDTH / 2, y: node.y };
  }
  return {
    x: side === 'right' ? node.x + NODE_WIDTH : node.x,
    y: node.y + NODE_HEIGHT / 2,
  };
}

export function buildEdgePath(
  edge: LayoutEdge, nodes: LayoutNode[],
  startX: number, startY: number, endX: number, endY: number,
): string {
  if (edge.isSelfLoop) {
    // 自环（goto-self）：从环节右侧偏下位置出发，向下走 SELF_LOOP_GAP，
    // 水平横穿到环节左侧，再向上回到左侧偏下位置。
    // 路径：右→下→左→上，形成底部折线。
    const node = nodes.find(n => n.id === edge.fromId);
    if (!node) return '';
    const exitX = node.x + NODE_WIDTH;
    const exitY = node.y + NODE_HEIGHT * 0.60;
    const entryX = node.x;
    const entryY = node.y + NODE_HEIGHT * 0.60;
    const bottomY = node.y + NODE_HEIGHT + SELF_LOOP_GAP;
    return `M ${exitX} ${exitY} V ${bottomY} H ${entryX} V ${entryY}`;
  }

  const from = getEdgeAnchor(edge.fromId, nodes, startX, startY, endX, endY, 'right');
  const to = getEdgeAnchor(edge.toId, nodes, startX, startY, endX, endY, 'left');
  if (!from || !to) return '';

  if (edge.isLoopBack) {
    // 回环：3 段正交折线（up → left → down），从源 step 顶边中点出发，
    // 上升到 baseY - H 后水平走到目标 step 顶边中点上方，再下到 to。
    // 起点/终点都在顶边而不是 right/left 中线，跟用户期望的「从顶部出、
    // 从顶部入」一致；虚拟节点没有顶边概念，自动回退到 right/left 旧路径。
    const fromTop = getEdgeAnchor(edge.fromId, nodes, startX, startY, endX, endY, 'top');
    const toTop = getEdgeAnchor(edge.toId, nodes, startX, startY, endX, endY, 'top');
    if (fromTop && toTop) {
      const H = LOOP_BACK_TOP_PADDING;
      const baseY = Math.min(fromTop.y, toTop.y);
      return `M ${fromTop.x} ${fromTop.y} V ${baseY - H} H ${toTop.x} V ${toTop.y}`;
    }
    // 起点或终点是虚拟节点（START/END），回退到 right/left 锚点的旧版本。
    const H = LOOP_BACK_TOP_PADDING;
    const baseY = Math.min(from.y, to.y);
    return `M ${from.x} ${from.y} V ${baseY - H} H ${to.x} V ${to.y}`;
  }

  const dx = Math.abs(to.x - from.x);
  const cx1 = from.x + dx * 0.4;
  const cx2 = to.x - dx * 0.4;

  return `M ${from.x} ${from.y} C ${cx1} ${from.y}, ${cx2} ${to.y}, ${to.x} ${to.y}`;
}

export function getEdgeMidX(
  edge: LayoutEdge, nodes: LayoutNode[],
  startX: number, endX: number,
): number {
  // 自环标签放在底部水平线段的中点
  if (edge.isSelfLoop) {
    const node = nodes.find(n => n.id === edge.fromId);
    if (node) return node.x + NODE_WIDTH / 2;
    return 0;
  }
  // 回环用 top 锚点的 x（顶边中点），跟路径的水平段两端对齐。
  if (edge.isLoopBack) {
    const fromTop = getEdgeAnchor(edge.fromId, nodes, startX, 0, endX, 0, 'top');
    const toTop = getEdgeAnchor(edge.toId, nodes, startX, 0, endX, 0, 'top');
    if (fromTop && toTop) return (fromTop.x + toTop.x) / 2;
  }
  const from = getEdgeAnchor(edge.fromId, nodes, startX, 0, endX, 0, 'right');
  const to = getEdgeAnchor(edge.toId, nodes, startX, 0, endX, 0, 'left');
  if (!from || !to) return 0;
  return (from.x + to.x) / 2;
}

export function getEdgeMidY(
  edge: LayoutEdge, nodes: LayoutNode[],
  startY: number, endY: number,
): number {
  const from = getEdgeAnchor(edge.fromId, nodes, 0, startY, 0, endY, 'right');
  const to = getEdgeAnchor(edge.toId, nodes, 0, startY, 0, endY, 'left');
  if (!from || !to) return 0;
  // 自环标签：放在底部折线的水平段上方，用白底矩形包住。
  if (edge.isSelfLoop) {
    const node = nodes.find(n => n.id === edge.fromId);
    if (node) {
      const bottomY = node.y + NODE_HEIGHT + SELF_LOOP_GAP;
      return bottomY - 6;
    }
    return 0;
  }
  // 回环标签：水平折线段在 y=baseY-H，标签 y 设为 baseY-H-11 让白底矩形
  // 底边离折线 4px（rect 高度 16，y=midY-9 → 底边=midY+7），不会盖住折线。
  // 用 top 锚点的 y（节点顶边）而不是 right 锚点的 y（垂直中点），因为
  // 回环的水平段在两个 step 顶边之上的 baseY-H 高度。
  if (edge.isLoopBack) {
    const fromTop = getEdgeAnchor(edge.fromId, nodes, 0, startY, 0, endY, 'top');
    const toTop = getEdgeAnchor(edge.toId, nodes, 0, startY, 0, endY, 'top');
    const baseY = Math.min(fromTop?.y ?? from.y, toTop?.y ?? to.y);
    return baseY - LOOP_BACK_TOP_PADDING - 11;
  }
  return (from.y + to.y) / 2;
}

// 按可视宽度粗略估算标签宽度（中文字符 10px，ASCII 字符 6px，加 1px 字间距），
// 用于给回环标签加白底。SVG 文本测量在 React 里成本较高，这里用近似。
function getLabelWidth(text: string): number {
  let w = 0;
  for (const ch of text) {
    w += ch.charCodeAt(0) > 0x7F ? 10 : 6;
  }
  return w;
}

interface FlowEdgeProps {
  edge: LayoutEdge;
  index: number;
  nodes: LayoutNode[];
  startX: number;
  startY: number;
  endX: number;
  endY: number;
}

// 单条边的渲染：箭头线 + 标签。回环边用白底圆角矩形包标签，
// 让弧顶处的文字跟红色加粗虚线一起成为「回头重做」的强信号。
// 自环边用底部折线，同样白底标签放在水平段上方。
export function FlowEdge({
  edge, index, nodes, startX, startY, endX, endY,
}: FlowEdgeProps) {
  const baseStyle = EDGE_STYLES[edge.type] || EDGE_STYLES['success-next'];
  // 回环是带条件的执行路径，与普通跳转同属主路径，不该用虚线暗示「次要」。
  // 区分度只靠颜色加深（success 深绿、fail 深红）和更粗的 stroke 即可。
  const style = edge.isLoopBack ? {
    color: edge.type === 'success-goto' ? '#15803d' : '#b91c1c',
    dash: '',
    labelColor: edge.type === 'success-goto' ? '#15803d' : '#b91c1c',
  } : baseStyle;
  // 自环边：成功自重用橙色（代表重试），失败自重用深红。
  const selfLoopStyle = edge.isSelfLoop ? {
    color: edge.type === 'success-goto' ? '#d97706' : '#b91c1c',
    dash: '',
    labelColor: edge.type === 'success-goto' ? '#d97706' : '#b91c1c',
  } : null;
  const activeStyle = selfLoopStyle || style;
  // 回环边：stroke 比普通边略粗（1.8 vs 1.5）即可，颜色已经够深无需再叠粗细差。
  const strokeWidth = edge.isLoopBack || edge.isSelfLoop ? 1.8 : 1.5;
  const markerSize = 6;
  const labelW = edge.label ? getLabelWidth(edge.label) : 0;
  const midX = getEdgeMidX(edge, nodes, startX, endX);
  const midY = getEdgeMidY(edge, nodes, startY, endY);

  return (
    <g>
      <defs>
        <marker
          id={`arrow-${index}`}
          viewBox="0 0 10 10" refX="10" refY="5"
          markerWidth={markerSize} markerHeight={markerSize} orient="auto"
        >
          <path d="M 0 0 L 10 5 L 0 10 z" fill={activeStyle.color} />
        </marker>
      </defs>
      <path
        d={buildEdgePath(edge, nodes, startX, startY, endX, endY)}
        fill="none"
        stroke={activeStyle.color}
        strokeWidth={strokeWidth}
        strokeDasharray={activeStyle.dash || undefined}
        markerEnd={`url(#arrow-${index})`}
      />
      {edge.label && (
        edge.isLoopBack || edge.isSelfLoop ? (
          <g>
            <rect
              x={midX - labelW / 2 - 4} y={midY - 9}
              width={labelW + 8} height={16} rx={4}
              fill="#ffffff" stroke={activeStyle.color} strokeWidth={1}
            />
            <text
              x={midX} y={midY + 2}
              textAnchor="middle" fontSize={10} fontWeight={700}
              fill={activeStyle.labelColor}
              style={{ fontFamily: 'system-ui' }}
            >
              {edge.label}
            </text>
          </g>
        ) : (
          <text
            x={midX} y={midY - 6}
            textAnchor="middle" fontSize={10}
            fill={activeStyle.labelColor}
            style={{ fontFamily: 'monospace' }}
          >
            {edge.label}
          </text>
        )
      )}
    </g>
  );
}
