// processGraphBuilder.ts
// ---------------------------------------------------------------------------
// M4 里程碑：从 ProcessDefinition 构建 React Flow nodes + edges 的纯函数模块。
//
// 设计意图（对应 docs/design/029-M4-ReactFlow可视化编辑器-方案.md §3.1.5）：
// - 纯函数无副作用，便于 vitest 测试。
// - 调用 processLayout 计算坐标，调用 processDefinitionUpdater 做 ID 映射。
// - 输出 React Flow v12 的 Node[] + Edge[]。
//
// 节点类型：
// - phase：group 容器，width/height + position
// - link：子节点，parentNode 指向 phase，position 相对于父节点
//
// 边类型：
// - process：自定义边，颜色和虚线由 data.color / data.dashed 决定
//
// 边视觉规则（设计 §5.4.1）：
// - on_success: next / end → 不生成边（顺向边由 React Flow 自动连？不，M4 不画顺向边）
//   实际上 on_success: next 是"执行下一个"，不是显式 goto，M4 不画这类边。
// - on_success: goto:xxx → 绿色 #10b981 smoothstep
// - on_gate_fail: goto:xxx → 橙色虚线 #d97706 smoothstep
//
// 节点 ID 约定（M4 用数组索引，M5 可改用 phase.id/link.id）：
// - phase 节点 id：`phase-${phaseIndex}`
// - link 节点 id：`link-${phaseIndex}-${linkIndex}`
// - 边 id：`edge-${sourceLinkId}-${handleType}-${targetLinkId}`
// ---------------------------------------------------------------------------

import type { Node, Edge } from '@xyflow/react';
import type { ProcessDefinition } from '@/types/process';
import {
  layoutPhases,
  layoutLinksInPhase,
  NODE_WIDTH,
  NODE_HEIGHT,
  PHASE_PADDING,
  PHASE_HEADER,
  LINK_GAP,
} from './processLayout';

// ── 边颜色常量 ──────────────────────────────────────

// goto 成功边颜色（绿）
const GOTO_SUCCESS_COLOR = '#10b981';
// goto 门禁失败边颜色（橙）
const GOTO_GATE_FAIL_COLOR = '#d97706';
// goto 门禁失败边是否虚线
const GOTO_GATE_FAIL_DASHED = true;

// ── 回调接口 ──────────────────────────────────────

// buildProcessGraph 需要的回调集合，由 ProcessVisualEditor 注入。
export interface GraphCallbacks {
  // 删除 phase（弹 Modal.confirm）
  onDeletePhase: (phaseId: string) => void;
  // 选中 phase（右侧属性面板切换）
  onSelectPhase: (phaseId: string) => void;
  // 选中 link（右侧属性面板切换）
  onSelectLink: (linkId: string) => void;
  // 删除边（重置对应 link 的 on_success / on_gate_fail）
  onDeleteEdge: (edgeId: string) => void;
}

// ── 构建结果 ──────────────────────────────────────

export interface ProcessGraph {
  // React Flow 节点数组
  nodes: Node[];
  // React Flow 边数组
  edges: Edge[];
}

// ── 主构建函数 ────────────────────────────────────

