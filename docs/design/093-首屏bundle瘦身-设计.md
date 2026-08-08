# 093-首屏bundle瘦身-设计

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Pi) | 2026-08-08 | 初始版本 |
| AI (Pi) | 2026-08-08 | 实施修订：补录构建期实测发现的三个隐藏依赖边（§1.3），manualChunks 最终函数式方案以 §2-C1 修订版为准 |

> 本专项源自 093 全量优化扫描（架构 + 性能），无独立需求文档。与 091 的关系：091 完成了页面级 React.lazy 与 manualChunks 拆分，但构建产物证据表明拆分未生效于首屏——本专项修复「拆而仍载」的根因。

## 1. 背景与问题（构建产物实测证据）

`frontend/dist/index.html` 的 modulepreload 列表（Vite 只为**入口 chunk 的静态依赖**注入 preload）：

```html
<script type="module" src="/assets/index-*.js"></script>          <!-- 862 KB -->
<link rel="modulepreload" href="/assets/vendor-antd-*.js">       <!-- 1.40 MB -->
<link rel="modulepreload" href="/assets/vendor-monaco-*.js">     <!-- 3.28 MB ❌ -->
<link rel="modulepreload" href="/assets/vendor-md-editor-*.js">  <!-- 1.18 MB ❌ -->
<link rel="modulepreload" href="/assets/vendor-flow-*.js">       <!-- 270 KB ❌ -->
<link rel="modulepreload" href="/assets/vendor-misc-*.js">
<link rel="modulepreload" href="/assets/vendor-icons-*.js">
<link rel="modulepreload" href="/assets/vendor-antd-icons-*.js">
```

首屏被迫并行下载 ~7.2MB JS（gzip 前），其中 monaco / md-editor / flow 共 ~4.7MB 本应按需加载。两条独立根因：

### 1.1 根因 A：manualChunks 对象写法导致 helper 提升

入口 chunk 产物中存在 `import{_ as we}from"./vendor-monaco-*.js"` —— Rollup 对象式 `manualChunks` 会把被列出模块的关联模块/共享 helper 合并进对应 vendor chunk，入口为了拿一个 helper 被迫静态依赖整个 monaco chunk（3.3MB）。vendor-flow 同理（扫描确认入口无任何静态 import 链指向 `@xyflow/react`/`monaco-editor`/`@monaco-editor/react`）。

### 1.2 根因 B：真实的静态导入链（扫描确认）

以下链路把 markdown 渲染/编辑库锚定进首屏静态图：

| 依赖 | 静态链（main.tsx 起可达） |
|------|--------------------------|
| `@ant-design/x-markdown` | `App.tsx` → `TodoDetailPage` → `TodoDetailActions` → `ActionButton` → `ChatView`（`import XMarkdown`，ChatView.tsx:13） |
| `@ant-design/x-markdown` | `App.tsx` → `HelpPage`（App.tsx:45 静态 import）→ `HelpContentRenderer`（:8） |
| `@ant-design/x-markdown` | `App.tsx` → `WikiChatFloatingWindow`（App.tsx:44 静态）→ `ChatMessageItem`（:9） |
| `@ant-design/x-markdown` | `App.tsx` → `TodoDetailPage` → `TodoDetail` → `todo-detail/PromptDisplay`（:2）/ `CollapsibleConclusion`（:22） |
| `@uiw/react-md-editor` | `App.tsx` → `TodoDrawer`（App.tsx:38 静态）→ `todo-drawer/PromptEditor` → `MdEditor.tsx:2` |

`BlackboardPage` / `WikiViewPage` 本身已是 lazy 页面，其 XMarkdown 引用不构成首屏链，但为彻底切断「未来谁静态引一下就回潮」的可能，一并改造。

### 1.3 实施期补录：三个扫描未覆盖的隐藏依赖边（构建产物实测）

