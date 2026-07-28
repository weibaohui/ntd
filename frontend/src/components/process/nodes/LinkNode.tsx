// LinkNode.tsx
// ---------------------------------------------------------------------------
// M4 里程碑：React Flow 自定义节点 — LinkNode（环节卡片）。
//
// 设计意图（对应 docs/design/029-M4-ReactFlow可视化编辑器-方案.md §3.1.2 + 设计 §5.2.2）：
// - 环节卡片，显示 link.name + step_template。
// - 右侧两个 source handle：上方绿色 on_success，下方橙色 on_gate_fail。
//   每个 handle 旁附文字小标签「成功 ✓」「失败 ✗」，颜色 + 文字双重标识，
//   避免用户只看两个圆点的颜色无法分辨哪个是成功、哪个是失败出口。
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
  // 删除 link 回调（卡片删除按钮触发）
  onDeleteLink: (linkId: string) => void;
  // React Flow 要求 data 是 Record<string, unknown> 兼容类型
  [key: string]: unknown;
}

// ── 颜色常量 ──────────────────────────────────────

// on_success handle 颜色（绿）
const ON_SUCCESS_COLOR = '#10b981';
// on_gate_fail handle 颜色（橙）
const ON_GATE_FAIL_COLOR = '#d97706';

// ── 出口 handle 纵向位置常量 ──────────────────────

// 两个出口 handle（及其文字标签）的纵向位置，用百分比相对卡片高度。
// handle 与标签共用同一百分比：无论卡片因内容多寡高度如何变化，二者始终同高对齐。
const SUCCESS_HANDLE_TOP = '30%';
const GATE_FAIL_HANDLE_TOP = '70%';

// ── 组件实现 ──────────────────────────────────────

