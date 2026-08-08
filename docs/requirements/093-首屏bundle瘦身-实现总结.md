# 093-首屏bundle瘦身-实现总结

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Pi) | 2026-08-08 | 初始版本 |

> 对应设计：`docs/design/093-首屏bundle瘦身-设计.md`。本专项源自全量优化扫描（架构+性能），无独立需求文档。

## 1. 实现了什么

修复 091 懒加载拆分「拆而仍载」的遗留根因，首屏 JS preload 从 **~7.2MB 降到 2.31MB（-68%）**：

| 指标 | 优化前 | 优化后 |
|------|--------|--------|
| 首屏 preload 总量（min 前） | ~7.2 MB | **2.31 MB** |
| 其中 monaco | 3.28 MB（首屏 preload） | 0（工艺页按需） |
| 其中 md-editor 系 | 1.18 MB（首屏 preload） | 0（编辑/渲染时按需） |
| 其中 flow（xyflow+dagre） | 270 KB（首屏 preload） | 0（工艺/loop 页按需） |
| 入口 index chunk | 862 KB | 769 KB（merslim 迁出） |

优化后首屏 preload 清单（全部为首屏合法依赖）：`index 769KB + vendor-antd 1282KB + vendor-react 195KB + vendor-misc 60KB + vendor-icons 3KB + vendor-runtime 1.1KB`。

## 2. 与设计的对应关系

| 设计项 | 落地内容 | 状态 |
|--------|---------|------|
| C1 manualChunks 函数式 | `vite.config.ts` 改函数式；新增 `vendor-runtime`（preload-helper 锚定）与 `vendor-react`（防 TDZ 循环）；antd 全家桶合并单 chunk | ✅ |
| C2 LazyXMarkdown | 新增 `components/common/LazyXMarkdown.tsx` + 单测 4 例；替换全部 7 处静态引用 | ✅ |
| C2.5 MermaidDiagram 懒加载（实施新增） | `HelpContentRenderer.tsx` 改 lazy + Suspense（merslim→dagre 是 vendor-flow 首屏锚定的真实来源） | ✅ |
| C3 MdEditor 懒加载 | `components/MdEditor.tsx` 封装层内部 lazy，3 个调用方零改动；`PromptMdField.test.tsx` 3 例改 waitFor 适配异步就绪 | ✅ |

## 3. 关键实现点

- **三个隐藏依赖边**（扫描不可见、构建产物实测才暴露，已补录设计文档 §1.3）：
  1. `vite/preload-helper` 被 Rollup 并入 3.8MB vendor-monaco → 显式归入 1.1KB `vendor-runtime`；
  2. `merslim`（帮助页 mermaid 渲染库）外部依赖 dagre → MermaidDiagram 懒加载；
  3. react 自然分包并入 vendor-antd 后，react-icons 顶层 `React.createContext` 白屏；antd⇄icons 拆分会 TDZ → react 锚定 `vendor-react`，antd 全家桶单 chunk。
- **LazyXMarkdown 契约**：x-markdown 的 default 与命名导出是同一组件（包 `es/index.d.ts` 实证），故 7 处两种引用形态统一替换；fallback 用 `pre-wrap` 纯文本保证加载一瞬内容可读。
- **MdEditor ref 透传**：React 19 函数组件 ref 走 prop，lazy 不破坏 `editorRef` 转发链（PromptMdField 光标插入功能依赖）。

## 4. 测试与验证结果

- `npx tsc --noEmit`：零错误 ✅
- `npx vitest run`：33 文件 / 281 用例全绿（含新增 LazyXMarkdown 4 例、适配改造的 PromptMdField 3 例）✅
- `npm run build`：通过；`dist/index.html` preload 列表无 vendor-monaco / vendor-md-editor / vendor-flow；入口 chunk 无指向它们的静态 import ✅
- Playwright（`frontend/tests/093-first-screen-bundle.spec.ts`，对 18088 dev 实例）3 项全过 ✅：
  1. HTML preload 清单断言（无重型 chunk、antd 仍在防误伤）；
  2. 首屏渲染 + 首批网络请求无重型 chunk；
  3. 帮助抽屉（LazyXMarkdown + 懒加载 MermaidDiagram 叠加路径）markdown 正常渲染。

## 5. 已知限制 / 待改进

- `vendor-antd` 仍 1.28MB（gzip 391KB）：antd 全量引入是历史形态，按组件按需引入需评估 antd v6 的 ESM tree-shaking 实际收益，留待后续专项。
- 入口 index chunk 769KB 主要是业务代码（App 静态壳 + Dashboard/TodoList 等首屏页面），可再做页面级拆分，但收益递减，未做。
- monaco chunk 3.83MB 仍在产物中但仅工艺 YAML 编辑器打开时加载；`chunkSizeWarningLimit: 500` 的构建告警因此仍在，属预期（告警阈值未调，保留提醒价值）。
- 截图证据按规范发 PR 评论，不入库。

## 6. 安全反思

- 纯构建配置 + 组件加载方式变更，无新接口、无权限面变化、无输入处理逻辑变更。
- XMarkdown 的 DOMPurify 配置（`BlackboardPage` 的 `ALLOWED_URI_REGEXP`）原样透传，懒加载不改变 sanitize 行为。
- Suspense fallback 渲染的是用户已有权限查看的本地内容，无信息泄露面。
