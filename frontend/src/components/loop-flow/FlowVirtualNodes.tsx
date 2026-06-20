// Loop 流程图虚拟节点 (Start / End)。
//
// Start/End 是 dagre 布局时插入的虚拟节点，不渲染真实的环节卡片，但需要
// 视觉提示给用户「这里是一切的起点/终点」。之所以独立成文件：让主文件
// LoopFlowGraph 保持在 500 行硬限内，虚拟节点视觉是独立的关注点。

export const VIRTUAL_NODE_RADIUS = 20;

interface VirtualNodeProps {
  x: number;
  y: number;
  selected?: boolean;
}

// 入口节点：绿色实心圆 + ▶ 符号，下方标 "开始"。
// 选中态改为更深的青色，呼应真实环节的选中样式。
export function StartNode({ x, y, selected = false }: VirtualNodeProps) {
  return (
    <g>
      <circle
        cx={x} cy={y} r={VIRTUAL_NODE_RADIUS}
        fill={selected ? '#0891b2' : '#22c55e'}
        stroke={selected ? '#0e7490' : '#16a34a'}
        strokeWidth={2}
      />
      <text
        x={x} y={y + 5} textAnchor="middle"
        fontSize={15} fontWeight={700} fill="#ffffff"
        style={{ fontFamily: 'system-ui' }}
      >
        ▶
      </text>
      <text
        x={x} y={y + VIRTUAL_NODE_RADIUS + 13} textAnchor="middle"
        fontSize={10} fontWeight={600}
        fill={selected ? '#0e7490' : '#16a34a'}
        style={{ fontFamily: 'system-ui' }}
      >
        开始
      </text>
    </g>
  );
}

// 出口节点：深灰色实心圆 + ■ 符号，下方标 "结束"。
// 故意用低饱和灰色，避免抢走入口节点的视觉重点。
export function EndNode({ x, y }: VirtualNodeProps) {
  return (
    <g>
      <circle
        cx={x} cy={y} r={VIRTUAL_NODE_RADIUS}
        fill="#475569" stroke="#334155" strokeWidth={2}
      />
      <text
        x={x} y={y + 5} textAnchor="middle"
        fontSize={15} fontWeight={700} fill="#ffffff"
        style={{ fontFamily: 'system-ui' }}
      >
        ■
      </text>
      <text
        x={x} y={y + VIRTUAL_NODE_RADIUS + 13} textAnchor="middle"
        fontSize={10} fontWeight={600} fill="#475569"
        style={{ fontFamily: 'system-ui' }}
      >
        结束
      </text>
    </g>
  );
}
