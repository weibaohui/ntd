// CollapsiblePropertyPanel.tsx
// ---------------------------------------------------------------------------
// 096-W4-4-4：从 ProcessEditor.tsx 抽出的「可折叠属性面板」UI 块。
//
// 承接原主组件右侧属性面板的整段渲染（展开态工具条 + 内容区 / 收起态窄条），
// 连同其专属样式族与 getCollapsedPanelTitle 纯函数一并下沉，让主组件只负责编排。
//
// 设计取舍：
// - propertyPanelCollapsed 收为组件内部 state——它纯属视图偏好（看大图时收起），
//   父组件无需感知，故不下放为 prop（YAGNI）。
// - 不使用宽度过渡动画：过渡期间窄条几何位置不稳定、点击易落空，直接切换宽度更可靠
//   （原实现的踩坑结论，逐字保留）。
// - 收起态整条窄条都是一个 AntD Button：靠 Button 原生冒泡处理图标/文字点击，
//   无需 pointer-events hack。
// ---------------------------------------------------------------------------

import { useState, type CSSProperties, type JSX } from 'react';
import { Button, Empty } from 'antd';
import { LeftOutlined, RightOutlined } from '@ant-design/icons';
import type { ProcessDefinition } from '@/types/process';
import { ProcessPropertyPanel } from './ProcessPropertyPanel';

export interface CollapsiblePropertyPanelProps {
  // 工艺定义（React Flow source of truth）。null 表示 YAML 解析失败，面板兜底空态。
  definition: ProcessDefinition | null;
  // 当前选中节点 YAML id（null = 全局面板）。用于收缩栏标题与属性表单路由。
  selectedNodeId: string | null;
  // 属性字段变更回调（可视化 → definition 路径），向上转交 useProcessEditorState。
  onDefinitionChange: (definition: ProcessDefinition) => void;
}

// 收起态标题解析：与 ProcessPropertyPanel 的表单路由保持一致，
// 让收缩栏显示的名称就是展开后表单头部的名称（工艺属性/阶段属性/环节属性）。
// definition 为 null（YAML 未解析）时兜底为「工艺属性」。
// 导出供单测直接断言（纯函数，无 React 依赖）。
export function getCollapsedPanelTitle(
  definition: ProcessDefinition | null,
  selectedNodeId: string | null,
): string {
  // 未选中任何节点 → 全局面板，对应「工艺属性」
  if (selectedNodeId === null || definition === null) return '工艺属性';
  // 命中 phase → 「阶段属性」
  if (definition.phases?.some((p) => p.id === selectedNodeId)) return '阶段属性';
  // 命中任一 phase 下的 link → 「环节属性」
  for (const p of definition.phases ?? []) {
    if ((p.links ?? []).some((l) => l.id === selectedNodeId)) return '环节属性';
  }
  // 悬空引用（节点已删但 selectedNodeId 未清）→ 兜底工艺属性
  return '工艺属性';
}

export function CollapsiblePropertyPanel({
  definition,
  selectedNodeId,
  onDefinitionChange,
}: CollapsiblePropertyPanelProps): JSX.Element {
  // 收起状态：true = 收起为 32px 窄条（看大图时用），false = 正常 360px 面板。
  // 纯视图态，组件自管，不外泄。
  const [collapsed, setCollapsed] = useState(false);

  return (
    <div style={propertyPanelStyle(collapsed)}>
      {collapsed ? (
        // 收起态：整条窄条都是一个展开按钮（用 AntD Button，可靠处理点击），
        // 纵向排版：顶部向左箭头、下方竖排标题。
        // 箭头方向与展开态的向右箭头形成「→ 收起 / ← 展开」的对称语义。
        <Button
          type="text"
          aria-label="展开属性面板"
          title="展开属性面板"
          style={expandStripButtonStyle}
          onClick={() => setCollapsed(false)}
        >
          <div style={expandStripInnerStyle}>
            {/* 顶部：向左箭头作为展开触发器（整条可点，箭头只是示意） */}
            <LeftOutlined style={expandArrowStyle} />
            {/* 底部：竖排标题，左靠齐（贴左边缘），让用户看清这是哪个面板 */}
            <span style={expandTitleStyle}>
              {getCollapsedPanelTitle(definition, selectedNodeId)}
            </span>
          </div>
        </Button>
      ) : (
        // 展开态：顶部工具条（向右箭头收起）+ 面板内容
        <>
          <div style={panelToolbarStyle}>
            {/* 工具条标题：与收起态窄条标题保持一致，左靠齐，
                让用户一眼识别当前面板类型（工艺属性/阶段属性/环节属性）。 */}
            <span style={panelToolbarTitleStyle}>
              {getCollapsedPanelTitle(definition, selectedNodeId)}
            </span>
            <Button
              type="text"
              aria-label="收起属性面板"
              title="收起属性面板"
              style={collapseButtonStyle}
              onClick={() => setCollapsed(true)}
            >
              {/* 展开态用向右箭头：面板固定在右侧，点击后向右收缩成窄条，
                  箭头方向与面板退出的方向一致（常规语义）。 */}
              <RightOutlined />
            </Button>
          </div>
          <div style={panelBodyStyle}>
            {definition ? (
              <ProcessPropertyPanel
                definition={definition}
                selectedNodeId={selectedNodeId}
                onDefinitionChange={onDefinitionChange}
              />
            ) : (
              <Empty description="YAML 解析失败，无法显示属性面板" />
            )}
          </div>
        </>
      )}
    </div>
  );
}

