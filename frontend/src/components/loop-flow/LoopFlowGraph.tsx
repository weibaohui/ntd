// Loop Studio 执行环节流程图。
//
// 布局：dagre 自动排列虚拟 Start/End + 真实 step 节点（逻辑已抽到 useFlowLayout）。
// 渲染：FlowEdge + FlowStepNode + StartNode / EndNode。
// 回环：当某环节失败要回到前面重做（fail-goto 目标 index < 源 index），
//       用正交折线（顶边出 + 顶边入）+ 加粗红色虚线 + 白底「↻ 重试」标签。
//
// 文件按 500 行硬限拆为：
// - LoopFlowGraph.tsx（本文件）：边分裂 + 主组装
// - useFlowLayout.ts：dagre 布局 hook（泛化，ProcessFlowGraph 也复用）
// - FlowStepNode.tsx：环节节点卡片 SVG 渲染
// - FlowEdge.tsx：单条边渲染与路径计算
// - FlowVirtualNodes.tsx：Start/End 节点
// - flowConstants.ts / flowTypes.ts：共享常量与类型

import { useMemo } from 'react';
import type { LoopStepDto } from '@/types/loop';
import {
  StartNode, EndNode,
} from '@/components/loop-flow/FlowVirtualNodes';
import { FlowStepNode } from '@/components/loop-flow/FlowStepNode';
import { FlowEdge, classifyEdge, resolveTargetStep } from '@/components/loop-flow/FlowEdge';
import { useFlowLayout } from '@/components/loop-flow/useFlowLayout';
import type { FlowNodeInput, FlowEdgeInput } from '@/components/loop-flow/useFlowLayout';
import {
  NODE_WIDTH, NODE_HEIGHT,
  START_NODE_ID, END_NODE_ID,
} from '@/components/loop-flow/flowConstants';
import type { LayoutEdge } from '@/components/loop-flow/flowTypes';

interface FlowGraphProps {
  steps: LoopStepDto[];
  selectedStepId?: number | null;
  /** 044 起环节只读：未注入时节点不可点、不渲染「添加环节」入口 */
  onSelectStep?: (step: LoopStepDto) => void;
  onAddStep?: () => void;
  /** 点击节点上的事项标题跳转事项详情（「环路 → 事项」向下钻取）；未注入时标题不可点击。 */
  onOpenTodo?: (todoId: number) => void;
}

