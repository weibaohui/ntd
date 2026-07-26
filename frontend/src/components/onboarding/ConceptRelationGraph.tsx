// 概念关系图：纯 SVG 手绘 + 节点点击弹 Drawer + hover 高亮 + 流动动画。
// 不引入 reactflow 重依赖（节点固定 7 个，手布局即可）。
// 尊重 prefers-reduced-motion：动画降级为静态高亮。

import { useState, useEffect, useRef } from 'react';
import { Drawer, Button, Typography, Descriptions, Empty } from 'antd';
import { ArrowRightOutlined } from '@ant-design/icons';
import { useViewState } from '@/hooks/useViewState';
import {
  CONCEPTS,
  GRAPH_EDGES,
  GRAPH_NODES,
  type ConceptNode,
  type GraphNode,
} from '@/components/onboarding/concepts';

const { Text, Title } = Typography;

/** 检测 prefers-reduced-motion 用户偏好。 */
function usePrefersReducedMotion(): boolean {
  // 默认 false（开启动画）；只在用户系统设置 reduce 时关闭。
  const [reduce, setReduce] = useState(false);
  useEffect(() => {
    const mq = window.matchMedia('(prefers-reduced-motion: reduce)');
    const update = () => setReduce(mq.matches);
    update();
    // 监听变化：用户切换系统设置时实时响应。
    mq.addEventListener('change', update);
    return () => mq.removeEventListener('change', update);
  }, []);
  return reduce;
}

/** 取两节点中心点连线坐标（用于 SVG path）。 */
function edgePath(from: GraphNode, to: GraphNode): string {
  // 节点半径 40，连线从边缘出发而非中心，视觉更干净。
  const r = 40;
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const dist = Math.sqrt(dx * dx + dy * dy);
  // 单位向量 × 半径 = 边缘偏移。
  const ux = dx / dist;
  const uy = dy / dist;
  const x1 = from.x + ux * r;
  const y1 = from.y + uy * r;
  const x2 = to.x - ux * r;
  const y2 = to.y - uy * r;
  return `M ${x1} ${y1} L ${x2} ${y2}`;
}

/** 单条连线，带流动动画或降级静态。 */
function Edge({
  from,
  to,
  active,
  reduceMotion,
}: {
  from: GraphNode;
  to: GraphNode;
  active: boolean;
  reduceMotion: boolean;
}) {
  // active = hover 命中时连线加粗高亮。
  // 流动动画用 stroke-dasharray + stroke-dashoffset keyframes，由 CSS class 驱动。
  return (
    <path
      d={edgePath(from, to)}
      stroke={active ? '#1677ff' : '#d9d9d9'}
      strokeWidth={active ? 2.5 : 1.5}
      fill="none"
      className={reduceMotion ? undefined : 'ntd-onboarding-edge-flow'}
      style={{
        // reduced-motion 时无动画，但仍可加粗高亮。
        transition: 'stroke 0.2s ease, stroke-width 0.2s ease',
      }}
    />
  );
}

/** 单个节点圆形，hover 高亮 + 点击触发 Drawer。 */
function GraphNodeCircle({
  node,
  hovered,
  onHover,
  onLeave,
  onClick,
}: {
  node: GraphNode;
  hovered: string | null;
  onHover: (id: string) => void;
  onLeave: () => void;
  onClick: (node: GraphNode) => void;
}) {
  // 命中条件：当前 hover 是自己，或自己在该节点的 highlights 列表里。
  const isActive = hovered === node.id || (hovered !== null && node.highlights.includes(hovered));
  const fill = isActive ? '#1677ff' : '#fff';
  const stroke = isActive ? '#1677ff' : '#d9d9d9';
  const textColor = isActive ? '#fff' : '#333';
  return (
    <g
      // SVG <g> 无原生 onClick 事件冒泡问题，直接挂。
      onClick={() => onClick(node)}
      onMouseEnter={() => onHover(node.id)}
      onMouseLeave={onLeave}
      style={{ cursor: 'pointer' }}
      data-testid={`onboarding-graph-node-${node.id}`}
    >
      {/* 圆形主体 */}
      <circle
        cx={node.x}
        cy={node.y}
        r={40}
        fill={fill}
        stroke={stroke}
        strokeWidth={2}
        style={{ transition: 'fill 0.2s ease, stroke 0.2s ease' }}
      />
      {/* 标题文本居中 */}
      <text
        x={node.x}
        y={node.y}
        textAnchor="middle"
        dominantBaseline="middle"
        fill={textColor}
        style={{ fontSize: 13, fontWeight: 500, transition: 'fill 0.2s ease', userSelect: 'none' }}
      >
        {node.label}
      </text>
    </g>
  );
}

