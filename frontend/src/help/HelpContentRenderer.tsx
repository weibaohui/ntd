// 帮助内容渲染器：md 源码 → XMarkdown + mermaid 拦截。
//
// 设计要点：
// 1. 复用项目已有的 @ant-design/x-markdown (XMarkdown)，不引新 md 渲染器。
// 2. XMarkdown 的 components prop 按 HTML tagName 映射，code 组件接收 lang/block props。
// 3. 拦截 ```mermaid 代码块，交给 MermaidDiagram 渲染；其他代码块走默认渲染。

import XMarkdown from '@ant-design/x-markdown';
import type { ComponentProps } from '@ant-design/x-markdown/lib/XMarkdown/interface';
import { MermaidDiagram } from './MermaidDiagram';

interface HelpContentRendererProps {
  /** md 源码。 */
  source: string;
}

/**
 * 把 md 源码渲染成带 mermaid 拦截的视图。
 *
 * @param props.source md 源码
 * @returns 渲染后的 React 节点
 */
export function HelpContentRenderer({ source }: HelpContentRendererProps) {
  // 自定义 code 渲染：mermaid 代码块交给 MermaidDiagram，其他走默认
  function CodeRenderer(props: ComponentProps) {
    // lang 是 marked 解析后的语言标识，如 "mermaid"、"typescript"
    const lang = props.lang ?? '';
    // children 是代码内容，可能是 string 或 ReactNode[]
    const children = props.children;
    const text = typeof children === 'string'
      ? children
      : Array.isArray(children)
        ? children.join('')
        : String(children ?? '');
    if (lang === 'mermaid') {
      return <MermaidDiagram chart={text} />;
    }
    // 非 mermaid 代码块：用原生 <code> 渲染，保留 lang 用于语法高亮
    return <code className={props.className}>{children}</code>;
  }

  return (
    <XMarkdown
      content={source}
      components={{ code: CodeRenderer }}
    />
  );
}
