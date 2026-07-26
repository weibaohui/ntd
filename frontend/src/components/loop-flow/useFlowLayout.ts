// 泛化 dagre 自动排布 hook。
// LoopFlowGraph 和 ProcessFlowGraph 共用，各自提供节点尺寸与边关系，
// 本 hook 只负责跑 dagre 并返回 SVG 位置数据。

import { useMemo } from 'react';
import dagre from 'dagre';
import {
  NODE_WIDTH, NODE_HEIGHT, RANK_SEP, NODE_SEP,
  VIRTUAL_NODE_RADIUS,
  LOOP_BACK_TOP_PADDING, SELF_LOOP_GAP,
  START_NODE_ID, END_NODE_ID,
} from '@/components/loop-flow/flowConstants';

/** 入参：一个节点的标识与尺寸。ProcessFlowGraph 可用不同宽高覆盖常量。 */
export interface FlowNodeInput {
  id: number;
  /** 节点宽度（默认 NODE_WIDTH） */
  width?: number;
  /** 节点高度（默认 NODE_HEIGHT） */
  height?: number;
}

/** 入参：一条 dagre 布局用边。区分 from/to 不是从 steps 的反向查找，而是
 *  边两端都是已注册 node id（含虚拟 START/END 的 -1/-2）。 */
export interface FlowEdgeInput {
  from: number;
  to: number;
  label: string;
}

/** 单个节点的布局位置（SVG 左上角）。 */
export interface NodePosition {
  x: number;
  y: number;
}

export interface FlowLayoutResult {
  /** node id → 布局位置（左上角）。 */
  positions: Map<number, NodePosition>;
  /** 虚拟 Start 节点位置。 */
  startX: number;
  startY: number;
  /** 虚拟 End 节点位置。 */
  endX: number;
  endY: number;
  /** SVG 画布宽高。 */
  width: number;
  height: number;
  /** 是否有回环边（用于顶部留白 + dagre 内容下移）。 */
  hasLoopBack: boolean;
  /** dagre 内容 Y 方向下移量（hasLoopBack 时 > 0）。 */
  dagreOffsetY: number;
}

/** 按字符数粗截断（布局无关，但 FlowStepNode / TemplateStepCard 都复用）。 */
export function truncateText(text: string, maxLen: number): string {
  return text.length > maxLen ? text.slice(0, maxLen - 1) + '…' : text;
}

/** 阶段色板：按 phase_id 哈希取色，保证同阶段颜色稳定。 */
const PHASE_PALETTE = [
  '#0891b2', '#7c3aed', '#db2777', '#ea580c', '#16a34a',
  '#2563eb', '#ca8a04', '#9333ea', '#059669', '#dc2626',
];

export function phaseColor(phaseId: number): string {
  return PHASE_PALETTE[Math.abs(phaseId) % PHASE_PALETTE.length];
}

/**
 * 用 dagre 自动排布节点与虚拟 Start/End。
 *
 * @param nodeInputs  真实节点列表（只用到 id / width / height，不关心业务数据）。
 * @param edgeInputs  边列表（from/to 必须已注册到 dagre 图中，可以是虚拟 -1/-2）。
 * @param hasLoopBack 调用方在构图时自行判定的回环标志，影响顶部留白。
 */
export function useFlowLayout(
  nodeInputs: FlowNodeInput[],
  edgeInputs: FlowEdgeInput[],
  hasSelfLoop: boolean,
  hasLoopBack: boolean,
): FlowLayoutResult {
  return useMemo(() => {
    if (nodeInputs.length === 0) {
      return {
        positions: new Map(), startX: 0, startY: 0, endX: 0, endY: 0,
        width: 0, height: 0, hasLoopBack: false, dagreOffsetY: 0,
      };
    }

    const g = new dagre.graphlib.Graph();
    g.setGraph({ rankdir: 'LR', ranksep: RANK_SEP, nodesep: NODE_SEP, marginx: 20, marginy: 20 });
    g.setDefaultEdgeLabel(() => ({}));

    // 虚拟 Start / End
    const vsize = VIRTUAL_NODE_RADIUS * 2;
    g.setNode(String(START_NODE_ID), { width: vsize, height: vsize });
    g.setNode(String(END_NODE_ID), { width: vsize, height: vsize });

    for (const ni of nodeInputs) {
      g.setNode(String(ni.id), {
        width: ni.width ?? NODE_WIDTH,
        height: ni.height ?? NODE_HEIGHT,
      });
    }

    // 注册边（自环不加入 dagre，否则布局乱）
    for (const e of edgeInputs) {
      if (e.from === e.to) continue;
      g.setEdge(String(e.from), String(e.to));
    }

    dagre.layout(g);

    const positions = new Map<number, NodePosition>();
    for (const ni of nodeInputs) {
      const pos = g.node(String(ni.id));
      positions.set(ni.id, {
        x: pos.x - (ni.width ?? NODE_WIDTH) / 2,
        y: pos.y - (ni.height ?? NODE_HEIGHT) / 2,
      });
    }

    const sp = g.node(String(START_NODE_ID));
    const ep = g.node(String(END_NODE_ID));
    const gw = g.graph().width || 0;
    const gh = g.graph().height || 0;

    const loopPad = hasLoopBack ? LOOP_BACK_TOP_PADDING : 0;
    const slPad = hasSelfLoop ? SELF_LOOP_GAP : 0;

    return {
      positions,
      startX: sp?.x ?? 40, startY: sp?.y ?? 40,
      endX: ep?.x ?? gw - 40, endY: ep?.y ?? gh - 40,
      width: gw + 40,
      height: gh + 40 + loopPad + slPad,
      hasLoopBack,
      dagreOffsetY: hasLoopBack ? LOOP_BACK_TOP_PADDING : 0,
    };
  }, [nodeInputs, edgeInputs, hasLoopBack, hasSelfLoop]);
}
