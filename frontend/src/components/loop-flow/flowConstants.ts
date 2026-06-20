// Loop 流程图共享常量与虚拟节点 ID。
//
// 同时被 LoopFlowGraph、FlowEdge、FlowVirtualNodes 引用，集中在一处
// 避免修改时漏改导致尺寸对不上。

export const NODE_WIDTH = 180;
export const NODE_HEIGHT = 80;
export const RANK_SEP = 60;
export const NODE_SEP = 30;

export const VIRTUAL_NODE_RADIUS = 20;
export const VIRTUAL_NODE_SIZE = VIRTUAL_NODE_RADIUS * 2;

// 回环边弧顶距 dagre 内容顶部的距离。决定 SVG 顶部留白大小，
// 同时也是 buildEdgePath 中回环控制点的 Y 偏移（绝对值）。
export const LOOP_BACK_TOP_PADDING = 100;

export const START_NODE_ID = -1;
export const END_NODE_ID = -2;