// ── 属性面板专属样式族（随组件就近收口）──────────────────────────

// 右：属性面板。展开固定 360px；收起为 32px 窄条（只留展开箭头）。
// 注意：不使用宽度过渡动画。过渡会让面板在 200ms 内逐步收窄，
// 期间窄条几何位置不稳定，点击容易落空（表现为「前几次点击失效」）。
// 直接切换宽度更可靠，点击区域始终稳定。
function propertyPanelStyle(collapsed: boolean): CSSProperties {
  return {
    width: collapsed ? 32 : 360,
    height: '100%',
    // 纵向 flex：展开时工具条在上、内容区撑满剩余
    display: 'flex',
    flexDirection: 'column',
    // 面板背景，与可视化区区分（主题变量：亮色浅灰 / 暗色 surface0）
    background: 'var(--color-bg-card)',
    // 左边框分隔（主题变量）
    borderLeft: '1px solid var(--color-border)',
    // 收起态内容（窄条按钮）不允许溢出
    overflow: 'hidden',
  };
}

// 面板顶部工具条：左侧标题、右侧收起按钮，两端对齐。
// 标题与收起态窄条标题一致，保持展开/收起两种形态名称统一。
const panelToolbarStyle: CSSProperties = {
  display: 'flex',
  justifyContent: 'space-between',
  alignItems: 'center',
  padding: '8px 12px',
  borderBottom: '1px solid var(--color-border)',
  // 工具条不参与收缩，固定高度由内容决定
  flexShrink: 0,
};

// 工具条标题：与表单内原有大标题同级字号、加粗，但不再占表单垂直空间。
// 颜色用主题主文字变量，暗色下不再是写死的 slate-700。
const panelToolbarTitleStyle: CSSProperties = {
  fontSize: 16,
  fontWeight: 600,
  color: 'var(--color-text)',
  lineHeight: 1.4,
};

// 面板内容区：撑满工具条之外的剩余高度，滚动只发生在内容区。
const panelBodyStyle: CSSProperties = {
  flex: 1,
  overflow: 'auto',
};

// 收起按钮（AntD Button type=text）：小号透明按钮，hover 由 AntD 处理。
// 覆盖默认阴影/最小高度，保持工具条极简、与标题两端对齐。
const collapseButtonStyle: CSSProperties = {
  background: 'transparent',
  border: 'none',
  boxShadow: 'none',
  color: 'var(--color-text-secondary)',
  fontSize: 12,
  cursor: 'pointer',
  height: 'auto',
  padding: '2px 6px',
  lineHeight: 1.4,
};

// 收起态的整条展开按钮（AntD Button type=text）：铺满 32px 窄条全高，
// 整条可点让用户不用瞄准小图标。覆盖 AntD 默认内边距/最小高度/阴影，
// 确保按钮占满窄条且不被默认样式挤压。
const expandStripButtonStyle: CSSProperties = {
  width: '100%',
  height: '100%',
  padding: 0,
  background: 'transparent',
  border: 'none',
  boxShadow: 'none',
  color: 'var(--color-text-secondary)',
  fontSize: 12,
  cursor: 'pointer',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
};

// 收起态内部纵向容器：箭头贴顶、竖排标题贴底，两端分布。
// 这样在 32px 窄条里也能清晰呈现「标题 + 箭头」的收缩栏形态。
// 内层容器不拦截鼠标，点击由外层 AntD Button 统一处理（Button 原生会正确
// 把内部图标/文字的点击冒泡到自身，无需 pointer-events hack）。
const expandStripInnerStyle: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'center',
  justifyContent: 'space-between',
  width: '100%',
  height: '100%',
  padding: '10px 0',
};

// 收缩栏箭头：小号、低调颜色（主题次级文字），hover 由按钮整体承接交互。
const expandArrowStyle: CSSProperties = {
  fontSize: 12,
  color: 'var(--color-text-secondary)',
};

// 收缩栏标题：竖排（writing-mode vertical-rl）以便在中文字符下自然竖读，
// 贴左边缘、不换行。窄条宽度有限，竖排是唯一可读的呈现方式。
// 颜色用主题主文字变量，暗色下保持可读。
const expandTitleStyle: CSSProperties = {
  writingMode: 'vertical-rl',
  textOrientation: 'upright',
  letterSpacing: 2,
  fontSize: 13,
  color: 'var(--color-text)',
  whiteSpace: 'nowrap',
};
