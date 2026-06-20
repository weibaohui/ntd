// Loop Studio 执行环节流程图。
//
// 布局：dagre 自动排列虚拟 Start/End + 真实 step 节点。
// 渲染：边（FlowEdge） + 真实环节卡片 + Start/End 虚拟节点（FlowVirtualNodes）。
// 回环：当某环节失败要回到前面重做（fail-goto 目标 step index < 源 step index），
//       用正交折线（顶边出 + 顶边入）+ 加粗红色虚线 + 白底「↻ 重试」标签。
//
// 文件按 500 行硬限拆为：
// - LoopFlowGraph.tsx（本文件）：布局 + 主组装
// - FlowEdge.tsx：单条边渲染与路径计算
// - FlowVirtualNodes.tsx：Start/End 节点
// - flowConstants.ts / flowTypes.ts：共享常量与类型

import { useMemo } from 'react';
import dagre from 'dagre';
import type { LoopStepDto } from '@/types/loop';
import {
  StartNode, EndNode,
} from '@/components/loop-flow/FlowVirtualNodes';
import { FlowEdge, classifyEdge, resolveTargetStep } from '@/components/loop-flow/FlowEdge';
import {
  NODE_WIDTH, NODE_HEIGHT, RANK_SEP, NODE_SEP,
  LOOP_BACK_TOP_PADDING,
  VIRTUAL_NODE_RADIUS, START_NODE_ID, END_NODE_ID,
} from '@/components/loop-flow/flowConstants';
import type { LayoutNode, LayoutEdge } from '@/components/loop-flow/flowTypes';

interface FlowGraphProps {
  steps: LoopStepDto[];
  selectedStepId: number | null;
  onSelectStep: (step: LoopStepDto) => void;
  onAddStep: () => void;
}

