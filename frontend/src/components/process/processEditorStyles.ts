// processEditorStyles.ts
// ---------------------------------------------------------------------------
// 096-W4-4-4：从 ProcessEditor.tsx 抽出的样式常量（纯机械搬运，零行为变化）。
//
// 这里只收「编辑器骨架」相关的容器/Tab/可视化区/YAML 区样式——它们被 ProcessEditor
// 主组件直接消费。属性面板那一族样式（收缩/展开/工具条）随 CollapsiblePropertyPanel
// 子组件就近收口，不集中到这里，避免一个 styles 文件同时服务两个消费方。
//
// 全部用 CSS 主题变量（var(--color-*)），暗色下随 data-theme 切换，不写死色值。
// ---------------------------------------------------------------------------

import type { CSSProperties } from 'react';

// 加载/空状态容器：居中。
// loading 与「工艺不存在」两种早退分支共用，故抽到模块级复用。
export const loadingContainerStyle: CSSProperties = {
  display: 'flex',
  justifyContent: 'center',
  alignItems: 'center',
  height: '100%',
  minHeight: 300,
};

// 编辑器主容器：纵向 flex，Alert 固定高度，双栏区填满剩余。
export const editorContainerStyle: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  height: '100%',
  width: '100%',
};

// Alert 样式：底部留间距，与下方 Tab 区分隔。
export const alertStyle: CSSProperties = {
  marginBottom: 12,
};

// 双栏布局：横向 flex。
// 备注：原注释提到用 position:absolute+inset:0 绕过 antd Tabs tabpane 链，但本组件
// 已弃用 antd Tabs 改手写按钮切换（见 ProcessEditor），故这里只需 flex:1 撑开 + minHeight
// 兜底，React Flow 由父代明确 flex 链获取尺寸。
export const splitViewStyle: CSSProperties = {
  flexDirection: 'row',
  flex: 1,
  minHeight: 400,
};

// Tab 按钮栏：横向 flex，底部边框分隔（仿 antd Tabs 视觉）。
// 边框用主题变量，暗色下随 data-theme 切换，不再写死亮色 slate。
export const tabBarStyle: CSSProperties = {
  display: 'flex',
  flexDirection: 'row',
  borderBottom: '1px solid var(--color-border)',
  padding: '0 4px',
};

// Tab 按钮：未激活态，次级文字 + 透明背景（主题变量，暗色下保持可读）。
export const tabButtonStyle: CSSProperties = {
  padding: '8px 16px',
  border: 'none',
  background: 'transparent',
  cursor: 'pointer',
  color: 'var(--color-text-secondary)',
  fontSize: 14,
};

// Tab 按钮：激活态，主色文字 + 底部主色高亮条（仿 antd Tabs 激活态）。
export const tabButtonActiveStyle: CSSProperties = {
  ...tabButtonStyle,
  color: 'var(--color-primary)',
  fontWeight: 500,
  boxShadow: 'inset 0 -2px 0 var(--color-primary)',
};

// Tabs 容器：填满剩余高度。
// overflow:hidden 让内容区剪裁，Tab 按钮栏不溢出覆盖可视化区；
// flex:1 + minHeight 让内部 React Flow 撑开（父代 splitViewStyle 已 flex:1）。
export const tabsStyle: CSSProperties = {
  flex: 1,
  display: 'flex',
  flexDirection: 'column',
  minHeight: 400,
  // 关键：让内容区（含 React Flow）正确剪裁，nav 不覆盖画布
  overflow: 'hidden',
};

// YAML 编辑器包装：撑满 Tab 内容区。
export const yamlEditorWrapperStyle: CSSProperties = {
  flex: 1,
  minHeight: 400,
};

// 左：可视化区，flex 1（占剩余宽度）。
// minWidth 防止属性面板过宽时把画布压到无法操作。
export const visualEditorStyle: CSSProperties = {
  flex: 1,
  minWidth: 400,
  height: '100%',
};
