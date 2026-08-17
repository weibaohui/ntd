// processLayout.ts
// ---------------------------------------------------------------------------
// M4 里程碑：React Flow 节点布局算法纯函数模块。
//
// 设计意图（对应 docs/design/029-M4-ReactFlow可视化编辑器-方案.md §3.1.4）：
// - 两层布局：dagre 横向布局 phase，phase 内部 flex 纵向排列 link。
// - 纯函数无副作用，便于 vitest 测试。
// - 布局结果供 processGraphBuilder 构造 React Flow nodes。
//
// 布局策略：
// 1. 横向：按 phases 数组顺序从左到右排列 PhaseNode，rankdir='LR'，ranksep=80。
// 2. 纵向：每个 PhaseNode 内部用 CSS flex 纵向排列 LinkNode，
//    间距 LINK_GAP=16，头部 PHASE_HEADER=60。
//
// 节点尺寸常量：
// - NODE_WIDTH = 240（LinkNode 卡片宽度）
// - NODE_HEIGHT = 80（LinkNode 卡片高度）
// - PHASE_PADDING = 40（PhaseNode 内边距）
// - PHASE_HEADER = 60（PhaseNode 头部高度）
// - LINK_GAP = 16（LinkNode 之间间距）
// ---------------------------------------------------------------------------

import dagre from 'dagre';
import type { PhaseDefinition } from '@/types/process';

// ── 尺寸常量 ──────────────────────────────────────────

// LinkNode 卡片宽度
export const NODE_WIDTH = 240;
// LinkNode 卡片高度
export const NODE_HEIGHT = 80;
// PhaseNode 内边距（左右各 20）
export const PHASE_PADDING = 40;
// PhaseNode 头部高度（显示 phase.name + 删除按钮）
export const PHASE_HEADER = 60;
// 头部与首个 LinkNode 之间的纵向间距：
// 首个卡片紧贴阶段标题会显得拥挤，留出与卡片间距一致的呼吸感
export const HEADER_LINK_GAP = 16;
// LinkNode 之间纵向间距
export const LINK_GAP = 16;
// phase 之间横向间距（dagre ranksep）
const PHASE_RANKSEP = 80;

// ── phase 高度公式（单一数据源）────────────────────

// 计算 phase 容器总高度：头部 + 头部与首卡片间距 + link 行高（空 phase 按 1 行占位）。
// layoutPhases（dagre 节点尺寸）与 processGraphBuilder（React Flow 节点 style）
// 必须共用此公式，否则容器高度与内部卡片布局错位。
export function calcPhaseHeight(linkCount: number): number {
  return (
    PHASE_HEADER + HEADER_LINK_GAP + Math.max(1, linkCount) * (NODE_HEIGHT + LINK_GAP)
  );
}

// ── 横向布局（dagre LR）──────────────────────────────────

// 横向布局 phase，返回每个 phase 的左上角坐标。
//
// dagre 配置：
// - rankdir='LR'：从左到右
// - ranksep=80：phase 之间间距
//
// 算法：
// 1. 每个 phase 是一个 dagre 节点，宽度 = NODE_WIDTH + PHASE_PADDING，
//    高度 = max(1, links.length) * (NODE_HEIGHT + LINK_GAP) + PHASE_HEADER
// 2. 顺序连边 phase-0 → phase-1 → phase-2 ...
// 3. dagre.layout 计算坐标
// 4. 转换为左上角坐标（dagre 返回中心坐标）
export function layoutPhases(
  phases: PhaseDefinition[],
): Map<string, { x: number; y: number }> {
  // 空数组直接返回空 Map
  if (phases.length === 0) return new Map();

  // 创建 dagre 图实例
  const g = new dagre.graphlib.Graph();
  // rankdir='LR' 从左到右布局，ranksep 控制节点间距。
  // 注意：只用 dagre 算横向 x（按 rank 分列），纵向 y 不采用 dagre 的居中结果
  // ——见下方 result.set 处统一置 0，强制阶段顶部对齐。
  g.setGraph({ rankdir: 'LR', ranksep: PHASE_RANKSEP });
  // 默认边标签（dagre 要求）
  g.setDefaultEdgeLabel(() => ({}));

  // 为每个 phase 创建 dagre 节点
  phases.forEach((phase, i) => {
    // phase 高度 = 头部 + 头部间距 + 内部 link 总高度（公式与 builder 共用 calcPhaseHeight）
    const linkCount = phase.links?.length ?? 0;
    const height = calcPhaseHeight(linkCount);
    // phase 宽度 = link 宽度 + 内边距
    const width = NODE_WIDTH + PHASE_PADDING;

    // 节点 id 用 phase-${i}，与 processGraphBuilder 保持一致
    g.setNode(`phase-${i}`, { width, height });

    // 顺序连边：phase-0 → phase-1 → phase-2 ...
    if (i > 0) {
      g.setEdge(`phase-${i - 1}`, `phase-${i}`);
    }
  });

  // 执行 dagre 布局算法
  dagre.layout(g);

  // 转换为左上角坐标 Map
  const result = new Map<string, { x: number; y: number }>();
  phases.forEach((_, i) => {
    const node = g.node(`phase-${i}`);
    // dagre 返回中心坐标，x 转左上角需减去宽度的一半。
    // y 统一置 0：dagre 默认按节点中心纵向居中排布，环节数不同（容器高度不同）
    // 时阶段会上下错落；强制所有阶段从同一顶部 y 开始，实现"阶段上对齐"。
    result.set(`phase-${i}`, {
      x: node.x - node.width / 2,
      y: 0,
    });
  });

  return result;
}

// ── 纵向布局（phase 内部 link）──────────────────────────

// 纵向布局 phase 内部的 link，返回每个 link 的相对坐标。
//
// 相对坐标：相对于父 PhaseNode 的左上角。
// x = PHASE_PADDING / 2 = 20（link 左边距）
// y = PHASE_HEADER + HEADER_LINK_GAP + linkIndex * (NODE_HEIGHT + LINK_GAP)
//
// React Flow 的 parentNode 语义：子节点 position 是相对于父节点的。
export function layoutLinksInPhase(
  phase: PhaseDefinition,
  phaseIndex: number,
): Array<{ id: string; position: { x: number; y: number } }> {
  const links = phase.links ?? [];
  // link 左边距 = 内边距的一半（PHASE_PADDING=40，左右各 20）
  const x = PHASE_PADDING / 2;

  // 为每个 link 计算相对坐标
  return links.map((_link, linkIndex) => {
    // y = 头部高度 + 头部与首卡片间距 + linkIndex * (节点高度 + 间距)
    const y =
      PHASE_HEADER + HEADER_LINK_GAP + linkIndex * (NODE_HEIGHT + LINK_GAP);
    return {
      // 节点 id 用 link-${phaseIndex}-${linkIndex}
      id: `link-${phaseIndex}-${linkIndex}`,
      position: { x, y },
    };
  });
}
