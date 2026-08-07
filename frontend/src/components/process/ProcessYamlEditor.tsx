// ProcessYamlEditor.tsx
// ---------------------------------------------------------------------------
// M3 里程碑：Monaco YAML 编辑器封装组件。
//
// 设计意图（对应 docs/design/029-M3-Monaco-YAML编辑器-方案.md §2）：
// - 用 @monaco-editor/react 的 Editor 组件，自带 worker 加载与生命周期管理。
// - 通过 loader.config({ monaco }) 注入本地 monaco-editor，避免 CDN 依赖（离线可用 + 版本锁定）。
// - 错误标记用 deltaDecorations API 在行号槽（glyph margin）标红波浪线。
// - 主题由父组件传入（避免组件内重复 useTheme hook），映射 dark→vs-dark / light→vs。
// - 错误标记 CSS 通过 <style> 标签内联注入，避免新增全局 CSS 文件。
//
// 非目标（留给后续里程碑）：
// - M4 React Flow 可视化（这里只管 YAML 文本）
// - M5 双向联动 sync flag（这里只单向 onChange，不回写 ProcessDefinition）
// ---------------------------------------------------------------------------

import { useEffect, useRef, useState, type CSSProperties, type JSX } from 'react';
// @monaco-editor/react 默认从 CDN 加载 monaco-editor；
// 用 loader.config 注入本地 monaco-editor，实现离线可用 + 版本锁定。
import Editor, { loader } from '@monaco-editor/react';
// 091：monaco-editor 改为动态导入（不再 `import * as MonacoNamespace`）。
// 静态导入会把整个 monaco 树（核心 + 全部语言子包）打进主 bundle，首屏 ~5MB；
// 动态导入让 monaco 形成独立 chunk，仅在打开工艺 YAML 编辑器时按需加载。
import type * as Monaco from 'monaco-editor';
import { parseYaml, type YamlError } from './processYamlValidator';

// 确保只配置一次 monaco 实例；
// 多次调用 loader.config 会导致 worker 重复注册，引发编辑器初始化异常。
let monacoConfigured = false;
// 单飞 promise：多个 ProcessYamlEditor 实例并发挂载时，只触发一次动态导入，
// 其余实例 await 同一个 promise，避免重复 loader.config。
let monacoConfiguring: Promise<void> | null = null;
async function ensureMonacoConfigured(): Promise<void> {
  if (monacoConfigured) return;
  if (monacoConfiguring) return monacoConfiguring;
  monacoConfiguring = (async () => {
    // 动态拉取本地 monaco-editor 命名空间交给 loader，跳过 CDN 加载。
    const monaco = await import('monaco-editor');
    loader.config({ monaco });
    monacoConfigured = true;
  })();
  return monacoConfiguring;
}

export interface ProcessYamlEditorProps {
  // YAML 文本，由父组件控制（受控组件）
  value: string;
  // 文本变化回调
  onChange: (newText: string) => void;
  // 是否只读（系统工艺 true）
  readOnly: boolean;
  // 主题：'dark' | 'light'，由父组件从 useTheme() 传入
  theme: 'dark' | 'light';
}

// Monaco 主题名映射：
// dark → vs-dark（Monaco 自带暗色主题）
// light → vs（Monaco 自带亮色主题）
function monacoThemeName(theme: 'dark' | 'light'): string {
  return theme === 'dark' ? 'vs-dark' : 'vs';
}

