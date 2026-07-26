// 所属环路标签组：把「事项被哪些启用环路引用」渲染为可点击标签。
//
// 从 TodoCenterCard 抽出（原为其内部私有组件），供两处复用：
// - 事项中心卡片（loop_driven 分类）：Tag 紧凑形态
// - 事项详情页「所属环路」区块：同一数据源、同一跳转语义
//
// 数据口径与后端 get_referencing_loops_for_todos 一致：只统计 enabled=1 的环节。

import { Tag } from 'antd';
import { RetweetOutlined } from '@ant-design/icons';
import type { LoopRefSummary } from '@/types';

interface ReferencingLoopsProps {
  /** 引用该事项的环路摘要列表（空数组时渲染兜底文案或 null，由调用方决定）。 */
  loops: LoopRefSummary[];
  /** 点击标签跳转环路详情。 */
  onSelectLoop: (loopId: number) => void;
  /** 无摘要时的兜底计数文案（事项中心卡片用）；不传则空列表时渲染 null。 */
  fallbackCount?: number;
}

export function ReferencingLoops({ loops, onSelectLoop, fallbackCount }: ReferencingLoopsProps) {
  if (loops.length === 0) {
    // 有计数兜底时展示「被 N 个启用环节引用」；
    // 详情页场景不传 fallbackCount，空引用直接不渲染（区块整体隐藏）。
    if (fallbackCount != null && fallbackCount > 0) {
      return (
        <div className="todo-center-card-meta-line">
          <span className="todo-center-card-meta-icon"><RetweetOutlined /></span>
          <span>被 {fallbackCount} 个启用环节引用</span>
        </div>
      );
    }
    return null;
  }
  return (
    <div className="todo-center-card-meta-line">
      <RetweetOutlined />
      {loops.map((l) => (
        <Tag
          key={l.loop_id}
          color="geekblue"
          style={{ cursor: 'pointer' }}
          onClick={(e) => {
            // 阻止冒泡：卡片整行有点击选中语义，标签点击专用于跳环路
            e.stopPropagation();
            onSelectLoop(l.loop_id);
          }}
        >
          {l.loop_name}
        </Tag>
      ))}
    </div>
  );
}