function useFlowLayout(steps: LoopStepDto[]) {
  return useMemo(() => {
    if (steps.length === 0) return {
      nodes: [] as LayoutNode[], edges: [] as LayoutEdge[], width: 0, height: 0,
      startX: 0, startY: 0, endX: 0, endY: 0,
      hasLoopBack: false,
    };

    const g = new dagre.graphlib.Graph();
    g.setGraph({ rankdir: 'LR', ranksep: RANK_SEP, nodesep: NODE_SEP, marginx: 20, marginy: 20 });
    g.setDefaultEdgeLabel(() => ({}));

    // 虚拟 Start / End 节点（dagre 用 width/height 计算位置，半径由 VIRTUAL_NODE_RADIUS 决定）
    const VIRTUAL_NODE_SIZE = VIRTUAL_NODE_RADIUS * 2;
    g.setNode(String(START_NODE_ID), { width: VIRTUAL_NODE_SIZE, height: VIRTUAL_NODE_SIZE });
    g.setNode(String(END_NODE_ID),   { width: VIRTUAL_NODE_SIZE, height: VIRTUAL_NODE_SIZE });

    // 真实 step 节点
    for (const step of steps) {
      g.setNode(String(step.id), { width: NODE_WIDTH, height: NODE_HEIGHT });
    }

    // 边集合：Start→first、step↔step（含回环识别）、step→end 都会 push 进来。
    // dagre 知道这些边用于布局，前端按 layoutEdges 渲染。
    const layoutEdges: LayoutEdge[] = [];
    const stepIndexById = new Map<number, number>();
    steps.forEach((s, i) => stepIndexById.set(s.id, i));

    // Start → 第一个 step：dagre 用这条边把首节点排到 Start 右侧，
    // layoutEdges 也得 push 同款边，否则前端不会画这条连线。
    if (steps.length > 0) {
      g.setEdge(String(START_NODE_ID), String(steps[0].id));
      layoutEdges.push({
        from: String(START_NODE_ID), to: String(steps[0].id),
        label: '',
        type: 'start-first', fromId: START_NODE_ID, toId: steps[0].id,
      });
    }

    for (const step of steps) {
      const sourceIdx = stepIndexById.get(step.id) ?? 0;
      const targetNameOf = (id: number) => steps.find(s => s.id === id)?.name || String(id);

      // 成功边
      const successType = classifyEdge(step, steps, step.on_success, step.success_goto_step_id, true);
      const successTarget = resolveTargetStep(step, steps, step.on_success, step.success_goto_step_id);
      if (successTarget != null) {
        g.setEdge(String(step.id), String(successTarget));
        const targetIdx = stepIndexById.get(successTarget);
        const isLoopBack = successType === 'success-goto'
          && targetIdx != null && targetIdx < sourceIdx;
        const name = targetNameOf(successTarget);
        layoutEdges.push({
          from: String(step.id), to: String(successTarget),
          label: isLoopBack
            ? `跳回 ${name}`
            : step.on_success === 'goto' ? `✅→${name}` : '',
          type: successType, fromId: step.id, toId: successTarget,
          isLoopBack,
        });
      }

      // 失败边（仅当策略与成功策略不同时绘制，避免双线重叠）
      if (step.min_rating != null && step.on_rating_fail !== step.on_success) {
        const failType = classifyEdge(step, steps, step.on_rating_fail, step.fail_goto_step_id, false);
        const failTarget = resolveTargetStep(step, steps, step.on_rating_fail, step.fail_goto_step_id);
        if (failTarget != null) {
          g.setEdge(String(step.id), String(failTarget));
          const targetIdx = stepIndexById.get(failTarget);
          const isLoopBack = failType === 'fail-goto'
            && targetIdx != null && targetIdx < sourceIdx;
          // 回环边：标签只写阈值条件（<90分），不加 ↻ 之类的前缀符号。
          // 路径已经是向上拱的正交折线 + 红色虚线，「回环」语义靠视觉传达，
          // 文字聚焦在「什么情况下」这一核心信息上。
          const name = targetNameOf(failTarget);
          layoutEdges.push({
            from: String(step.id), to: String(failTarget),
            label: isLoopBack
              ? `<${step.min_rating}分`
              : step.on_rating_fail === 'goto' ? `❌→${name}`
              : step.on_rating_fail === 'skip' ? '失败→继续' : '',
            type: failType, fromId: step.id, toId: failTarget,
            isLoopBack,
          });
        }
      }
    }

    // 任何带 end 策略的环节连到 End 节点
    for (const step of steps) {
      if (step.on_success === 'end' || step.on_rating_fail === 'end') {
        g.setEdge(String(step.id), String(END_NODE_ID));
        layoutEdges.push({
          from: String(step.id), to: String(END_NODE_ID),
          label: '',
          type: 'end', fromId: step.id, toId: END_NODE_ID,
        });
      }
    }
    // 兜底：所有环节都跑通后没显式 end 策略，就把最后一个 step 连到 End
    if (!layoutEdges.some(e => e.toId === END_NODE_ID) && steps.length > 0) {
      const lastId = steps[steps.length - 1].id;
      g.setEdge(String(lastId), String(END_NODE_ID));
      layoutEdges.push({
        from: String(lastId), to: String(END_NODE_ID),
        label: '', type: 'end', fromId: lastId, toId: END_NODE_ID,
      });
    }

    dagre.layout(g);

    const nodes: LayoutNode[] = steps.map(step => {
      const pos = g.node(String(step.id));
      return {
        id: step.id,
        x: pos.x - NODE_WIDTH / 2,
        y: pos.y - NODE_HEIGHT / 2,
        step,
      };
    });

    const startPos = g.node(String(START_NODE_ID));
    const endPos = g.node(String(END_NODE_ID));

    const graphWidth = g.graph().width || 0;
    const graphHeight = g.graph().height || 0;

    // 顶部留白：任一连线是回环（弧线向上），给 SVG 顶部加 padding，
    // 否则弧线会裁切。dagre 内容整体下移以让出顶部空间。
    const hasLoopBack = layoutEdges.some(e => e.isLoopBack);
    const loopBackPad = hasLoopBack ? LOOP_BACK_TOP_PADDING : 0;

    return {
      nodes,
      edges: layoutEdges,
      width: graphWidth + 40,
      height: graphHeight + 40 + loopBackPad,
      startX: startPos?.x ?? 40,
      startY: startPos?.y ?? 40,
      endX: endPos?.x ?? graphWidth - 40,
      endY: endPos?.y ?? graphHeight - 40,
      hasLoopBack,
    };
  }, [steps]);
}

