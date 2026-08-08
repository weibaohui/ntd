# 093-useApp合并context拆分迁移-设计

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI (Pi) | 2026-08-08 | 初始版本（批次 1） |
| AI (Pi) | 2026-08-08 | 实施修订：Dashboard 实测跨 todo+exec 两域（解构访问 runningTasks），按双 hook 组合迁移；两个测试文件 mock 目标同步切换 |

> 093 优化扫描专项第 4 项。091 已把 logs 拆出独立 LogsContext，但 `useApp()` 仍合并
> todoState/execState/uiState 三个域——任一域变化，所有消费方重渲染。
> 本设计把消费方分批迁移到细粒度 hooks（`useTodos`/`useExecution`/`useUI`）。

## 1. 现状盘点（逐文件 grep 实证）

真实消费方 **24 个文件**（此前估算 55 含 antd `App.useApp()` 误报）：

| 消费内容 | 文件数 | 明细 |
|---------|-------|------|
| 仅 `state.selectedWorkspace` / `state.tags`（todo 域） | **18** | Dashboard、SettingsPage、ReferencingLoopsSection、WikiChatFloatingWindow、loop-list/index、LoopListView、RatingDistCard、LoopStatsCard、CronTodosCard、loop-kanban/index、MemorialBoard、RunningRecordDrawer、running-board/index、TodoListPage、TodoListView、TaskDetailPage、ExecutorsPanel、TodoCenterCardView |
| todo + exec 域 | 3 | TodoDetail（ADD_EXECUTION_RECORD+SELECT_TODO）、KanbanBoard（executionRecords）、ExecutionPanel（exec + REMOVE_TASK_LOGS 走 logs 域） |
| 全三域 | 2 | App.tsx（组合根）、useExecutionEvents.ts（WS 事件路由 dispatch） |

重渲染放大机制：执行期间 `UPDATE_TASK_TODO_PROGRESS`/`UPDATE_TASK_EXECUTION_STATS`
（每 10 条日志或工具调用即触发）→ execState 变更 → `useApp` 的合并 state 换新 →
**全部 24 个消费方重渲染**，包括 18 个只读 workspace 的组件（它们根本不关心执行态）。

## 2. 批次划分

### 批次 1（本 PR）：18 个消费方（17 纯 todo 域 + Dashboard 双域组合）

> 实施修订：Dashboard 经解构 `const { tags, runningTasks } = state` 实际跨 todo+exec 两域
> （直接访问审计未覆盖解构模式，tsc 编译期暴露），按 `useTodos()` + `useExecution()`
> 双 hook 组合迁移，仍免订阅 uiState。迁移时注意区分 antd `App.useApp()`（message API）
> 与项目 `useApp()`——机械替换曾误伤两处，tsc 编译期拦截，已修复并加注释防再犯。

### 批次 1 原清单（17 个纯 todo 域消费方）

机械替换，零行为变化：

```ts
// 前
import { useApp } from '@/hooks/useApp';
const { state } = useApp();
// 后
import { useTodos } from '@/hooks/useTodoContext';
const { state } = useTodos();
```

dispatch 使用方同样安全（`useTodos()` 返回的 dispatch 即 todo 域 dispatch，类型精确）：
- WikiChatFloatingWindow：`SELECT_WORKSPACE`（todo 域 action）
- MemorialBoard：`SELECT_TODO`（todo 域）
- SettingsPage：`dispatch={dispatch}` 传给 TagsPanel（prop 类型 `any`，且 ADD_TAG/DELETE_TAG 是 todo 域 action）

收益：这 18 个组件不再因 execState（任务进度/统计推送）与 uiState 变化重渲染；
只剩 todo 域自身变化（切换 workspace/tag）才触发——这正是它们语义上关心的全部。

### 批次 2（后续 PR）：跨域 4 文件

TodoDetail（todo+exec）、KanbanBoard（todo+exec）、ExecutionPanel（exec+logs dispatch）、
SettingsPage 之外的遗留。每处需显式组合两个 hooks，改动面与回归面更大，单独 PR。

### 不迁移（永久保留 useApp）

- `App.tsx`：组合根，真实消费全三域（runningTasks 决定执行面板、loading 决定骨架屏），拆分无收益；
- `useExecutionEvents.ts`：WS 事件路由层，需向全域 dispatch，`useApp` 的合并 dispatch 正是为此设计。

## 3. 影响模块

仅前端 18 个组件文件 + 本设计文档。无接口/行为变化。

## 4. 验证方案

1. `npx tsc --noEmit` 零错误（dispatch action 类型不匹配会在编译期暴露）；
2. `npx vitest run` 全绿；
3. Playwright 冒烟：首页/列表页/看板/Dashboard 渲染正常，切换 workspace 联动正常；
4. 重渲染收益验证（文字推演 + 可选 profiler）：执行任务产生日志流时，Dashboard 卡片等
   18 个组件不再随 `UPDATE_TASK_EXECUTION_STATS` 重渲染。

## 5. 安全反思

纯前端状态订阅路径变更，无数据流/权限/接口变化；dispatch 的 action 路由目标域不变
（useTodos 的 dispatch 就是 useApp 内部 todo 分支的同一个 reducer dispatch）。
