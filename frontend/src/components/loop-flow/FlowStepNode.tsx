// 流程图节点卡片组件（loop instance 运行时样式）。
// 从 LoopFlowGraph 中抽出，使节点渲染独立于布局组装，
// ProcessFlowGraph 可以用自己的 TemplateStepCard 替换此组件。

import type { LoopStepDto } from '@/types/loop';
import {
  NODE_WIDTH, NODE_HEIGHT,
} from '@/components/loop-flow/flowConstants';
import { truncateText, phaseColor } from '@/components/loop-flow/useFlowLayout';

export interface FlowStepNodeProps {
  /** 环节 DTO */
  step: LoopStepDto;
  /** 环节在 dagre 布局中的序号（0-based） */
  index: number;
  /** SVG 左上角 x */
  x: number;
  /** SVG 左上角 y */
  y: number;
  /** 当前是否选中（编辑态） */
  selected?: boolean;
  /** 点击节点体 → 打开环节编辑弹窗；044 只读模式未注入时节点不可点 */
  onSelect?: (step: LoopStepDto) => void;
  /** 点击标题跳事项详情（G5 闭环）；未注入时标题不可点 */
  onOpenTodo?: (todoId: number) => void;
}

export function FlowStepNode({
  step, index, x, y, selected, onSelect, onOpenTodo,
}: FlowStepNodeProps) {
  return (
    <g
      onClick={onSelect ? () => onSelect(step) : undefined}
      style={onSelect ? { cursor: 'pointer' } : undefined}
    >
      {/* 阶段色带 */}
      {step.phase_id != null && (
        <rect
          x={x} y={y + 4}
          width={6} height={NODE_HEIGHT - 8}
          rx={3} ry={3}
          fill={phaseColor(step.phase_id)}
        />
      )}
      {/* 卡片背景：作为节点主体点击区域，承担 <g onClick> 的命中测试。
          删 pointerEvents:'none' —— 7a4d459 抽出时误加该属性导致卡片不接点击，
          而 SVG <g> 无原生形状不接事件，点击节点体落空，环节无法打开编辑窗口。 */}
      <rect
        x={x} y={y}
        width={NODE_WIDTH} height={NODE_HEIGHT}
        rx={8} ry={8}
        fill={selected ? '#f0f9ff' : '#ffffff'}
        stroke={selected ? '#0891b2' : '#e2e8f0'}
        strokeWidth={selected ? 2 : 1}
      />
      {/* 状态 dot（右上角） */}
      <circle
        cx={x + NODE_WIDTH - 10} cy={y + 10} r={4}
        fill={step.enabled ? '#22c55e' : '#94a3b8'}
      />
      {/* 序号 badg（左上角探出） */}
      <rect
        x={x - 10} y={y - 10}
        width={20} height={20} rx={10}
        fill={selected ? '#0891b2' : '#f1f5f9'}
      />
      <text
        x={x} y={y + 4}
        textAnchor="middle" fontSize={11} fontWeight={700}
        fill={selected ? '#ffffff' : '#64748b'}
        style={{ fontFamily: 'monospace' }}
      >
        {String(index + 1).padStart(2, '0')}
      </text>
      {/* 环节名 */}
      <text
        x={x + 12} y={y + 22}
        fontSize={13} fontWeight={600}
        fill="#0f172a"
        style={{ fontFamily: 'system-ui' }}
      >
        {truncateText(step.name, 18)}
      </text>
      {/* 事项标题（G5 可点击跳转） */}
      <text
        x={x + 12} y={y + 40}
        fontSize={11}
        fill={onOpenTodo ? '#0891b2' : '#64748b'}
        data-testid={onOpenTodo ? `flow-todo-link-${step.todo_id}` : undefined}
        style={onOpenTodo ? { cursor: 'pointer', textDecoration: 'underline', textUnderlineOffset: '2px' } : undefined}
        onClick={onOpenTodo ? (e) => {
          e.stopPropagation();
          onOpenTodo(step.todo_id);
        } : undefined}
      >
        {truncateText(step.todo_title ? `#${step.todo_id} ${step.todo_title}` : `#${step.todo_id}`, 24)}
      </text>
      {/* 执行者 */}
      <text
        x={x + 12} y={y + 56}
        fontSize={10}
        fill="#94a3b8"
      >
        {step.todo_executor || '未指派'}
      </text>
      {/* 阶段名（底部） */}
      {step.phase_name && (
        <text
          x={x + 12} y={y + NODE_HEIGHT - 8}
          fontSize={9}
          fill={phaseColor(step.phase_id!)}
          style={{ fontFamily: 'system-ui', fontWeight: 500 }}
        >
          {truncateText(step.phase_name, 16)}
        </text>
      )}
      {/* 已归档提示 */}
      {step.todo_archived_at && (
        <text
          x={x + 12} y={y + NODE_HEIGHT - 6}
          fontSize={9}
          fill="#ef4444"
          style={{ fontFamily: 'system-ui' }}
        >
          已归档
        </text>
      )}
    </g>
  );
}
