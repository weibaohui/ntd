// LinkNode.tsx
// ---------------------------------------------------------------------------
// M4 里程碑：React Flow 自定义节点 — LinkNode（环节卡片）。
//
// 设计意图（对应 docs/design/029-M4-ReactFlow可视化编辑器-方案.md §3.1.2 + 设计 §5.2.2）：
// - 环节卡片，显示 link.name + step_template。
// - 右侧两个 source handle：上方绿色 on_success，下方橙色 on_gate_fail。
// - 左侧一个 target handle：连线终点。
// - 通过 parentNode 挂到 PhaseNode 下，React Flow 自动处理拖动联动。
//
// 交互：
// - 点击卡片 → 选中 link → 右侧属性面板切换到 LinkPropertyForm
// - 从绿色 handle 拖出线 → 松开在目标 LinkNode 的 target handle 上
//   → 触发 onConnect → 更新 on_success: goto:<target_id>
// - 从橙色 handle 拖出线 → 同上 → 更新 on_gate_fail: goto:<target_id>
// ---------------------------------------------------------------------------

import { memo, type CSSProperties, type MouseEvent, type JSX } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import type { LinkDefinition } from '@/types/process';

// ── 节点 data 类型 ─────────────────────────────────

// buildProcessGraph 注入的 data 结构
export interface LinkNodeData {
  // link 定义
  link: LinkDefinition;
  // 所属 phase 的 id（YAML 里的 phase.id）
  phaseId: string;
  // phase 在 phases 数组中的索引
  phaseIndex: number;
  // link 在 phase.links 数组中的索引
  linkIndex: number;
  // 选中 link 回调（右侧属性面板切换）
  onSelectLink: (linkId: string) => void;
  // React Flow 要求 data 是 Record<string, unknown> 兼容类型
  [key: string]: unknown;
}

// ── 颜色常量 ──────────────────────────────────────

// on_success handle 颜色（绿）
const ON_SUCCESS_COLOR = '#10b981';
// on_gate_fail handle 颜色（橙）
const ON_GATE_FAIL_COLOR = '#d97706';

// ── 组件实现 ──────────────────────────────────────

// LinkNode 组件实现。
//
// 使用 React.memo 包裹避免不必要重渲染：
// React Flow 在拖拽时会频繁更新节点 props，memo 避免未变节点重渲染。
function LinkNodeImpl({ data, selected }: NodeProps): JSX.Element {
  const linkData = data as unknown as LinkNodeData;
  const { link, onSelectLink } = linkData;

  // 卡片点击：选中 link
  // stopPropagation 防止点击卡片时同时触发父 phase 的选中
  const handleClick = (e: MouseEvent) => {
    e.stopPropagation();
    onSelectLink(link.id);
  };

  return (
    <div style={cardStyle(selected)} onClick={handleClick}>
      {/* 左侧 target handle：连线终点 */}
      <Handle
        type="target"
        position={Position.Left}
        id="target"
        style={targetHandleStyle}
      />

      {/* 卡片内容：link.name + step_template */}
      <div style={nameStyle}>{link.name}</div>
      <div style={stepTemplateStyle}>
        {link.step_template ?? '无原型引用'}
      </div>

      {/* 右侧上方绿色 source handle：on_success 连线起点 */}
      <Handle
        type="source"
        position={Position.Right}
        id="on_success"
        style={successHandleStyle}
      />

      {/* 右侧下方橙色 source handle：on_gate_fail 连线起点 */}
      <Handle
        type="source"
        position={Position.Right}
        id="on_gate_fail"
        style={gateFailHandleStyle}
      />
    </div>
  );
}

// ── 样式工厂 ──────────────────────────────────────

// 卡片样式：白色背景 + 边框 + 圆角
// 宽度由父 phase 容器约束，高度自适应内容
function cardStyle(selected: boolean): CSSProperties {
  return {
    // 卡片宽度 240px（与 processLayout.NODE_WIDTH 一致）
    width: 240,
    // 高度自适应内容，但至少 80px（与 NODE_HEIGHT 一致）
    minHeight: 80,
    // 白色背景
    background: '#fff',
    // 边框：默认灰色，选中时绿色加粗
    border: selected ? '2px solid #10b981' : '1px solid #e2e8f0',
    // 圆角
    borderRadius: 8,
    // 内边距
    padding: '12px 16px',
    // 防止内容溢出
    overflow: 'hidden',
    // 相对定位，确保 handle 相对于卡片定位
    position: 'relative',
    // 鼠标指针：可点击
    cursor: 'pointer',
    // 阴影：轻微提升层次感
    boxShadow: '0 1px 3px rgba(0, 0, 0, 0.08)',
  };
}

// link.name 样式：主标题
const nameStyle: CSSProperties = {
  fontSize: 14,
  fontWeight: 600,
  color: '#1e293b',
  marginBottom: 4,
};

// step_template 样式：副标题（小字灰色）
const stepTemplateStyle: CSSProperties = {
  fontSize: 12,
  color: '#94a3b8',
};

// ── Handle 样式 ───────────────────────────────────

// target handle 样式：左侧，灰色小圆点
const targetHandleStyle: CSSProperties = {
  background: '#64748b',
  width: 8,
  height: 8,
  border: '2px solid #fff',
};

// on_success source handle 样式：右侧上方，绿色
// top: 25% 让 handle 偏上，与 on_gate_fail 错开
const successHandleStyle: CSSProperties = {
  background: ON_SUCCESS_COLOR,
  width: 10,
  height: 10,
  border: '2px solid #fff',
  top: '30%',
};

// on_gate_fail source handle 样式：右侧下方，橙色
// bottom: 30% 让 handle 偏下
const gateFailHandleStyle: CSSProperties = {
  background: ON_GATE_FAIL_COLOR,
  width: 10,
  height: 10,
  border: '2px solid #fff',
  bottom: '30%',
  top: 'auto', // 覆盖 successHandleStyle 的 top
};

// ── 导出 ──────────────────────────────────────────

// 用 memo 包裹导出
export const LinkNode = memo(LinkNodeImpl);
