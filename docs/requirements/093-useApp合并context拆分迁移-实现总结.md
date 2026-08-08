# 093-useApp合并context拆分迁移-实现总结（批次 1）

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Pi) | 2026-08-08 | 批次 1 完成 |

> 对应设计：`docs/design/093-useApp合并context拆分迁移-设计.md`。093 优化扫描专项第 4 项。

## 1. 实现了什么

`useApp()` 合并三个域 state（todo/exec/ui），任一域变化 → 全部消费方重渲染。执行期间
`UPDATE_TASK_TODO_PROGRESS`/`UPDATE_TASK_EXECUTION_STATS`（每 10 条日志/工具调用即触发）
会让只读 workspace 的组件白白重渲染。

批次 1 迁移 **18 个消费方**到细粒度 hooks，它们不再因执行态推送重渲染：

| 迁移方式 | 文件 |
|---------|------|
| `useApp()` → `useTodos()`（仅读 selectedWorkspace/tags） | loop-list/index、LoopListView、RatingDistCard、LoopStatsCard、CronTodosCard、loop-kanban/index、RunningRecordDrawer、running-board/index、TodoListPage、TodoListView、TaskDetailPage、ExecutorsPanel、TodoCenterCardView、ReferencingLoopsSection、WikiChatFloatingWindow（SELECT_WORKSPACE）、MemorialBoard（SELECT_TODO）、SettingsPage（dispatch 透传 TagsPanel，prop 类型 any 且 ADD_TAG/DELETE_TAG 属 todo 域） |
| 双 hook 组合（实测跨域） | Dashboard：`useTodos()`（tags/selectedWorkspace）+ `useExecution()`（runningTasks），免订阅 uiState |

## 2. 实施期发现（已补录设计文档）

1. **Dashboard 跨域**：通过解构 `const { tags, runningTasks } = state` 访问执行域，直接访问审计（`state.X` grep）未覆盖解构模式，tsc 编译期暴露 → 双 hook 组合迁移。
2. **antd `App.useApp()` 陷阱**：Dashboard/loop-kanban 用 antd message API `App.useApp()`，机械替换误伤 → tsc 拦截，已修复并在两处加「勿混淆」注释。
3. **测试 mock 切换**：`TodoListPage.test.tsx`、`loop-list/index.test.tsx` 的 `vi.mock('@/hooks/useApp')` 随组件迁移改 mock `@/hooks/useTodoContext`（mock 形状不变）。

## 3. 测试与验证结果

- `npx tsc --noEmit`：零错误 ✅（dispatch action 域不匹配会在编译期暴露，本轮无）
- `npx vitest run`：32 文件 / 277 用例全绿 ✅
- Playwright 冒烟（`frontend/tests/093-useapp-batch1-smoke.spec.ts`，vite dev 5173 代理 18088）：
  首屏/列表页/看板渲染正常、零页面错误 ✅
- 残留审计：批次 1 文件无 `useApp()` 残留（Dashboard/loop-kanban 的两处为 antd `App.useApp()` message API，有意保留并注释）✅

## 4. 已知限制 / 后续批次

- **批次 2（后续 PR）**：TodoDetail（todo+exec）、KanbanBoard（todo+exec）、ExecutionPanel（exec+logs dispatch）。
- **永久保留 useApp**：App.tsx（组合根，真实消费全三域）、useExecutionEvents.ts（WS 事件全域 dispatch 路由）。
- 重渲染收益的量化（profiler 对比）未做，机制层面收益明确：18 个组件的渲染不再挂在 execState 变更链上。

## 5. 安全反思

纯前端状态订阅路径变更；dispatch 路由目标 reducer 不变（useTodos 的 dispatch 即 useApp 内部 todo 分支的同一个）；无接口/权限/数据流变化。
