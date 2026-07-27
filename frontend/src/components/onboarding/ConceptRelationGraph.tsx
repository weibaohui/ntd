// 概念关系图：纯 SVG 手绘 + 节点点击弹 Drawer + hover 高亮 + 流动动画。
// 不引入 reactflow 重依赖（节点固定 7 个，手布局即可）。
// 尊重 prefers-reduced-motion：动画降级为静态高亮。
// 030：支线节点 Drawer 支持定制说明文案 + 「去 XX 页」跳转按钮（黑板/看板）。

import { useState, useEffect, useRef } from 'react';
import { Button, Drawer, Typography, Descriptions, Empty } from 'antd';
import {
  CONCEPTS,
  GRAPH_EDGES,
  GRAPH_NODES,
  type ConceptNode,
  type GraphNode,
} from '@/components/onboarding/concepts';
// 030：嵌套组件独立实例化 useViewState 是 028 既定模式（靠 ntd-nav-change 事件全站同步），
// 跳转按钮必须走 pushUrl，不能用 location.hash 裸跳（不触发事件广播会导致 App 根实例状态脱节）。
import { useViewState } from '@/hooks/useViewState';

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

/** 取两节点中心点连线坐标（用于 SVG path）。
 *  fromR/toR 允许两端节点半径不同（主节点大、支节点小），连线从边缘出发而非中心。 */
function edgePath(from: GraphNode, to: GraphNode, fromR: number, toR: number): string {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const dist = Math.sqrt(dx * dx + dy * dy);
  // 单位向量 × 半径 = 边缘偏移。
  const ux = dx / dist;
  const uy = dy / dist;
  const x1 = from.x + ux * fromR;
  const y1 = from.y + uy * fromR;
  const x2 = to.x - ux * toR;
  const y2 = to.y - uy * toR;
  return `M ${x1} ${y1} L ${x2} ${y2}`;
}

/** 取节点半径：主航线节点 48，支线节点 36。 */
function nodeRadius(node: GraphNode): number {
  return node.isMain ? 48 : 36;
}

/** 单条连线，主航线带箭头 + 加粗深色，支线无箭头 + 细线浅色。 */
function Edge({
  from,
  to,
  active,
  reduceMotion,
  isMain,
}: {
  from: GraphNode;
  to: GraphNode;
  active: boolean;
  reduceMotion: boolean;
  isMain: boolean;
}) {
  // 主航线：深色 #1677ff + 加粗 2.5；支线：浅色 #d9d9d9 + 细 1.5。
  // active（hover 命中）时支线也升级到主航线色粗，保持高亮反馈一致。
  const baseStroke = isMain ? '#1677ff' : '#d9d9d9';
  const baseWidth = isMain ? 2.5 : 1.5;
  const stroke = active ? '#1677ff' : baseStroke;
  const strokeWidth = active ? 3 : baseWidth;
  // 主航线带箭头方向标记（marker-end），支线无。
  // 箭头颜色需与 stroke 同步，用 CSS currentColor 避免硬编码。
  return (
    <path
      d={edgePath(from, to, nodeRadius(from), nodeRadius(to))}
      stroke={stroke}
      strokeWidth={strokeWidth}
      fill="none"
      markerEnd={`url(#${isMain ? 'ntd-onboarding-arrow-main' : 'ntd-onboarding-arrow-side'})`}
      className={reduceMotion ? undefined : 'ntd-onboarding-edge-flow'}
      style={{
        // reduced-motion 时无动画，但仍可加粗高亮。
        transition: 'stroke 0.2s ease, stroke-width 0.2s ease',
      }}
    />
  );
}

/** 单个节点圆形，主节点加大 + 主色填充，hover 高亮 + 点击触发 Drawer。 */
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
  // 主航线节点默认主色填充（白底 + 主色边框会与支线混），
  // 用主色淡背景 #e6f4ff + 主色边框突出主链层级；hover 时升级到实心主色。
  const fill = isActive ? '#1677ff' : (node.isMain ? '#e6f4ff' : '#fff');
  const stroke = isActive ? '#1677ff' : (node.isMain ? '#1677ff' : '#d9d9d9');
  const textColor = isActive ? '#fff' : (node.isMain ? '#1677ff' : '#333');
  const r = nodeRadius(node);
  return (
    <g
      // SVG <g> 无原生 onClick 事件冒泡问题，直接挂。
      onClick={() => onClick(node)}
      onMouseEnter={() => onHover(node.id)}
      onMouseLeave={onLeave}
      style={{ cursor: 'pointer' }}
      data-testid={`onboarding-graph-node-${node.id}`}
    >
      {/* 圆形主体：主节点 r=48，支线 r=36 */}
      <circle
        cx={node.x}
        cy={node.y}
        r={r}
        fill={fill}
        stroke={stroke}
        strokeWidth={node.isMain ? 2.5 : 2}
        style={{ transition: 'fill 0.2s ease, stroke 0.2s ease' }}
      />
      {/* 标题文本居中：主节点字号略大 */}
      <text
        x={node.x}
        y={node.y}
        textAnchor="middle"
        dominantBaseline="middle"
        fill={textColor}
        style={{
          fontSize: node.isMain ? 14 : 13,
          fontWeight: node.isMain ? 600 : 500,
          transition: 'fill 0.2s ease',
          userSelect: 'none',
        }}
      >
        {node.label}
      </text>
    </g>
  );
}

