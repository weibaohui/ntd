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
import { Handle, Position, type NodeProps } from '@xyflow/react';
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
  // 新增环节回调（在 phase 内追加 link）
  onAddLink: (phaseId: string) => void;
  // React Flow 要求 data 是 Record<string, unknown> 兼容类型
  [key: string]: unknown;
}

// ── 样式常量 ──────────────────────────────────────

// phase 容器背景色（半透明灰，避免遮挡子节点）
const PHASE_BG = 'rgba(148, 163, 184, 0.05)';
// phase 容器边框色（虚线灰）
const PHASE_BORDER = '#94a3b8';

// 阶段流转 handle 样式：视觉隐藏，仅作 React Flow edge 连接点。
// 阶段顺序边由 builder 自动生成，不可由用户拖拽连线改动，故 pointerEvents:none。
const phaseHandleStyle: CSSProperties = {
  opacity: 0,
  pointerEvents: 'none',
  // 留 1px 尺寸，保证 React Flow 仍把它识别为有效连接点。
  width: 1,
  height: 1,
  // 纵向固定在头部第一行（phase 名称所在行），让阶段间箭头在顶部对齐
  top: PHASE_HEADER / 2,
};

// ── 组件实现 ──────────────────────────────────────

// PhaseNode 组件实现。
//
// 使用 React.memo 包裹避免不必要重渲染：
// React Flow 在拖拽时会频繁更新节点 props，memo 避免未变节点重渲染。
function PhaseNodeImpl({ data, selected }: NodeProps): JSX.Element {
  const phaseData = data as unknown as PhaseNodeData;
  const { phase, onDeletePhase, onSelectPhase, onAddLink } = phaseData;

  // 删除按钮点击：阻止事件冒泡（避免触发选中），调用删除回调
  const handleDeleteClick = (e: MouseEvent) => {
    // stopPropagation 防止点击删除按钮时同时触发头部选中
    e.stopPropagation();
    onDeletePhase(phase.id);
  };

  // 新增环节按钮点击：阻止事件冒泡，调用新增回调
  const handleAddLinkClick = (e: MouseEvent) => {
    e.stopPropagation();
    onAddLink(phase.id);
  };

  // 头部点击：选中 phase
  const handleHeaderClick = () => {
    onSelectPhase(phase.id);
  };

  return (
    <div style={containerStyle(selected)}>
      {/* 阶段流转连接点：左 target 流入 / 右 source 流出。
          仅作 edge 端点，视觉隐藏、不可交互（阶段顺序边由 builder 自动生成）。 */}
      <Handle type="target" id="phase-target" position={Position.Left} style={phaseHandleStyle} />
      <Handle type="source" id="phase-source" position={Position.Right} style={phaseHandleStyle} />
      {/* 头部：phase.name + 操作按钮 */}
      <div
        style={headerStyle}
        onClick={handleHeaderClick}
      >
        <span style={headerTextStyle}>
          ▸ {phase.name}
        </span>
        <span style={headerActionsStyle}>
          <button
            style={addLinkButtonStyle}
            onClick={handleAddLinkClick}
            type="button"
            aria-label="新增环节"
            title="新增环节"
          >
            + 环节
          </button>
          <button
            style={deleteButtonStyle}
            onClick={handleDeleteClick}
            type="button"
            aria-label="删除阶段"
          >
            ×
          </button>
        </span>
      </div>
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
    // 不设 overflow:hidden：group 容器需让子节点（LinkNode）溢出部分可见，
    // React Flow 用 absolute 定位挂子节点，overflow:hidden 会剪裁掉 link 节点
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

// 头部右侧操作按钮容器：横向排列，右对齐
const headerActionsStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: 8,
};

// 新增环节按钮样式：小号、浅蓝文字
const addLinkButtonStyle: CSSProperties = {
  background: 'transparent',
  border: '1px solid #93c5fd',
  borderRadius: 4,
  color: '#3b82f6',
  fontSize: 12,
  cursor: 'pointer',
  padding: '2px 6px',
  lineHeight: 1.4,
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