export function LoopFlowGraph({
  steps,
  selectedStepId,
  onSelectStep, onAddStep,
}: FlowGraphProps) {
  const {
    nodes, edges, width, height,
    startX, startY, endX, endY,
    hasLoopBack,
  } = useFlowLayout(steps);
  // 有回环时把 dagre 内容整体下移，让顶部正交折线有画布。
  const dagreOffsetY = hasLoopBack ? LOOP_BACK_TOP_PADDING : 0;

  if (steps.length === 0) {
    return (
      <div
        onClick={onAddStep}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => { if (e.key === 'Enter') onAddStep(); }}
        style={{
          display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
          minHeight: 160, width: '100%',
          border: '2px dashed var(--color-border, #e2e8f0)',
          borderRadius: 12, cursor: 'pointer',
          color: 'var(--color-text-tertiary, #94a3b8)',
          fontSize: 13, gap: 8,
          transition: 'border-color 200ms, color 200ms',
        }}
        onMouseEnter={(e) => { e.currentTarget.style.borderColor = '#0891b2'; e.currentTarget.style.color = '#0891b2'; }}
        onMouseLeave={(e) => { e.currentTarget.style.borderColor = '#e2e8f0'; e.currentTarget.style.color = '#94a3b8'; }}
      >
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <line x1="12" y1="5" x2="12" y2="19" /><line x1="5" y1="12" x2="19" y2="12" />
        </svg>
        <span>暂无执行环节，点击添加</span>
      </div>
    );
  }

  return (
    <div style={{ overflowX: 'auto', overflowY: 'hidden', padding: '12px 0', minHeight: 160 }}>
      <svg width={width} height={height} style={{ display: 'block' }}>
        {/* dagre 布局的所有内容（边、真实环节、Start/End 节点）。
            有回环时下移 dagreOffsetY 腾出顶部空间画正交折线。 */}
        <g transform={`translate(0, ${dagreOffsetY})`}>
          {/* 边 */}
          {edges.map((edge, i) => (
            <FlowEdge
              key={`edge-${i}`}
              edge={edge}
              index={i}
              nodes={nodes}
              startX={startX}
              startY={startY}
              endX={endX}
              endY={endY}
            />
          ))}

          {/* Start / End 虚拟节点 */}
          <StartNode x={startX} y={startY} />
          <EndNode x={endX} y={endY} />

          {/* 真实环节节点 */}
          {nodes.map((node) => {
            const isSelected = selectedStepId === node.id;
            return (
              <g
                key={`node-${node.id}`}
                onClick={() => onSelectStep(node.step)}
                style={{ cursor: 'pointer' }}
              >
                <rect
                  x={node.x} y={node.y}
                  width={NODE_WIDTH} height={NODE_HEIGHT}
                  rx={8} ry={8}
                  fill={isSelected ? '#f0f9ff' : '#ffffff'}
                  stroke={isSelected ? '#0891b2' : '#e2e8f0'}
                  strokeWidth={isSelected ? 2 : 1}
                />
                <circle
                  cx={node.x + NODE_WIDTH - 10} cy={node.y + 10} r={4}
                  fill={node.step.enabled ? '#22c55e' : '#94a3b8'}
                />
                {/* Index badge 放在 step 左上角（探出卡片外），
                    不再放在 left-middle——那里正好是入边箭头落点，
                    圆形 badge 会把箭头完全遮住。移到 top-left 后箭头在
                    left-middle 自由落下，跟右上角的状态 dot 视觉对角呼应。 */}
                <rect
                  x={node.x - 10} y={node.y - 10}
                  width={20} height={20} rx={10}
                  fill={isSelected ? '#0891b2' : '#f1f5f9'}
                />
                <text
                  x={node.x} y={node.y + 4}
                  textAnchor="middle" fontSize={11} fontWeight={700}
                  fill={isSelected ? '#ffffff' : '#64748b'}
                  style={{ fontFamily: 'monospace' }}
                >
                  {String(nodes.indexOf(node) + 1).padStart(2, '0')}
                </text>
                <text
                  x={node.x + 12} y={node.y + 22}
                  fontSize={13} fontWeight={600}
                  fill="#0f172a"
                  style={{ fontFamily: 'system-ui' }}
                >
                  {truncateText(node.step.name, 18)}
                </text>
                <text
                  x={node.x + 12} y={node.y + 40}
                  fontSize={11}
                  fill="#64748b"
                >
                  {truncateText(node.step.todo_title || `#${node.step.todo_id}`, 22)}
                </text>
                <text
                  x={node.x + 12} y={node.y + 56}
                  fontSize={10}
                  fill="#94a3b8"
                >
                  {node.step.todo_executor || '未指派'}
                </text>
                {node.step.min_rating != null && (
                  <text
                    x={node.x + NODE_WIDTH - 8} y={node.y + NODE_HEIGHT - 6}
                    textAnchor="end" fontSize={9}
                    fill="#f97316"
                    style={{ fontFamily: 'monospace' }}
                  >
                    闸门:{node.step.min_rating}
                  </text>
                )}
              </g>
            );
          })}
        </g>
      </svg>

      {/* Add button */}
      <div style={{ display: 'flex', justifyContent: 'center', marginTop: 8 }}>
        <div
          onClick={onAddStep}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => { if (e.key === 'Enter') onAddStep(); }}
          style={{
            display: 'flex', alignItems: 'center', gap: 6, padding: '6px 16px',
            border: '1px dashed var(--color-border, #e2e8f0)',
            borderRadius: 8, cursor: 'pointer',
            color: 'var(--color-text-tertiary, #94a3b8)',
            fontSize: 12,
            transition: 'border-color 200ms, color 200ms',
          }}
          onMouseEnter={(e) => { e.currentTarget.style.borderColor = '#0891b2'; e.currentTarget.style.color = '#0891b2'; }}
          onMouseLeave={(e) => { e.currentTarget.style.borderColor = '#e2e8f0'; e.currentTarget.style.color = '#94a3b8'; }}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <line x1="12" y1="5" x2="12" y2="19" /><line x1="5" y1="12" x2="19" y2="12" />
          </svg>
          添加环节
        </div>
      </div>
    </div>
  );
}

// 按字符数粗截断，避免环节名过长溢出卡片。
function truncateText(text: string, maxLen: number): string {
  return text.length > maxLen ? text.slice(0, maxLen - 1) + '…' : text;
}
