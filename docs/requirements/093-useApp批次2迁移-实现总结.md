# 093-useApp批次2迁移-实现总结

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Pi) | 2026-08-09 | 批次 2（收尾批次）完成 |

> 对应设计：`docs/design/093-useApp合并context拆分迁移-设计.md`（批次 1 文档已规划批次 2 范围）。
> 注：批次 1 遗留清单中的 KanbanBoard 已由其它会话先行迁移；本批次迁移剩余全部消费方，
> 并顺手完成「最后一公里」——WS 事件路由的零订阅 dispatch。

## 1. 实现了什么

| 文件 | 迁移内容 |
|------|---------|
| `TodoDetail.tsx` | 拆 `useTodos()`（selectedWorkspace/selectedTodoId/SELECT_TODO）+ `useExecution()`（executionRecords/runningTasks/ADD_EXECUTION_RECORD） |
| `ExecutionPanel.tsx` | 拆 `useTodos()`（selectedWorkspace）+ `useExecution()`（runningTasks/activeTaskId/executionRecords + REMOVE_RUNNING_TASK/SET_ACTIVE_TASK）+ `useLogsDispatch()`（REMOVE_TASK_LOGS） |
| `useExecutionHistory.ts` | dispatch prop 类型从三域联合收窄到 `Dispatch<ExecutionAction>`（hook 内只 dispatch 执行域 action） |

### 最后一公里：dispatch-only context + useAppDispatch

批次 1 后 `useApp()` 仅剩 `useExecutionEvents.ts` 一个消费方，但 `useApp()` 内部订阅三域
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

- `npx tsc --noEmit` 零错误 ✅；`npx vitest run` 54 文件 / 452 用例全绿 ✅
- Playwright 冒烟 2 项（列表→详情 TodoDetail 路径 / 执行面板挂载 + WS 路由）全过 ✅

## 4. 最终状态

- 项目 `useApp()` 消费方：**0**（仅保留 hook 本体与 AppProvider 组合根用途）；
- 三域 state 的订阅面全部细粒度化，执行态高频推送的重渲染半径收敛到真实使用方。

## 5. 安全反思

纯前端订阅路径变更；dispatch 路由表与原实现逐字对齐；无接口/行为变化。
