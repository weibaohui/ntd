// PhaseNode.tsx
// ---------------------------------------------------------------------------
// M4 里程碑：React Flow 自定义节点 — PhaseNode（泳道容器）。
//
// 设计意图（对应 docs/design/029-M4-ReactFlow可视化编辑器-方案.md §3.1.1 + 设计 §5.2.1）：
// - React Flow group 节点语义：position + width + height 定义容器边界。
// - 内部 LinkNode 通过 parentNode 挂到 PhaseNode 下，React Flow 自动处理
//   "拖动父节点移动子节点"。
// - 头部显示 phase.name + 右上角小叉号删除按钮。
//
// 视觉（设计 §5.2.1）：
// - 容器背景：rgba(phaseColor, 0.05)
// - 容器边框：1px dashed phaseColor
// - 头部：▸ {phase.name}，高度 60px
//
// 交互：
// - 点击头部 → 选中 phase → 右侧属性面板切换到 PhasePropertyForm
// - 点击删除按钮 → 弹 Modal.confirm（级联重置悬空 goto 引用，见 §5.5）
//
// 注意：PhaseNode 不渲染内部 LinkNode，React Flow 会自动渲染子节点。
// 这里只渲染容器边框 + 头部。
// ---------------------------------------------------------------------------

import { memo, type CSSProperties, type MouseEvent, type JSX } from 'react';
import { type NodeProps } from '@xyflow/react';
import type { PhaseDefinition } from '@/types/process';
import { PHASE_HEADER } from '../processLayout';

// ── 节点 data 类型 ─────────────────────────────────

// buildProcessGraph 注入的 data 结构
export interface PhaseNodeData {
  // phase 定义
  phase: PhaseDefinition;
  // phase 在 phases 数组中的索引
  phaseIndex: number;
  // 删除 phase 回调（弹 Modal.confirm）
  onDeletePhase: (phaseId: string) => void;
  // 选中 phase 回调（右侧属性面板切换）
  onSelectPhase: (phaseId: string) => void;
  // React Flow 要求 data 是 Record<string, unknown> 兼容类型
  [key: string]: unknown;
}

// ── 样式常量 ──────────────────────────────────────

// phase 容器背景色（半透明灰，避免遮挡子节点）
const PHASE_BG = 'rgba(148, 163, 184, 0.05)';
// phase 容器边框色（虚线灰）
const PHASE_BORDER = '#94a3b8';

// ── 组件实现 ──────────────────────────────────────

// PhaseNode 组件实现。
//
// 使用 React.memo 包裹避免不必要重渲染：
// React Flow 在拖拽时会频繁更新节点 props，memo 避免未变节点重渲染。
function PhaseNodeImpl({ data, selected }: NodeProps): JSX.Element {
  const phaseData = data as unknown as PhaseNodeData;
  const { phase, onDeletePhase, onSelectPhase } = phaseData;

  // 删除按钮点击：阻止事件冒泡（避免触发选中），调用删除回调
  const handleDeleteClick = (e: MouseEvent) => {
    // stopPropagation 防止点击删除按钮时同时触发头部选中
    e.stopPropagation();
    onDeletePhase(phase.id);
  };

  // 头部点击：选中 phase
  const handleHeaderClick = () => {
    onSelectPhase(phase.id);
  };

  return (
    <div style={containerStyle(selected)}>
      {/* 头部：phase.name + 删除按钮 */}
      <div
        style={headerStyle}
        onClick={handleHeaderClick}
      >
        <span style={headerTextStyle}>
          ▸ {phase.name}
        </span>
        <button
          style={deleteButtonStyle}
          onClick={handleDeleteClick}
          // type="button" 避免触发表单提交
          type="button"
          aria-label="删除阶段"
        >
          ×
        </button>
      </div>
      {/* phase 容器不需要 target handle，因为 phase 不接收连线 */}
      {/* phase 容器需要 source handle 吗？不需要，连线从 link 出发 */}
    </div>
  );
}

// ── 样式工厂 ──────────────────────────────────────

// 容器样式：虚线边框 + 半透明背景 + 头部高度
function containerStyle(selected: boolean): CSSProperties {
  return {
    // position: relative 确保内部子节点（React Flow 渲染）正确定位
    position: 'relative',
    // 虚线边框，选中时加粗
    border: selected ? '2px dashed #10b981' : `1px dashed ${PHASE_BORDER}`,
    // 半透明背景
    background: PHASE_BG,
    // 圆角
    borderRadius: 8,
    // 头部高度 + 内部 link 区域（由 React Flow 子节点填充）
    minHeight: PHASE_HEADER,
    // 宽度由 style.width 设定（buildProcessGraph 注入）
    width: '100%',
    height: '100%',
    // 防止内容溢出
    overflow: 'hidden',
  };
}

// 头部样式：固定高度 + flex 布局
const headerStyle: CSSProperties = {
  height: PHASE_HEADER,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  padding: '0 12px',
  borderBottom: `1px dashed ${PHASE_BORDER}`,
  cursor: 'pointer',
};

// 头部文字样式
const headerTextStyle: CSSProperties = {
  fontSize: 14,
  fontWeight: 600,
  color: '#475569',
};

// 删除按钮样式
const deleteButtonStyle: CSSProperties = {
  background: 'transparent',
  border: 'none',
  color: '#ef4444',
  fontSize: 18,
  cursor: 'pointer',
  padding: '0 4px',
  lineHeight: 1,
};

// ── 导出 ──────────────────────────────────────────

// 用 memo 包裹导出
export const PhaseNode = memo(PhaseNodeImpl);