/** Drawer 内容：定义 + 字段表（无跳转按钮，点击节点仅展示概念解释）。 */
function ConceptDrawerContent({ concept }: { concept: ConceptNode }) {
  return (
    <>
      {/* 头部：图标 + 标签 + 一句话定义。不加外层 padding，让 Drawer 自身 body padding 接管避免顶部留白叠加。 */}
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
    </>
  );
}

/**
 * 空态：支线节点（无 conceptId）点击时展示说明。
 *
 * 030 增强处理思路：
 * 1. 文案：节点带 drawerDesc 用定制文案（黑板/看板），缺省回退通用说明，
 *    保证既有 fallback 节点（触发器/技能/模型/执行记录）逐字不回归。
 * 2. 跳转：仅 navTarget 存在的节点渲染「去 XX 页」按钮；
 *    跳转动作不在这里直接做，回调给主组件（要先关 Drawer 再跳，避免遮罩残留）。
 */
function GraphNodeDrawerFallback({
  node,
  onNavigate,
}: {
  node: GraphNode;
  onNavigate: (node: GraphNode) => void;
}) {
  return (
    <div style={{ padding: 24 }}>
      <Title level={4} style={{ marginBottom: 8 }}>{node.label}</Title>
      <Text type="secondary">
        {node.drawerDesc ?? '这是关系图的支线节点，用于辅助说明主概念关系，不对应独立菜单入口。'}
      </Text>
      {/* 按钮 testid 沿用 026 测试约定的 onboarding-graph-drawer-goto-{id} 命名 */}
      {node.navTarget && (
        <div style={{ marginTop: 16 }}>
          <Button
            type="primary"
            onClick={() => onNavigate(node)}
            data-testid={`onboarding-graph-drawer-goto-${node.id}`}
          >
            {`去${node.label}页`}
          </Button>
        </div>
      )}
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
  // 030：跳转按钮走 pushUrl（自动广播 ntd-nav-change 同步全站视图状态）。
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

  // 「去 XX 页」按钮回调：先关 Drawer 再跳转 —— 跳转后 onboarding 视图卸载，
  // 不关 Drawer 会残留遮罩盖住新页面；navTarget 为空属防御分支（按钮根本不渲染），直接忽略。
  const handleNodeNavigate = (node: GraphNode) => {
    if (!node.navTarget) return;
    setDrawerNode(null);
    pushUrl(node.navTarget, { mode: node.navMode });
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
        {/* 慨头定义：主航线 + 支线各一个 marker，颜色区分主支层级。
            marker 内部 fill 用 currentColor，跟随 path 的 stroke 自动同步，
            hover 时支线 stroke 升级到 #1677ff，支线箭头也同步升级到主色。 */}
        <defs>
          <marker
            id="ntd-onboarding-arrow-main"
            viewBox="0 0 10 10"
            refX="9"
            refY="5"
            markerWidth="8"
            markerHeight="8"
            orient="auto-start-reverse"
          >
            <path d="M 0 0 L 10 5 L 0 10 z" fill="currentColor" />
          </marker>
          <marker
            id="ntd-onboarding-arrow-side"
            viewBox="0 0 10 10"
            refX="9"
            refY="5"
            markerWidth="7"
            markerHeight="7"
            orient="auto-start-reverse"
          >
            <path d="M 0 0 L 10 5 L 0 10 z" fill="currentColor" />
          </marker>
        </defs>
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
              isMain={!!edge.isMain}
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

      {/* Drawer：点击节点弹出。无 title 避免与 ConceptDrawerContent 内头行重复，
          body 内子组件也不额外加 padding，让 Drawer 自身 body padding 接管避免顶部留白叠加。 */}
      <Drawer
        open={!!drawerNode}
        onClose={() => setDrawerNode(null)}
        width={480}
        title={null}
        data-testid="onboarding-graph-drawer"
      >
        {drawerNode && drawerConcept ? (
          <ConceptDrawerContent concept={drawerConcept} />
        ) : drawerNode ? (
          <GraphNodeDrawerFallback node={drawerNode} onNavigate={handleNodeNavigate} />
        ) : (
          <Empty />
        )}
      </Drawer>
    </div>
  );
}