// LinkNode 组件实现。
//
// 使用 React.memo 包裹避免不必要重渲染：
// React Flow 在拖拽时会频繁更新节点 props，memo 避免未变节点重渲染。
function LinkNodeImpl({ data, selected }: NodeProps): JSX.Element {
  const linkData = data as unknown as LinkNodeData;
  const { link, onSelectLink, onDeleteLink } = linkData;

  // 卡片点击：选中 link
  // stopPropagation 防止点击卡片时同时触发父 phase 的选中
  const handleClick = (e: MouseEvent) => {
    e.stopPropagation();
    onSelectLink(link.id);
  };

  // 删除按钮：阻止冒泡（避免同时触发选中），调删除回调
  const handleDeleteClick = (e: MouseEvent) => {
    e.stopPropagation();
    onDeleteLink(link.id);
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

      {/* 删除按钮：右上角红色 ×，删除该环节 */}
      <button
        type="button"
        aria-label="删除环节"
        title="删除环节"
        style={deleteButtonStyle}
        onClick={handleDeleteClick}
      >
        ×
      </button>

      {/* 左栏文字：环节名 + spec 模板引用。
          minWidth:0 让 flex 子项可收缩，配合子元素的 ellipsis 防止长文本撞到右侧出口标签。 */}
      <div style={textContentStyle}>
        <div style={nameStyle}>{link.name}</div>
        <div style={stepTemplateStyle}>
          {(link.step_template ?? []).map((s) => s.name).join('、') ||
            '无 spec 模板'}
        </div>
      </div>

      {/* 右侧出口标签：文字 + 颜色双重标识，绝对定位与各自 handle 圆点同高对齐。
          pointerEvents:none，不拦截鼠标，避免影响 handle 的拖拽连线。 */}
      <span style={exitLabelStyle(ON_SUCCESS_COLOR, SUCCESS_HANDLE_TOP)}>
        成功 ✓
      </span>
      <span style={exitLabelStyle(ON_GATE_FAIL_COLOR, GATE_FAIL_HANDLE_TOP)}>
        失败 ✗
      </span>

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

// 卡片样式：白色背景 + 边框 + 圆角。
// 宽度由父 phase 容器约束，高度自适应内容。
function cardStyle(selected: boolean): CSSProperties {
  return {
    // 卡片宽度 240px（与 processLayout.NODE_WIDTH 一致）
    width: 240,
    // 高度自适应内容，但至少 80px（与 NODE_HEIGHT 一致）
    minHeight: 80,
    // flex 布局：让左栏 textContentStyle(flex:1) 占据左侧空间，
    // 右侧 padding 专属留给「成功/失败」标签 + handle 圆点
    display: 'flex',
    // 白色背景
    background: '#fff',
    // 边框：默认灰色，选中时绿色加粗
    border: selected ? '2px solid #10b981' : '1px solid #e2e8f0',
    // 圆角
    borderRadius: 8,
    // 内边距：右侧加大到 70px，给出口标签 + handle 留出专属空间，
    // 避免左栏文字与右侧标签重叠
    padding: '12px 70px 12px 16px',
    // 防止内容溢出
    overflow: 'hidden',
    // 相对定位，handle / 标签均相对卡片绝对定位
    position: 'relative',
    // 鼠标指针：可点击
    cursor: 'pointer',
    // 阴影：轻微提升层次感
    boxShadow: '0 1px 3px rgba(0, 0, 0, 0.08)',
  };
}

// 左栏文字容器：flex:1 占据除右侧 padding 外的空间；
// minWidth:0 允许收缩，子元素的 text-overflow:ellipsis 才能生效。
const textContentStyle: CSSProperties = {
  flex: 1,
  minWidth: 0,
  overflow: 'hidden',
};

// link.name 样式：主标题，长名称截断省略避免撑破卡片
const nameStyle: CSSProperties = {
  fontSize: 14,
  fontWeight: 600,
  color: '#1e293b',
  marginBottom: 4,
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
};

// step_template 样式：副标题（小字灰色），同样截断省略
const stepTemplateStyle: CSSProperties = {
  fontSize: 12,
  color: '#94a3b8',
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
};

// ── Handle 样式 ───────────────────────────────────

// target handle 样式：左侧，灰色小圆点
const targetHandleStyle: CSSProperties = {
  background: '#64748b',
  width: 8,
  height: 8,
  border: '2px solid #fff',
};

// 删除按钮样式：绝对定位卡片右上角，红色 ×；z-index 确保浮在出口标签之上
const deleteButtonStyle: CSSProperties = {
  position: 'absolute',
  top: 2,
  right: 4,
  background: 'transparent',
  border: 'none',
  color: '#ef4444',
  fontSize: 14,
  cursor: 'pointer',
  padding: '0 4px',
  lineHeight: 1,
  zIndex: 2,
};

// on_success source handle 样式：右侧上方，绿色。
// top 用 SUCCESS_HANDLE_TOP，与「成功」标签共用，保证二者同高。
const successHandleStyle: CSSProperties = {
  background: ON_SUCCESS_COLOR,
  width: 10,
  height: 10,
  border: '2px solid #fff',
  top: SUCCESS_HANDLE_TOP,
};

// on_gate_fail source handle 样式：右侧下方，橙色。
// top 用 GATE_FAIL_HANDLE_TOP，与「失败」标签共用；top:auto 覆盖 React Flow 默认 top。
const gateFailHandleStyle: CSSProperties = {
  background: ON_GATE_FAIL_COLOR,
  width: 10,
  height: 10,
  border: '2px solid #fff',
  top: GATE_FAIL_HANDLE_TOP,
};

// ── 出口标签样式工厂 ──────────────────────────────

// 生成「成功/失败」小标签样式：绝对定位贴卡片右侧、与对应 handle 同高。
// color 为标签文字 + 底色来源（绿/橙），top 为纵向百分比（与 handle 共用同一常量）。
function exitLabelStyle(color: string, top: string): CSSProperties {
  return {
    position: 'absolute',
    // 距右边缘 16px：紧贴 handle 圆点（圆点贴 right:0）左侧，留出视觉间距
    right: 16,
    top,
    // 与 React Flow handle 的纵向居中对齐方式一致，让标签中线落在 top 处
    transform: 'translateY(-50%)',
    fontSize: 11,
    fontWeight: 600,
    lineHeight: 1.4,
    color,
    // 8 位 hex：在颜色后追加 '1f'(≈12% alpha) 生成浅色底，文字仍用原色保证可读
    background: `${color}1f`,
    borderRadius: 4,
    padding: '1px 6px',
    whiteSpace: 'nowrap',
    // 标签仅作展示，不拦截鼠标，避免影响 handle 的拖拽连线交互
    pointerEvents: 'none',
  };
}

// ── 导出 ──────────────────────────────────────────

// 用 memo 包裹导出
export const LinkNode = memo(LinkNodeImpl);