// ProcessYamlEditor 组件实现。
//
// 内部状态：
// - editorRef：保存 Monaco editor 实例，用于 deltaDecorations
// - monacoRef：保存 monaco 命名空间引用，用于构造 Range
// - decorations：当前错误标记的 ID 列表，用于下次 deltaDecorations 清理旧标记
//
// 副作用：
// - mount 时保存 editor 引用
// - value 变化时调用 parseYaml 校验，失败则用 deltaDecorations 标红
export function ProcessYamlEditor({
  value,
  onChange,
  readOnly,
  theme,
}: ProcessYamlEditorProps): JSX.Element {
  // 091：monaco 改为动态导入，配置是异步的；用 monacoReady 门控 <Editor> 渲染，
  // 确保 loader.config 在 <Editor> 挂载前完成，避免它回退到 CDN 加载。
  const [monacoReady, setMonacoReady] = useState(monacoConfigured);
  useEffect(() => {
    // 已配置则无需等待；否则触发单飞动态导入，就绪后置位渲染编辑器。
    if (monacoConfigured) { setMonacoReady(true); return; }
    let active = true;
    ensureMonacoConfigured().then(() => { if (active) setMonacoReady(true); });
    return () => { active = false; };
  }, []);

  // Monaco editor 实例引用，用于调用 deltaDecorations
  const editorRef = useRef<Monaco.editor.IStandaloneCodeEditor | null>(null);
  // monaco 命名空间引用，用于构造 Range 和 Marker
  const monacoRef = useRef<typeof Monaco | null>(null);
  // 当前错误标记的 decoration ID 列表
  const [decorations, setDecorations] = useState<string[]>([]);

  // 错误标记副作用：value 变化时校验，失败则标红对应行
  // 依赖 [value]：每次 YAML 文本变化都重新校验
  useEffect(() => {
    const editor = editorRef.current;
    const monaco = monacoRef.current;
    // editor 或 monaco 未就绪时跳过（首次渲染前 onMount 尚未触发）
    if (!editor || !monaco) return;

    // 调用纯函数 parseYaml 校验 YAML
    const result = parseYaml(value);
    // 解析失败时用 deltaDecorations 标红错误行
    // deltaDecorations 返回新 decoration ID 列表，旧标记自动清理
    if (result.error) {
      const error: YamlError = result.error;
      // monaco.Range(startLine, startCol, endLine, endCol)，行号 1-based
      const range = new monaco.Range(error.line, 1, error.line, 1);
      const newDecorations = editor.deltaDecorations(decorations, [
        {
          range,
          options: {
            isWholeLine: true,
            // 整行背景标红（半透明，避免遮挡文本）
            className: 'yaml-error-line',
            // 行号槽（glyph margin）标红方块
            glyphMarginClassName: 'yaml-error-glyph',
            // hover 行号槽时显示错误消息
            glyphMarginHoverMessage: { value: error.message },
          },
        },
      ]);
      setDecorations(newDecorations);
    } else {
      // 解析成功：若有旧标记则清理
      if (decorations.length > 0) {
        const cleared = editor.deltaDecorations(decorations, []);
        setDecorations(cleared);
      }
    }
  }, [value]); // decorations 不放依赖：避免循环触发

  // Monaco 编辑器容器样式：填满父容器高度
  const containerStyle: CSSProperties = {
    height: '100%',
    width: '100%',
  };

  return (
    <div style={containerStyle}>
      {/* 错误标记 CSS：内联注入，避免新增全局 CSS 文件 */}
      <style>{`
        .yaml-error-line {
          background: rgba(248, 81, 73, 0.15) !important;
        }
        .yaml-error-glyph {
          background: #f85149 !important;
          border-radius: 3px;
          color: #fff;
        }
      `}</style>
      {/* 091：monaco 动态导入就绪前先占位，避免 <Editor> 在 loader 未配置时回退 CDN。
          loading 态保持轻量（纯 CSS 文案），与编辑器高度一致防止布局跳动。 */}
      {!monacoReady ? (
        <div style={{ ...containerStyle, display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--color-text-tertiary, #999)', fontSize: 12 }}>
          编辑器加载中…
        </div>
      ) : (
        <Editor
          height="100%"
          language="yaml"
          theme={monacoThemeName(theme)}
          value={value}
          onChange={(v) => onChange(v ?? '')}
          onMount={(editor, monaco) => {
            // 保存 editor 和 monaco 引用，供 deltaDecorations 使用
            editorRef.current = editor;
            monacoRef.current = monaco;
          }}
          options={{
            readOnly,
            minimap: { enabled: false },
            fontSize: 13,
            lineNumbers: 'on',
            glyphMargin: true, // 行号槽，用于标红
            scrollBeyondLastLine: false,
            automaticLayout: true, // 父容器尺寸变化时自动调整布局
            tabSize: 2, // YAML 惯用 2 空格缩进
            insertSpaces: true, // 用空格而非 Tab，避免 YAML Tab 报错
          }}
        />
      )}
    </div>
  );
}
