# 093-useApp批次2迁移-实现总结

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Pi) | 2026-08-09 | 批次 2（收尾批次）完成 |
| AI (Claude) | 2026-08-23 | 评审整改：补迁漏网的 todo-post（useApp 消费方清零自此成立）、修正批次 1 遗留计数描述、冒烟 spec 端口 5173→18088、补 useAppDispatch/routeAppAction 单测 8 例、补「已知限制」章节 |

> 对应设计：`docs/design/093-useApp合并context拆分迁移-设计.md`（批次 1 文档已规划批次 2 范围）。
> 注：批次 1 遗留清单中的 KanbanBoard 已由其它会话先行迁移；本批次迁移剩余全部消费方，
> 并顺手完成「最后一公里」——WS 事件路由的零订阅 dispatch。

## 1. 实现了什么

| 文件 | 迁移内容 |
|------|---------|
| `TodoDetail.tsx` | 拆 `useTodos()`（selectedWorkspace/selectedTodoId/SELECT_TODO）+ `useExecution()`（executionRecords/runningTasks/ADD_EXECUTION_RECORD） |
| `ExecutionPanel.tsx` | 拆 `useTodos()`（selectedWorkspace）+ `useExecution()`（runningTasks/activeTaskId/executionRecords + REMOVE_RUNNING_TASK/SET_ACTIVE_TASK）+ `useLogsDispatch()`（REMOVE_TASK_LOGS） |
| `useExecutionHistory.ts` | dispatch prop 类型从三域联合收窄到 `Dispatch<ExecutionAction>`（hook 内只 dispatch 执行域 action） |
| `todo-post/index.tsx` | **评审补漏**：原漏迁的最后一处 `useApp()` 消费方——拆 `useTodos()`（selectedWorkspace，8 处）+ `useExecution()`（runningTasks），ui/logs 域变化不再牵动本页 |

### 最后一公里：dispatch-only context + useAppDispatch

批次 1 合入后 main 上 `useApp()` 实际剩 4 个消费方（ExecutionPanel、TodoDetail、todo-post、
useExecutionEvents——初版总结误记为 1 个），本批次全部迁移：前三个拆域订阅，
useExecutionEvents 借 dispatch-only context 解决。`useApp()` 内部订阅三域
state——只取 dispatch 也会被任一域变化卷入重渲染。本批次：

- Todo/Execution/UI 三个 context 各补 **dispatch-only 双 context**（沿用 091 LogsContext 先例）；
- 新增 `useAppDispatch()`：四域 dispatch 组合，零 state 订阅；
- 路由逻辑抽 `routeAppAction` 单一事实源，`useApp` 与 `useAppDispatch` 共用（防两份漂移）；
- `useExecutionEvents` 切换后：**WS 事件路由宿主组件不再被执行/日志高频更新卷入重渲染**。

## 2. 实施期发现

- main 已演进：tags action（SET_TAGS/ADD_TAG 等）已由组件本地状态接管，todo 域 action 只剩
  SELECT_TODO/SELECT_WORKSPACE——路由表按 main 现状逐字对齐；
- `useExecutionEvents.test.tsx` 的 mock 同步拆到 `./useApp`（useAppDispatch）与
  `./useTodoContext`（useTodos）两个模块。

## 3. 测试与验证

- `npx tsc --noEmit` 零错误 ✅；`npx vitest run` **55 文件 / 460 用例**全绿 ✅
  （评审新增 `useApp.test.tsx` 8 例：路由表逐 type 落域断言、dispatch 引用稳定性、
  4 个 dispatch hook 的缺 Provider 错误分支）
- Playwright 冒烟 2 项（列表→详情 TodoDetail 路径 / 执行面板挂载 + WS 路由）全过 ✅
  （评审整改后基于 `make dev` 18088 端口实测通过）

## 4. 最终状态

- 项目 `useApp()` 消费方：**0**（评审补迁 todo-post 后成立；仅保留 hook 本体与 AppProvider 组合根用途）；
- 三域 state 的订阅面全部细粒度化，执行态高频推送的重渲染半径收敛到真实使用方。

## 5. 安全反思

纯前端订阅路径变更；dispatch 路由表与原实现逐字对齐；无接口/行为变化。

## 6. 已知限制与待改进点

- **useExecutionEvents 非完全零订阅**：仍经 `useTodos()` 读 `selectedWorkspace`（094 的 WS
  重连作用域需要它），todo 域在 SELECT_TODO/SELECT_WORKSPACE 变化时仍会重渲染 WS 宿主——
  属低频操作，收益不受影响，但「只订阅 dispatch」的表述以代码注释自认的口径为准。
- **routeAppAction 静默丢弃未知 type**：路由表无 else 分支，未列出的 type 不分发也不报错。
  联合类型在编译期约束了合法值，风险低；`useApp.test.tsx` 已逐 type 固化路由表，新增
  action 忘记登记会直接测失败。
- **存量大文件未治理**：`TodoDetail.tsx`（534 行）、`useExecutionEvents.ts`（513 行）超
  禁止清单 #14 的 300 行线，均先于本批次存在；本批次重度编辑但未拆分，待后续专项。
- **冒烟 spec 端口**：初版误写 5173（vite 裸端口），评审已改 18088 对齐 `make dev` 流程。