1. **`vite/preload-helper` 提升**：函数式 manualChunks 下，Vite 注入的 `__vitePreload` helper（id 为 `\0vite/preload-helper.js`）被 Rollup 并入 vendor-monaco chunk，入口 `import{_ as xe}from"./vendor-monaco-*.js"` 只为引这 1KB helper。→ 显式归入 `vendor-runtime`（1.1KB）。
2. **merslim → dagre 静态链**：`App.tsx` → `HelpPage`（静态）→ `HelpContentRenderer` → `MermaidDiagram` → `merslim`（外部依赖 dagre）→ vendor-flow 被 preload。此前扫描漏判原因：merslim 的布局代码（`rankSeparation` 等）在 node_modules 内，不在 src 扫描面。→ `HelpContentRenderer` 中 MermaidDiagram 改 React.lazy（仅 mermaid 围栏代码块使用时加载）。
3. **React 与 antd 生态的 chunk 循环依赖**：函数式归类若把 react 留给自然分包，会被并入 vendor-antd；react-icons 顶层执行 `React.createContext` → `undefined.createContext` 白屏。同理 antd ⇄ @ant-design/icons 拆成两个 chunk 会形成 `Cannot access 'Yt' before initialization` 的 TDZ 循环。→ react/react-dom/scheduler/react-is 锚定 `vendor-react`；antd 全家桶（含 icons/cssinjs/rc-*/@rc-component）**必须同一 chunk**。

## 2. 设计（三个正交改动，可分 commit）

### C1：`vite.config.ts` manualChunks 改函数式

对象式 `manualChunks: { 'vendor-monaco': ['monaco-editor'] }` 在 Rollup 4 已弃用（构建时有 deprecation warning），且是 helper 提升的来源。改为函数式，只按模块路径精确归类，**不吞并依赖**：

```ts
manualChunks(id) {
  if (id.includes('vite/preload-helper')) return 'vendor-runtime';       // §1.3-1
  if (!id.includes('node_modules')) return undefined;                    // 项目源码交给 Rollup 按 lazy 边界自然分包
  if (/\/node_modules\/(react|react-dom|scheduler|react-is)\//.test(id)) return 'vendor-react'; // §1.3-3 防循环依赖
  if (id.includes('/monaco-editor/')) return 'vendor-monaco';
  if (id.includes('/@xyflow/') || id.includes('/dagre/')) return 'vendor-flow';
  if (id.includes('/@ant-design/x-markdown/')) return 'vendor-md-editor'; // 必须先于 antd 广义匹配
  if (id.includes('/@uiw/react-md-editor/') || id.includes('/@uiw/react-markdown-preview/')) return 'vendor-md-editor';
  // antd 全家桶（含 @ant-design/icons）同一 chunk，拆分必成 TDZ 循环（§1.3-3）
  if (id.includes('/antd/') || id.includes('/@ant-design/') || id.includes('/rc-') || id.includes('/@rc-component/')) return 'vendor-antd';
  if (id.includes('/react-icons/')) return 'vendor-icons';
  if (id.includes('/qrcode/') || id.includes('/react-countup/') || id.includes('/react-js-cron/')) return 'vendor-misc';
  return undefined;
}
```

- 匹配顺序即优先级：`x-markdown` 必须先于 `/antd/` 广义匹配；`react` 按目录边界精确匹配，不误伤 `react-icons` 等同缀包。
- `vendor-monaco` 仍保留命名 chunk：monaco 通过 `ProcessYamlEditor` 的动态 `import('monaco-editor')` 加载（091 已改），函数式归类后入口不再静态依赖它。
- 不引入 `onlyExplicitManualChunks` 显式开关：函数式写法下 Rollup 默认不合并依赖；保持配置面最小。

### C2：新增 `LazyXMarkdown` 共享懒加载包装组件

新建 `frontend/src/components/common/LazyXMarkdown.tsx`：