// 从 ProcessDefinition 构建 React Flow 图。
//
// 算法：
// 1. 调用 layoutPhases 计算每个 phase 的左上角坐标
// 2. 为每个 phase 创建一个 group 节点
// 3. 遍历每个 phase 内部的 link，调用 layoutLinksInPhase 计算相对坐标
// 4. 为每个 link 创建一个子节点，parentNode 指向 phase
// 5. 遍历所有 link，检查 on_success / on_gate_fail 是否是 goto:xxx
// 6. 如果是 goto，找到目标 link，创建一条边
//
// 边界处理：
// - definition 为 null → 返回空图
// - phases 为空 → 返回空图
// - goto 目标 link 不存在 → 跳过该边（悬空引用，不画）
export function buildProcessGraph(
  definition: ProcessDefinition | null,
  callbacks: GraphCallbacks,
): ProcessGraph {
  // definition 为 null 时返回空图
  if (!definition) return { nodes: [], edges: [] };

  const phases = definition.phases ?? [];
  // phases 为空时返回空图
  if (phases.length === 0) return { nodes: [], edges: [] };

  // 1. 横向布局 phase
  const phasePositions = layoutPhases(phases);

  // 2. 构建 phase 节点
  const phaseNodes: Node[] = phases.map((phase, phaseIndex) => {
    const pos = phasePositions.get(`phase-${phaseIndex}`) ?? { x: 0, y: 0 };
    // 计算 phase 高度：max(1, links.length) * (NODE_HEIGHT + LINK_GAP) + PHASE_HEADER
    const linkCount = phase.links?.length ?? 0;
    const innerHeight = Math.max(1, linkCount) * (NODE_HEIGHT + LINK_GAP);
    const height = innerHeight + PHASE_HEADER;
    const width = NODE_WIDTH + PHASE_PADDING;

    return {
      id: `phase-${phaseIndex}`,
      type: 'phase',
      position: pos,
      // group 节点需要显式 width/height
      style: { width, height },
      data: {
        phase,
        phaseIndex,
        onDeletePhase: callbacks.onDeletePhase,
        onSelectPhase: callbacks.onSelectPhase,
      },
    };
  });

  // 3. 构建 link 节点（子节点，parentNode 指向 phase）
  const linkNodes: Node[] = [];
  phases.forEach((phase, phaseIndex) => {
    // 调用 layoutLinksInPhase 计算 link 相对坐标
    const linkLayouts = layoutLinksInPhase(phase, phaseIndex);
    const links = phase.links ?? [];

    links.forEach((link, linkIndex) => {
      const layout = linkLayouts[linkIndex];
      linkNodes.push({
        id: `link-${phaseIndex}-${linkIndex}`,
        type: 'link',
        position: layout.position,
        // React Flow v12 用 parentId 而非 parentNode 建立父子关系
        parentId: `phase-${phaseIndex}`,
        data: {
          link,
          phaseId: phase.id,
          phaseIndex,
          linkIndex,
          onSelectLink: callbacks.onSelectLink,
        },
      });
    });
  });

  // 4. 构建边（goto 连线）
  const edges: Edge[] = [];
  // 构建 linkId → nodeId 映射，用于查找 goto 目标
  const linkIdToNodeId = new Map<string, string>();
  // 同时构建 phaseId/linkId → phaseIndex/linkIndex 映射
  const linkIdToIndices = new Map<
    string,
    { phaseIndex: number; linkIndex: number }
  >();

  phases.forEach((phase, phaseIndex) => {
    const links = phase.links ?? [];
    links.forEach((link, linkIndex) => {
      // link.id 是 YAML 里的环节 id，用于 goto:xxx 的 xxx
      linkIdToNodeId.set(link.id, `link-${phaseIndex}-${linkIndex}`);
      linkIdToIndices.set(link.id, { phaseIndex, linkIndex });
    });
  });

  // 遍历所有 link，检查 on_success / on_gate_fail 是否是 goto
  phases.forEach((phase, phaseIndex) => {
    const links = phase.links ?? [];
    links.forEach((link, linkIndex) => {
      const sourceNodeId = `link-${phaseIndex}-${linkIndex}`;

      // 检查 on_success
      if (link.on_success && link.on_success.startsWith('goto:')) {
        const targetLinkId = link.on_success.slice('goto:'.length);
        const targetNodeId = linkIdToNodeId.get(targetLinkId);
        if (targetNodeId) {
          // 目标存在，创建绿色边
          edges.push(
            createGotoEdge(
              sourceNodeId,
              targetNodeId,
              link.id,
              targetLinkId,
              'on_success',
              callbacks.onDeleteEdge,
            ),
          );
        }
        // 目标不存在（悬空引用）→ 跳过，不画边
      }

      // 检查 on_gate_fail
      if (link.on_gate_fail && link.on_gate_fail.startsWith('goto:')) {
        const targetLinkId = link.on_gate_fail.slice('goto:'.length);
        const targetNodeId = linkIdToNodeId.get(targetLinkId);
        if (targetNodeId) {
          // 目标存在，创建橙色虚线边
          edges.push(
            createGotoEdge(
              sourceNodeId,
              targetNodeId,
              link.id,
              targetLinkId,
              'on_gate_fail',
              callbacks.onDeleteEdge,
            ),
          );
        }
      }
    });
  });

  return { nodes: [...phaseNodes, ...linkNodes], edges };
}

// ── 辅助函数 ──────────────────────────────────────

// 创建一条 goto 边。
//
// 边 id 格式：edge-${sourceLinkId}-${handleType}-${targetLinkId}
// 例如：edge-link1-on_success-link2
//
// 边视觉：
// - on_success goto → 绿色实线
// - on_gate_fail goto → 橙色虚线
//
// handle 配置：
// - sourceHandle：'on_success' 或 'on_gate_fail'（与 LinkNode 的 handle id 对应）
// - targetHandle：'target'（与 LinkNode 的 target handle id 对应）
function createGotoEdge(
  sourceNodeId: string,
  targetNodeId: string,
  sourceLinkId: string,
  targetLinkId: string,
  handleType: 'on_success' | 'on_gate_fail',
  onDeleteEdge: (edgeId: string) => void,
): Edge {
  // 边 id 包含 sourceLinkId 和 handleType，便于删除时定位
  const edgeId = `edge-${sourceLinkId}-${handleType}-${targetLinkId}`;

  // 根据 handleType 决定颜色和虚线
  const color =
    handleType === 'on_success'
      ? GOTO_SUCCESS_COLOR
      : GOTO_GATE_FAIL_COLOR;
  const dashed = handleType === 'on_gate_fail' && GOTO_GATE_FAIL_DASHED;

  return {
    id: edgeId,
    source: sourceNodeId,
    target: targetNodeId,
    sourceHandle: handleType, // 'on_success' 或 'on_gate_fail'
    targetHandle: 'target',
    type: 'process', // 自定义边类型
    data: {
      color,
      dashed,
      onDelete: onDeleteEdge,
    },
  };
}
