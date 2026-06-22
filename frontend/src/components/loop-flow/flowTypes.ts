// Loop 流程图共享类型。
import type { LoopStepDto } from '@/types/loop';

export type EdgeType =
  | 'start-first'
  | 'success-next'
  | 'success-goto'
  | 'fail-skip'
  | 'fail-goto'
  | 'fail-break'
  | 'end';

export interface LayoutEdge {
  from: string;
  to: string;
  label: string;
  type: EdgeType;
  fromId: number;
  toId: number;
  // 是否跳回到排在前面的环节（goto 的目标 step index < 源 step index）。
  // 这种「回环」用拱形弧线 + 加粗虚线 + 重试图标标识，跟普通跳转明显区分。
  isLoopBack?: boolean;
  // 是否跳转到自身（goto-self / 重试）。
  // 从边框右下→向下→向左→回到边框左下，用橙色折线 + 重试标签标识。
  isSelfLoop?: boolean;
}

export interface LayoutNode {
  id: number;
  x: number;
  y: number;
  step: LoopStepDto;
}
