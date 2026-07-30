// Loop Studio 执行环节面板：DAG 流程图布局（044 起只读）。
//
// 044（环路瘦身）：环节定义只由工艺 install/upgrade 写入，不再提供
// 新增/编辑/删除/排序交互——要改环节请编辑工艺 YAML 后升级实例。
// 本面板只负责把环节流程图展示出来，节点上的事项标题仍可点击跳转。

import type { LoopStepDto } from '@/types/loop';
import { LoopFlowGraph } from '@/components/loop-flow/LoopFlowGraph';

interface StepsPanelProps {
  steps: LoopStepDto[];
  /** 点击流程图节点上的事项标题跳转事项详情；未注入时标题不可点击。 */
  onOpenTodo?: (todoId: number) => void;
}

export function LoopStepsPanel({ steps, onOpenTodo }: StepsPanelProps) {
  return (
    <LoopFlowGraph
      steps={steps}
      onOpenTodo={onOpenTodo}
    />
  );
}