export function LoopFlowGraph({
  steps,
  selectedStepId,
  onSelectStep, onAddStep, onOpenTodo,
}: FlowGraphProps) {
  // ── 1) 建 dagre 节点输入（不关心业务数据） ──
  const nodeInputs: FlowNodeInput[] = useMemo(() => steps.map(s => ({
    id: s.id, width: NODE_WIDTH, height: NODE_HEIGHT,
  })), [steps]);

  // ── 2) 环路专属边分裂（success / fail / end / loop-back） ──
  const { dagreEdges, layoutEdges, hasLoopBack, hasSelfLoop } = useMemo(() => {
    const dedges: FlowEdgeInput[] = [];
    const ledges: LayoutEdge[] = [];
    const stepIndexById = new Map<number, number>();
    steps.forEach((s, i) => stepIndexById.set(s.id, i));
    let selfLoop = false;
    let loopBack = false;

    const targetNameOf = (id: number) => steps.find(s => s.id === id)?.name || String(id);

    // Start → first
    if (steps.length > 0) {
      dedges.push({ from: START_NODE_ID, to: steps[0].id, label: '' });
      ledges.push({ from: String(START_NODE_ID), to: String(steps[0].id), label: '', type: 'start-first', fromId: START_NODE_ID, toId: steps[0].id });
    }

    for (const step of steps) {
      const si = stepIndexById.get(step.id) ?? 0;

      // 成功边
      const st = classifyEdge(step, steps, step.on_success, step.success_goto_step_id, true);
      const stg = resolveTargetStep(step, steps, step.on_success, step.success_goto_step_id);
      if (stg != null) {
        const ti = stepIndexById.get(stg);
        const isSelf = ti != null && ti === si;
        const isLB = !isSelf && st === 'success-goto' && ti != null && ti < si;
        if (isSelf) selfLoop = true;
        if (isLB) loopBack = true;
        if (!isSelf) dedges.push({ from: step.id, to: stg, label: '' });
        ledges.push({
          from: String(step.id), to: String(stg),
          label: isSelf ? '✅ 重试' : isLB ? `跳回 ${targetNameOf(stg)}` : step.on_success === 'goto' ? `✅→${targetNameOf(stg)}` : '',
          type: st, fromId: step.id, toId: stg, isLoopBack: isLB, isSelfLoop: isSelf,
        });
      }

      // 失败边（仅当策略不同于成功时绘制）
      // 044：min_rating 列已删，失败边按 on_rating_fail 策略直接绘制，标签不再带分数阈值
      if (step.on_rating_fail !== step.on_success) {
        const ft = classifyEdge(step, steps, step.on_rating_fail, step.fail_goto_step_id, false);
        const ftg = resolveTargetStep(step, steps, step.on_rating_fail, step.fail_goto_step_id);
        if (ftg != null) {
          const ti = stepIndexById.get(ftg);
          const isSelf = ti != null && ti === si;
          const isLB = !isSelf && ft === 'fail-goto' && ti != null && ti < si;
          if (isSelf) selfLoop = true;
          if (isLB) loopBack = true;
          if (!isSelf) dedges.push({ from: step.id, to: ftg, label: '' });
          ledges.push({
            from: String(step.id), to: String(ftg),
            label: isSelf ? '❌ 重试' : isLB ? `跳回 ${targetNameOf(ftg)}` : step.on_rating_fail === 'goto' ? `❌→${targetNameOf(ftg)}` : step.on_rating_fail === 'skip' ? '失败→继续' : '',
            type: ft, fromId: step.id, toId: ftg, isLoopBack: isLB, isSelfLoop: isSelf,
          });
        }
      }
    }

    // End 边
    for (const step of steps) {
      if (step.on_success === 'end' || step.on_rating_fail === 'end') {
        dedges.push({ from: step.id, to: END_NODE_ID, label: '' });
        ledges.push({ from: String(step.id), to: String(END_NODE_ID), label: '', type: 'end', fromId: step.id, toId: END_NODE_ID });
      }
    }
    if (!ledges.some(e => e.toId === END_NODE_ID) && steps.length > 0) {
      const lastId = steps[steps.length - 1].id;
      dedges.push({ from: lastId, to: END_NODE_ID, label: '' });
      ledges.push({ from: String(lastId), to: String(END_NODE_ID), label: '', type: 'end', fromId: lastId, toId: END_NODE_ID });
    }

    return { dagreEdges: dedges, layoutEdges: ledges, hasLoopBack: loopBack, hasSelfLoop: selfLoop };
  }, [steps]);

  // ── 3) dagre 布局 ──
  const { positions, width, height, startX, startY, endX, endY, dagreOffsetY } =
    useFlowLayout(nodeInputs, dagreEdges, hasSelfLoop, hasLoopBack);

  // ── 4) 构建带 step 数据的 LayoutNode 映射（供 FlowEdge 计算路径） ──
  const nodes = useMemo(() => steps.map((step) => {
    const p = positions.get(step.id) ?? { x: 0, y: 0 };
    return { id: step.id, x: p.x, y: p.y, step };
  }), [steps, positions]);

  // ── 5) 空态 ──
  if (steps.length === 0) {
    // 044 只读模式（onAddStep 未注入）：纯展示空态，不可点击
    if (!onAddStep) {
      return (
        <div
          style={{
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            minHeight: 120, width: '100%',
            border: '1px dashed var(--color-border, #e2e8f0)',
            borderRadius: 12,
            color: 'var(--color-text-tertiary, #94a3b8)',
            fontSize: 13,
          }}
        >
          <span>暂无执行环节</span>
        </div>
      );
    }
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

  // ── 6) 渲染 ──
  return (
    <div style={{ overflowX: 'auto', overflowY: 'hidden', padding: '12px 0', minHeight: 160 }}>
      <svg width={width} height={height} style={{ display: 'block' }}>
        <g transform={`translate(0, ${dagreOffsetY})`}>
          {/* 边 */}
          {layoutEdges.map((edge, i) => (
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
          {/* 虚拟节点 */}
          <StartNode x={startX} y={startY} />
          <EndNode x={endX} y={endY} />
          {/* 环节节点卡片 */}
          {nodes.map((node, i) => (
            <FlowStepNode
              key={`node-${node.id}`}
              step={node.step}
              index={i}
              x={node.x} y={node.y}
              selected={selectedStepId === node.id}
              onSelect={onSelectStep}
              onOpenTodo={onOpenTodo}
            />
          ))}
        </g>
      </svg>
      {/* 添加环节按钮（044：仅编辑模式注入 onAddStep 时渲染；只读模式不出现） */}
      {onAddStep && (
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
      )}
    </div>
  );
}
