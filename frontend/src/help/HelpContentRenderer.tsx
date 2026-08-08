// 帮助内容渲染器：md 源码 → XMarkdown + mermaid 拦截。
//
// 设计要点：
// 1. 复用项目已有的 @ant-design/x-markdown (XMarkdown)，不引新 md 渲染器。
// 2. XMarkdown 的 components prop 按 HTML tagName 映射，code 组件接收 lang/block props。
// 3. 拦截 ```mermaid 代码块，交给 MermaidDiagram 渲染；其他代码块走默认渲染。

// 093：运行时组件改懒加载包装（HelpPage 被 App.tsx 静态引用，是首屏链之一）；
// 下方 type 导入在构建期擦除，不产生运行时依赖边，可保留。
import { lazy, Suspense } from 'react';
import { LazyXMarkdown } from '@/components/common/LazyXMarkdown';
import type { ComponentProps } from '@ant-design/x-markdown/lib/XMarkdown/interface';

// 093：MermaidDiagram 依赖 merslim → dagre（~270KB vendor-flow chunk 的首屏锚定来源）。
// mermaid 图只在帮助内容含 ```mermaid 围栏时才需要，懒加载后帮助页首屏不再背负布局库。
const MermaidDiagram = lazy(() =>
  import('./MermaidDiagram').then((m) => ({ default: m.MermaidDiagram })),
);

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
      // Suspense 兜底用原始 mermaid 源码的 <pre>：本地 embedded 场景 chunk 加载仅毫秒级，
      // 纯文本兜底保证加载一瞬内容可读，且与图表容器高度接近、布局不跳动。
      return (
        <Suspense fallback={<pre>{text}</pre>}>
          <MermaidDiagram chart={text} />
        </Suspense>
      );
    }
    // 非 mermaid 代码块：用原生 <code> 渲染，保留 lang 用于语法高亮
    return <code className={props.className}>{children}</code>;
  }

  return (
    <LazyXMarkdown
      content={source}
      components={{ code: CodeRenderer }}
    />
  );
}
