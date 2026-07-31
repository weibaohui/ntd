// PromptMdField.tsx
// ---------------------------------------------------------------------------
// 需求 046：工艺编辑器 prompt 字段的 MD 编辑控件封装。
//
// 设计意图（对应 docs/design/046-工艺编辑器提示词MD编辑控件-设计.md §1.1）：
// - 复用 @/components/MdEditor（todo 页同款 @uiw/react-md-editor 封装），
//   把工艺属性面板中的大段 prompt 输入从 TextArea 升级为带工具栏的 MD 编辑器；
// - 收敛 LinkPropertyForm / GlobalPropertyForm 两处完全重复的「参数插入条」JSX，
//   两者的差异仅是参数数组，收敛后样式与交互只维护一份；
// - 参数插入由「尾部追加」升级为「光标处插入」，对齐 TodoDrawer.insertTextAtCursor
//   已在生产验证的既有模式。
//
// 数据流（受控，与 TextArea 时代同构，调用方零改动）：
//   MdEditor onChange → props.onChange → updateLinkField/浅克隆 → onDefinitionChange
// ---------------------------------------------------------------------------

import { useRef, type CSSProperties, type JSX, type ReactNode } from 'react';
import { Tooltip } from 'antd';
import { MdEditor } from '@/components/MdEditor';

// 可插入参数的描述结构：key 为点击后插入编辑器的占位符文本，desc 为 Tooltip 说明。
// 调用方的参数数组允许带多余字段（如 LinkPropertyForm 的 label），结构化类型天然兼容。
export interface PromptParam {
  key: string;
  desc: string;
}

export interface PromptMdFieldProps {
  // 受控文本；调用方用 ?? '' 兜底 undefined，与 TextArea 时代的受控契约一致
  value: string;
  // 变更回调：直通既有 handleFieldChange 链路，组件内不做任何加工
  onChange: (value: string) => void;
  // 可插入参数列表；缺省或空数组时不渲染参数条
  // （「提示词」字段现状就没有参数条，保持能力不扩张，YAGNI）
  params?: PromptParam[];
  // 参数条尾部扩展位（如评审 Prompt 的 DefaultReviewPromptButton）
  extraActions?: ReactNode;
  // 编辑器高度，默认 200（需求 046 锁定值，与 todo 页 PromptEditor 一致）
  height?: number;
}

// @uiw/react-md-editor 通过 useImperativeHandle 暴露 {...state, container, dispatch}，
// 其中 state.textarea 是内部 HTMLTextAreaElement（TodoDrawer 光标插入已依赖此形态）。
// 这里只声明组件真正依赖的字段，避免使用 any（前端禁止清单 #1）。
interface MdEditorHandle {
  textarea?: HTMLTextAreaElement;
}

// 尾部追加的纯函数：空串直接返回插入文本；末尾无换行时先补换行，
// 避免参数粘连在既有文本尾部导致 prompt 语义被破坏。
// 抽成独立导出纯函数，是为了单元测试可脱离 jsdom 编辑器环境直接断言边界行为。
export function buildAppendedText(prev: string, text: string): string {
  // 空值时无需任何分隔，直接以插入文本起步
  if (!prev) return text;
  // 已有换行结尾则不重复补，保证多次追加不会累积空行
  return prev.endsWith('\n') ? prev + text : prev + '\n' + text;
}

export function PromptMdField({
  value,
  onChange,
  params,
  extraActions,
  height = 200,
}: PromptMdFieldProps): JSX.Element {
  // 透传给 MdEditor 的 ref，用于读取/恢复内部 textarea 的光标位置
  const editorRef = useRef<MdEditorHandle>(null);

  // 参数点击 → 光标处插入。
  // 优先走 textarea 光标路径；ref 失效（未挂载/@uiw 内部结构变化）时兜底尾部追加，
  // 退化为改造前行为，保证功能可用不报错。
  const insertAtCursor = (text: string): void => {
    const textarea = editorRef.current?.textarea;
    if (!textarea) {
      onChange(buildAppendedText(value, text));
      return;
    }
    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    // 以最新受控 value 为单一数据源做切片插入，不读 DOM value，避免受控/DOM 不一致
    onChange(value.substring(0, start) + text + value.substring(end));
    // setTimeout(0)：等 React 把新 value 提交到 DOM 后再复位光标，
    // 否则光标会被受控重渲染冲到文本末尾（TodoDrawer 同款处理）
    setTimeout(() => {
      textarea.selectionStart = textarea.selectionEnd = start + text.length;
      textarea.focus();
    }, 0);
  };

  // hover 高亮提取为独立处理函数：避免在每个 code 标签上重复两坨内联事件体，
  // 也让 JSX 保持干净（组件编写规范 §4：复杂逻辑抽离）
  const handleParamMouseEnter = (e: React.MouseEvent<HTMLElement>): void => {
    // 用主题变量而非硬编码颜色，暗色主题下自动跟随（禁止清单 #10）
    e.currentTarget.style.borderColor = 'var(--color-primary)';
    e.currentTarget.style.color = 'var(--color-primary)';
  };
  const handleParamMouseLeave = (e: React.MouseEvent<HTMLElement>): void => {
    // 恢复默认配色，与 paramCodeStyle 中的初始值保持一致
    e.currentTarget.style.borderColor = 'var(--color-border-secondary)';
    e.currentTarget.style.color = 'var(--color-text-secondary)';
  };

  // 参数条的渲染条件：有参数或有扩展操作。
  // 只有 extraActions 没有参数时也渲染（按钮需要落点），两者皆无则整条不渲染
  const showParamsBar = (params && params.length > 0) || Boolean(extraActions);

  return (
    <div>
      <MdEditor
        value={value}
        onChange={onChange}
        height={height}
        editorRef={editorRef}
      />
      {showParamsBar && (
        <div style={paramsBarStyle}>
          {params && params.length > 0 && (
            <>
              <span style={paramsLabelStyle}>可用参数:</span>
              {params.map((p) => (
                <Tooltip key={p.key} title={p.desc}>
                  <code
                    onClick={() => insertAtCursor(p.key)}
                    style={paramCodeStyle}
                    onMouseEnter={handleParamMouseEnter}
                    onMouseLeave={handleParamMouseLeave}
                  >
                    {p.key}
                  </code>
                </Tooltip>
              ))}
            </>
          )}
          {extraActions}
        </div>
      )}
    </div>
  );
}

// ── 样式 ──────────────────────────────────────────
// 以下样式迁移自 LinkPropertyForm/GlobalPropertyForm 两处重复的参数条内联样式，
// 保持视觉完全一致；颜色全部走主题变量，暗色模式自动适配。

const paramsBarStyle: CSSProperties = {
  // 与编辑器拉开 8px 间距，可换行排列以适配 360px 窄面板
  marginTop: 8,
  display: 'flex',
  flexWrap: 'wrap',
  gap: 6,
  alignItems: 'center',
};

const paramsLabelStyle: CSSProperties = {
  fontSize: 12,
  color: 'var(--color-text-tertiary)',
  marginRight: 2,
};

const paramCodeStyle: CSSProperties = {
  fontSize: 11,
  padding: '1px 6px',
  borderRadius: 4,
  background: 'var(--color-fill-quaternary)',
  border: '1px solid var(--color-border-secondary)',
  cursor: 'pointer',
  color: 'var(--color-text-secondary)',
  transition: 'all 0.2s',
};