/** Drawer 内容：定义 + 字段表 + 跳转按钮。 */
function ConceptDrawerContent({
  concept,
  onGoto,
}: {
  concept: ConceptNode;
  onGoto: (view: ConceptNode['navTarget']) => void;
}) {
  return (
    <div style={{ padding: 24 }}>
      {/* 头部：图标 + 标签 + 一句话定义 */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 16 }}>
        <span style={{ fontSize: 24, color: '#1677ff' }}>{concept.icon}</span>
        <div>
          <Title level={4} style={{ margin: 0 }}>{concept.label}</Title>
          <Text type="secondary" style={{ fontSize: 13 }}>{concept.oneLiner}</Text>
        </div>
      </div>
      {/* 字段表：Descriptions 单列，展示关键字段含义 */}
      <Descriptions
        column={1}
        size="small"
        bordered
        items={concept.fields.map((f) => ({ label: f.name, children: f.desc }))}
      />
      {/* 跳转按钮：调 pushUrl 路由跳转 */}
      <Button
        type="primary"
        icon={<ArrowRightOutlined />}
        block
        style={{ marginTop: 16 }}
        onClick={() => onGoto(concept.navTarget)}
        data-testid={`onboarding-graph-drawer-goto-${concept.id}`}
      >
        去{concept.label}页
      </Button>
    </div>
  );
}

/** 空态：支线节点（无 conceptId）点击时展示简单说明。 */
function GraphNodeDrawerFallback({ node }: { node: GraphNode }) {
  return (
    <div style={{ padding: 24 }}>
      <Title level={4} style={{ marginBottom: 8 }}>{node.label}</Title>
      <Text type="secondary">这是关系图的支线节点，用于辅助说明主概念关系，不对应独立菜单入口。</Text>
    </div>
  );
}

/**
 * 概念关系图主组件。
 *
 * 整体处理思路：
 * 1. SVG viewBox 1000x400，节点 + 连线静态手布局。
 * 2. hover 节点 → 该节点的 highlights 列表同时高亮，连线加粗。
 * 3. 点击节点 → 右侧 Drawer 展示概念详情（主节点）或简单说明（支线节点）。
 * 4. 流动动画由 CSS class 驱动，prefers-reduced-motion 时降级。
 */
export function ConceptRelationGraph() {
  const [hovered, setHovered] = useState<string | null>(null);
  const [drawerNode, setDrawerNode] = useState<GraphNode | null>(null);
  const reduceMotion = usePrefersReducedMotion();
  const { pushUrl } = useViewState();

  // 节点 id → GraphNode 映射，用于查连线两端。
  // useRef 避免每次 render 重建 Map。
  const nodeMapRef = useRef<Map<string, GraphNode>>(new Map());
  if (nodeMapRef.current.size === 0) {
    for (const n of GRAPH_NODES) nodeMapRef.current.set(n.id, n);
  }

  // 当前 Drawer 展示的概念（支线节点为 null）。
  const drawerConcept: ConceptNode | null = drawerNode?.conceptId
    ? CONCEPTS.find((c) => c.id === drawerNode.conceptId) ?? null
    : null;

  // 跳转：关 Drawer + pushUrl 路由跳转。
  const handleGoto = (view: ConceptNode['navTarget']) => {
    setDrawerNode(null);
    pushUrl(view, {});
  };

  return (
    <div
      style={{
        // 容器 overflow-x 兜底：小屏幕 SVG 溢出时允许横向滚动。
        overflowX: 'auto',
        padding: '24px 0',
      }}
      data-testid="onboarding-relation-graph"
    >
      <svg
        viewBox="0 0 1000 400"
        style={{ width: '100%', minWidth: 600, height: 400 }}
      >
        {/* 连线层：先渲染所有 Edge，让节点圆形覆盖在连线上方 */}
        {GRAPH_EDGES.map((edge) => {
          const from = nodeMapRef.current.get(edge.from);
          const to = nodeMapRef.current.get(edge.to);
          if (!from || !to) return null;
          // 连线 active = 任一端节点被 hover 或被关联高亮。
          const active =
            hovered === from.id ||
            hovered === to.id ||
            (hovered !== null && (from.highlights.includes(hovered) || to.highlights.includes(hovered)));
          return (
            <Edge
              key={`${edge.from}-${edge.to}`}
              from={from}
              to={to}
              active={active}
              reduceMotion={reduceMotion}
            />
          );
        })}
        {/* 节点层 */}
        {GRAPH_NODES.map((node) => (
          <GraphNodeCircle
            key={node.id}
            node={node}
            hovered={hovered}
            onHover={setHovered}
            onLeave={() => setHovered(null)}
            onClick={setDrawerNode}
          />
        ))}
      </svg>

      {/* Drawer：点击节点弹出 */}
      <Drawer
        open={!!drawerNode}
        onClose={() => setDrawerNode(null)}
        width={480}
        title={drawerNode?.label}
        data-testid="onboarding-graph-drawer"
      >
        {drawerNode && drawerConcept ? (
          <ConceptDrawerContent concept={drawerConcept} onGoto={handleGoto} />
        ) : drawerNode ? (
          <GraphNodeDrawerFallback node={drawerNode} />
        ) : (
          <Empty />
        )}
      </Drawer>
    </div>
  );
}