- `const XMarkdown = lazy(() => import('@ant-design/x-markdown'))` —— 包的 default 导出与命名导出 `XMarkdown` 是同一组件（见包 `es/index.d.ts`：`export { default, default as XMarkdown }`），因此 default / 命名两种引用方式的调用方都能无缝替换。
- props 直接透传：`React.ComponentProps<typeof XMarkdown>`，调用方零类型改动。
- `Suspense` fallback：以 `whiteSpace: pre-wrap` 的纯文本展示原始 markdown —— 本地 embedded 场景 chunk 加载为毫秒级，fallback 只是一瞬；纯文本兜底也保证 loading 期间内容可读。
- 替换全部 7 处静态引用：`ChatView.tsx`、`todo-detail/PromptDisplay.tsx`、`todo-detail/CollapsibleConclusion.tsx`、`wiki-chat/ChatMessageItem.tsx`、`help/HelpContentRenderer.tsx`、`BlackboardPage.tsx`、`WikiViewPage.tsx`。
- `HelpContentRenderer.tsx:9` 的 `import type { ComponentProps } ...` 是类型导入，构建期擦除，不产生运行时边，保留。

### C2.5（实施新增）：`HelpContentRenderer` 的 MermaidDiagram 懒加载

对应 §1.3-2：`MermaidDiagram` → merslim → dagre 是把 vendor-flow 错进首屏的静态链。改为 `lazy(() => import('./MermaidDiagram'))` + Suspense（fallback 为 mermaid 源码 `<pre>`）。仅帮助内容含 ```mermaid 围栏时才加载 ~87KB chunk。

### C3：`MdEditor.tsx` 内部懒加载 `@uiw/react-md-editor`

- `MdEditor.tsx` 本身就是项目内统一封装层（3 个调用方：`PromptEditor`、`DiscussionComposer`、`PromptMdField`），在封装层内部把 `MDEditor` 改为 `lazy(() => import('@uiw/react-md-editor'))` + `Suspense`，3 个调用方零改动。
- `editorRef` 透传：React 19 下函数组件 ref 走 prop，lazy 不破坏 ref 转发。
- fallback：antd `Spin` 居中的占位容器，高度沿用 `height` prop，避免抽屉打开瞬间布局跳动。

## 3. 影响模块

| 层 | 文件 |
|----|------|
| 构建 | `frontend/vite.config.ts` |
| 新增 | `frontend/src/components/common/LazyXMarkdown.tsx`（含单测） |
| 修改 | `MdEditor.tsx`、`ChatView.tsx`、`todo-detail/PromptDisplay.tsx`、`todo-detail/CollapsibleConclusion.tsx`、`wiki-chat/ChatMessageItem.tsx`、`help/HelpContentRenderer.tsx`、`BlackboardPage.tsx`、`WikiViewPage.tsx` |

## 4. 不做的事（YAGNI 边界）

- 不拆 `react`/`react-dom` vendor chunk（沿用 091 决策，它们在入口）。
- 不动 `BlackboardPage` 等 lazy 页面的内部结构，仅替换 XMarkdown 引用方式。
- 不引入 preact/htm 等替代渲染库 —— 问题是加载时机，不是库的体积。

## 5. 验证方案

1. `cd frontend && npx tsc --noEmit` 零错误。
2. `npm run build` 后检查 `dist/index.html`：
   - modulepreload **不得**包含 `vendor-monaco` / `vendor-md-editor` / `vendor-flow`；
   - 入口 chunk 无 `from"./vendor-monaco-*.js"` 静态 import；
   - 记录首屏 preload 总体积（目标：< 3MB，gzip 前）。
3. 新增单测 `LazyXMarkdown.test.tsx`（vitest + testing-library）：验证 fallback 纯文本 → 异步加载后渲染 markdown 内容。
4. `npx vitest run` 既有测试全绿。
5. Playwright（`make dev` 起 18088 后）：首页正常渲染；Todo 详情页结论区 markdown 渲染正常；打开 TodoDrawer 编辑器可用；帮助页渲染正常。spec 放 `frontend/tests/`。
6. 回归确认：工艺页（ProcessPage）Monaco YAML 编辑器首次打开仍可加载（动态 chunk 未被误删）。
