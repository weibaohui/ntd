# 030-任务详情页 Tabs 布局重设计 — 实现总结

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| CodeBuddy | 2026-07-28 | 完成 TaskDetailPanel Tabs 布局重构 |

---

## 1. 与需求/设计文档的对应

- 对应需求来源：用户反馈任务详情页交互布局排版不佳。
- 对应设计文档：docs/design/030-任务详情页Tabs布局重设计-设计.md
- 采用布局方案：标签页 Tabs（经用户确认）。

---

## 2. 实现了什么

### 2.1 TaskDetailPanel 整体重构

将原单栏竖排全宽拆为两区：

1. 顶部条（DetailHeader）：左侧 #id + 任务标题 + 状态/复杂度 Tag + 模板/版本元信息；右侧「再次执行」主按钮（type="primary"）。跨 Tab 始终可见。
2. Tabs 区：三个 Tab 共用同一宽度。
   - 概览：基本信息（模板/版本/复杂度/状态）+ 需求描述 + 最近一次执行进度条（Progress，状态映射 success/exception/active/normal）。
   - 工艺要求：轻量化步骤列表，圆形序号徽标 + 技能/产物/门禁分组，去掉原「卡片套卡片」。
   - 执行历史：每项是一个带边框卡片，点整行即在**该项表头正下方内联展开** ProcessExecutionBoard（顶部分隔线 + 略深底色，同框呈一个整体），同一时刻仅展开一项（手风琴）；明确归属于哪个执行。

### 2.2 拆出的小组件（单一职责，函数体均 ≤ 30 行）

| 组件 | 职责 |
|------|------|
| DetailHeader | 顶部条：标题/元信息/再次执行 |
| StepMetaRow | 步骤内一组标签的容器（技能/产物/门禁复用） |
| StepItem | 单步：序号徽标 + 名称 + 三个 StepMetaRow |
| OverviewTab | 概览 Tab 内容 |
| ProcessTab | 工艺要求 Tab 内容 |
| ExecTab | 执行历史 Tab 内容（内联手风琴展开看板） |
| progressStatus | 执行状态 → Progress.status 映射 |
| gateLabel | 门禁类型 → 中文标签（沿用） |

主组件 TaskDetailPanel 仅负责数据加载、状态管理与两区编排。

### 2.3 样式模块（TaskDetailPanel.module.css，新增）

- 顶部条：flex 左右分布 + 窄屏 flex-wrap；底边线 var(--color-border)。
- Tabs 撑满高度：display:flex; flex-direction:column，.ant-tabs-content-holder flex:1; overflow:auto（min-height:0 关键）。
- 步骤序号徽标：--color-primary-light 底 + --color-primary 字。
- 步骤行底细分隔线 --color-border-light，最后一行无。
- 全部颜色用 CSS 变量，明暗模式自动适配。

### 2.4 交互细节

- 再次执行按钮移到顶部条右侧，type="primary"，跨 Tab 始终可达。
- 任务数据加载用 alive 标记 + .then/.catch/.finally，防御快速切换造成的竞态（晚返回的请求丢弃结果）。
- 「再次执行」成功后除刷新详情外，调用 onTriggered 让宿主（TasksPage）刷新列表保持口径一致。

### 2.5 验证（Playwright）

- 脚本：frontend/tests/check_task_detail_tabs.spec.ts（保留作冒烟用例）。
- 验证项：详情页出现 .ant-tabs；三个 Tab 标签（概览/工艺要求/执行历史）可见；「再次执行」按钮可见；三个 Tab 内容分别截图核对。
- 结果：1 passed。
- 脚本：frontend/tests/check_exec_inline_expand.spec.ts（内联手风琴校验）。
- 验证项：执行历史 Tab 下每个执行项是带边框卡片；点击某条整行后，ProcessExecutionBoard 在该条**正下方同框**展开；同一时刻仅一项展开。
- 结果：1 passed。

---

## 3. 关键实现点

- Tabs 高度撑满 + 内容区独立滚动：通过 CSS module :global(.ant-tabs) 设 flex column、.ant-tabs-content-holder flex:1; min-height:0; overflow:auto，避免面板内容撑高外层。
- 看板内联展开：ProcessExecutionBoard 不再嵌在窄栏，也不在 Tab 全宽区展开；而是作为执行卡片的内联详情，点击该执行整行即在表头正下方同框展开，明确归属。
- 步骤轻量化：用 StepMetaRow 复用行结构 + border-bottom 分隔线，去除嵌套 Card，视觉更干净。
- 进度条状态映射：progressStatus 把后端 success/running/failed/pending 映射到 antd Progress 的 success/active/exception/normal。

---

## 4. 文件改动清单

| 文件 | 类型 | 说明 |
|------|------|------|
| frontend/src/components/tasks/TaskDetailPanel.tsx | 重写 | 单栏竖排 → 顶部条 + Tabs |
| frontend/src/components/tasks/TaskDetailPanel.module.css | 新增 | 局部样式，主题变量驱动 |
| frontend/tests/check_task_detail_tabs.spec.ts | 新增 | Playwright 冒烟脚本 |
| frontend/tests/check_exec_inline_expand.spec.ts | 新增 | Playwright 内联手风琴展开校验 |
| docs/design/030-任务详情页Tabs布局重设计-设计.md | 新增 | 设计文档 |

未修改后端（不新增 API）。

---

## 5. 测试与验证结果

- npx tsc --noEmit：零错误。
- npm run build：构建成功，无新增告警。
- Playwright：任务详情页 Tabs 布局校验 1 passed（3.4s）；执行历史内联手风琴展开校验 1 passed。

---

## 6. 已知限制 / 待改进

1. Tabs ink-bar 过渡动画：截图捕获瞬间可能看到指示条位置滞后于内容切换（antd 内置 transition），属正常渲染中间态，不影响功能。
2. 暗色模式视觉：本次仅在浅色模式截图核对；暗色模式依赖主题 CSS 变量自动适配，理论上一致，建议人工复核。
3. Tabs 数量：当前固定 3 个；后续若新增维度（如产物/黑板）需评估是否仍放 Tabs 还是改其他导航。

---

## 7. 安全反思

- 复用既有 bundledApi.getTaskDetail / createTaskExecution，未引入新 API，未新增权限面。
- 任务标题/需求来自后端，React 默认转义，不使用 dangerouslySetInnerHTML。
- 「再次执行」Modal 输入非空校验沿用既有逻辑（message.warning），后端再做二次校验。