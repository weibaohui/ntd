import { lazy, Suspense } from 'react';

/**
 * LazyXMarkdown — `@ant-design/x-markdown` 的懒加载共享包装（093）。
 *
 * 为什么存在：x-markdown 及其 markdown 依赖树约 600KB（gzip 前 1.18MB 的
 * vendor-md-editor chunk 的一半来源），而首屏静态链（App.tsx → TodoDetailPage /
 * HelpPage / WikiChatFloatingWindow → 各消费组件）把它锚定进了入口依赖图，
 * 导致用户打开首屏就必须下载。改为 React.lazy 后，该 chunk 只在首个 markdown
 * 内容真正渲染时才按需加载。
 *
 * 实现要点：
 * - 包导出核实：x-markdown 的 `es/index.d.ts` 为
 *   `export { default, default as XMarkdown }`，default 与命名导出是同一组件，
 *   因此无论调用方原来写 `import XMarkdown from` 还是 `import { XMarkdown } from`，
 *   都可直接替换为本组件，渲染行为完全一致。
 * - props 类型用 `React.ComponentProps<typeof XMarkdown>` 从 lazy 组件反推，
 *   与源组件 props 保持强一致（content / children / components / className 等全透传），
 *   调用方迁移时零类型改动。
 * - fallback 用纯文本兜底而非 Spin：markdown 正文在 chunk 加载的一瞬（本地 embedded
 *   场景为毫秒级）以 `pre-wrap` 纯文本呈现，内容始终可读，不会出现「内容区突然变
 *   加载圈再变回文字」的视觉跳动；fallback 高度与最终渲染接近，布局位移最小。
 */

// lazy 工厂顶格声明：模块加载只发生一次，后续渲染复用已解析的组件引用。
const XMarkdown = lazy(() => import('@ant-design/x-markdown'));

/** 与源组件完全一致的 props 类型（从 lazy 组件引用反推，避免手抄接口漂移）。 */
type LazyXMarkdownProps = React.ComponentProps<typeof XMarkdown>;

/** 从 props 中提取纯文本兜底内容：源组件支持 content 字符串与 string children 两种形态。 */
function plainTextOf(props: LazyXMarkdownProps): string {
  // content 是 x-markdown 的主输入形态，优先取它。
  if (typeof props.content === 'string') return props.content;
  // children 形态（如 `<XMarkdown>{text}</XMarkdown>`）仅兜底字符串；
  // 富节点 children 在 fallback 阶段无法安全降级，返回空串（加载一瞬后由真组件接管）。
  if (typeof props.children === 'string') return props.children;
  return '';
}

export function LazyXMarkdown(props: LazyXMarkdownProps) {
  return (
    <Suspense
      fallback={
        // pre-wrap 保留 markdown 原文的换行与缩进，loading 态也可读；
        // margin: 0 抵消浏览器对 pre 类排版的默认外边距，避免撑开父容器。
        <div style={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word', margin: 0 }}>
          {plainTextOf(props)}
        </div>
      }
    >
      <XMarkdown {...props} />
    </Suspense>
  );
}
