# 死代码扫描报告

**扫描日期**：2026-07-06
**扫描分支**：`chore/dead-code-scan`
**扫描工具**：
- 前端：`knip`（已临时安装执行，未入库）
- 后端：`grep`静态扫描 `#[allow(dead_code)]` 标记 + 待 clippy 全量编译验证

> ⚠️ **本报告仅是扫描结果，未做任何代码改动。** 清理死代码请另起 PR，分批进行，每批改完跑测试再下一批。

---

## 一、前端扫描结果

### 1. 未被引用的文件（16 个）

| 文件 | 说明 |
|---|---|
| `src/components/common/WorkspaceSelect.tsx` | 整文件未被引用 |
| `src/components/shell/QuickCaptureButton.tsx` | 整文件未被引用 |
| `src/components/skills/SkillTree.tsx` | 整文件未被引用 |
| `src/components/todo-detail/ContinuationLogsLoader.tsx` | 整文件未被引用 |
| `src/components/todo-detail/ContinuationLogView.tsx` | 整文件未被引用 |
| `src/components/todo-detail/NarrowLogView.tsx` | 整文件未被引用 |
| `src/utils/clipboard.ts` | 整文件未被引用 |
| `src/utils/device.ts` | 整文件未被引用 |
| `tests/debug_click.cjs` | 调试脚本，历史产物 |
| `tests/inspect.cjs` | 调试脚本，历史产物 |
| `tests/issue-648-mount.ts` | 已结 issue 调试脚本 |
| `tests/issue-652-mount.ts` | 已结 issue 调试脚本 |
| `tests/issue-657-mount.ts` | 已结 issue 调试脚本 |
| `tests/test_crash.cjs` | 调试脚本，历史产物 |
| `tests/test_crash.js` | 调试脚本，历史产物 |
| `tests/test_executor_config.cjs` | 调试脚本，历史产物 |

**建议**：源码侧 8 个文件可确认删除；测试脚本按 issue 编号核对后清理，若 issue 已闭环可删。

### 2. 未被引用的依赖（3 个）

| 类型 | 包 | 位置 |
|---|---|---|
| dep | `@xyflow/react` | `frontend/package.json:19` |
| dep | `clipboard` | `frontend/package.json:22` |
| devDep | `@types/clipboard` | `frontend/package.json:35` |

**建议**：先确认 `@xyflow/react` 是否被 Loop 流图相关组件使用（`loop-flow/` 目录可能动态引用），确认无引用后删除；`clipboard`/`@types/clipboard` 与 `utils/clipboard.ts` 配套，删文件后一并删依赖。

### 3. 未声明的依赖（2 处）

| 文件 | 行 | 缺失包 |
|---|---|---|
| `src/components/Dashboard.tsx` | 8:19 | `dayjs` |
| `src/components/dashboard/SpecialCards.tsx` | 3:28 | `dayjs` |

`dayjs` 实际在用但 `package.json` 未显式列出，靠传递依赖混进来。**建议**：补到 `package.json` 的 `dependencies`。

### 4. 未解析的测试导入（7 处）

测试文件引用了不存在的路径：

| 测试文件 | 引用的不存在路径 |
|---|---|
| `tests/backup-size-format-screenshots.spec.ts` | `/src/utils/format` |
| `tests/backup-size-format.spec.ts` | `/src/utils/format` |
| `tests/clipboard.spec.ts` | `/src/utils/clipboard.ts` |
| `tests/hook-pre-trigger.spec.ts` | `/src/utils/database/hooks` |
| `tests/issue-645-worktree-path-display.spec.ts` | `/src/types/execution.tsx` |
| `tests/issue-648-command-extractor.spec.ts` | `/src/utils/commandExtractor.ts` |
| `tests/kilo-executor.spec.ts` | `/src/types/execution` |

**建议**：这些测试本身要么已经失效要么路径过时，要么删测试要么补对应源码。

### 5. 未被引用的 export（123 个）

数量过多，分桶概览（详见下方附录）：

| 类别 | 数量 | 示例 |
|---|---|:--|
| 组件/页面 | ~30 | `CommandCard`、`ExecutionCard`、`TokenSummaryBar`、`SkillTree` 等 |
| 工具函数 | ~25 | `parseJsonSafe`、`isBashTool`、`groupBySession`、`parseUtcDate` 等 |
| 常量 | ~12 | `SWIPE`、`TIMER`、`TEXT_TRUNCATE`、`RESUMABLE_EXECUTORS` 等 |
| 主题/Provider | ~10 | `lightTheme`、`darkTheme`、`TodoProvider`、`useTodos` 等 |
| database 函数 | ~20 | `exportBackup`、`importBackup`、`createFeishuHistoryChat` 等 |
| logParserStrategy 类 | ~12 | `UserLogStrategy`、`ToolCallLogStrategy` 等 |

**建议**：逐文件核对，未引用的 export 要么删，要么补 `// @internal` 标注；策略类/Provider 类若预留给插件则保留。

### 6. 未被引用的 export type（54 个）

集中在 `src/types/` 与 `src/utils/database/`，多数是接口/类型预定义但当前未消费。
**建议**：先留着，类型删除风险较高（可能被隐式消费），可在下一轮结合运行时引用清理。

---

## 二、后端扫描结果

### 1. 显式 `#[allow(dead_code)]` 标记（15+ 处）

下列位置作者已显式标记「允许死代码」，需人工逐处判断是「真未用」还是「预留 API」：

| 文件 | 行 |
|---|---|
| `backend/src/models/loop_.rs` | 593 |
| `backend/src/cli/commands.rs` | 1070 |
| `backend/src/executor_service/pre_spawn.rs` | 255 |
| `backend/src/executor_service/types.rs` | 26 |
| `backend/src/sys.rs` | 117, 127, 134 |
| `backend/src/execution_events/pipeline.rs` | 233, 238 |
| `backend/src/handlers/skills.rs` | 168, 543 |
| `backend/src/services/feishu_history_fetcher.rs` | 48, 58, 60, 62, 435 |

**建议**：逐处阅读上下文，未用即删；预留 API 改用 `pub` 隐式暴露并加注释说明用途。

### 2. 全量 clippy 验证未完成

本次会话中 `cargo clean` 清空了 36GiB target 目录，全量重编耗时长，故未跑完 `cargo clippy --all-targets -D warnings`。
**建议**：清理前在本地完整跑一次 clippy，以 warning `dead_code` / `unused_imports` 为准。

---

## 三、清理建议优先级

| 优先级 | 项 | 收益 | 风险 |
|---|---|---|---|
| 🔴 高 | 删整文件 8 个源码 + 8 个调试脚本 | 立刻减体积 | 低，已确认无引用 |
| 🔴 高 | 补 `dayjs` 到 dependencies | 修复隐式依赖 | 极低 |
| 🟡 中 | 删未用依赖 `clipboard`/`@types/clipboard`/可能 `@xyflow/react` | 减 npm install 体积 | `@xyflow/react` 需先确认 |
| 🟡 中 | 后端逐处核对 `#[allow(dead_code)]` | 提高编译期检查覆盖 | 需读上下文 |
| 🟢 低 | 清未用 export（123 个）/ 未用 type（54 个） | 长期可维护性 | 删错会破坏隐式消费 |
| 🟢 低 | 失效测试（7 处不解析导入） | 测试树整洁 | 删测试要确认 issue 已结 |

---

## 附录：前端未用 export 完整清单（123 个）

（行末空白处为 knip 输出原行，保留以备核对）

### 组件/页面类
- `CommandCard` — `src/components/CommandPanel.tsx:115`
- `ExecutionCard` — `src/components/loop-kanban/index.tsx:4` 与 `src/components/LoopKanban.tsx:6:22`
- `KanbanColumn` — `src/components/loop-kanban/index.tsx:5` 与 `src/components/LoopKanban.tsx:6:37`
- `useLoopExecutions` — `src/components/loop-kanban/index.tsx:6` 与 `src/components/LoopKanban.tsx:6:51`
- `StepExecList` — `src/components/LoopStudioExecutionsPanel.tsx:6:49`
- `TokenSummaryBar` — `src/components/LoopStudioExecutionsPanel.tsx:6:63` 与 `src/components/loop-studio/executions/index.tsx:10`
- `ScheduledTodoCard`、`ExecutionRecordCard`、`RunningBoardColumnView`、`formatNextRunAt`、`COLUMN_ICONS` — `src/components/running-board/index.tsx:4-7` 与 `src/components/RunningBoard.tsx:6-7`
- `SkeletonRow`、`SkeletonList`、`TodoItemRow` — `src/components/todo-list/index.tsx:4-5` 与 `src/components/TodoList.tsx:6`
- `CollapsibleCommand`、`RatingControl`、`WorktreePathDisplay`、`ReplyRow`、`PostCard`、`ThreadGroup` — `src/components/todo-post/index.tsx:5-10`
- `WorktreePathDisplay`、`CollapsibleCommand`、`RatingControl` 等 6 个组件未消费
- `TodoSelectorForm`、`TagSelectorForm`、`FeishuMessageConfigForm`、`FeishuCommandConfigForm`、`TodoStateChangedConfigForm`、`TriggerConfigContent`、`CronConfigForm` — `src/components/loop-studio/triggers/index.tsx:12-18`
- `LogViewHeader` — `src/components/todo-detail/LogViewHeader.tsx:11`
- `CollapsibleCommand`、`RatingControl`、`WorktreePathDisplay`、`ReplyRow`、`PostCard`、`ThreadGroup` — `src/components/todo-post/index.tsx`

### 工具函数类
- `buildEdgePath`、`getEdgeMidX`、`getEdgeMidY` — `src/components/loop-flow/FlowEdge.tsx:100/145/167`
- `execStatusView`、`durationLabel`、`formatToken`、`formatCost` — `src/components/loop-studio/executions/index.tsx:13`
- `formatChatTime` — `src/components/wiki-chat/ChatMessageItem.tsx:45`
- `parseJsonSafe`、`isBashTool`、`__test__` — `src/utils/commandExtractor.ts:32/53/542`
- `groupBySession`、`formatLogTime`、`getElapsedSeconds` — `src/components/todo-post/index.tsx:11`
- `hasLogsStatic` — `src/components/todo-detail/helpers.ts:36` 与 `src/components/todo-post/helpers.ts:4`
- `getExecutorColor` — `src/components/skills/helpers.ts:5`
- `parseUtcDate` — `src/utils/datetime.ts:9`
- `getWorkspacePathById` — `src/utils/workspaceDisplay.ts:24`

### database 函数类（全部 export 但无 caller）
- `exportBackup`、`importBackup`、`exportSelectedBackup` — `src/utils/database/backup.ts:5/15/25`
- `createFeishuHistoryChat`、`updateFeishuHistoryChat`、`deleteFeishuHistoryChat`、`PENDING_CHAT_ID`、`getFeishuBindings`、`createFeishuBinding`、`deleteFeishuBinding`、`updateFeishuBindingEnabled` — `src/utils/database/bots.ts`
- `checkBackendHealth` — `src/utils/database/client.ts:16`
- `updateLoopTags`、`listTriggers`、`listLoopSteps`、`reorderLoopSteps`、`exportLoopsSelected` — `src/utils/database/loops.ts`
- `getReviewTemplate` — `src/utils/database/reviewTemplates.ts:32`
- `recordSkillInvocation`、`detectAllExecutors` — `src/utils/database/skills.ts:34/139`
- `forceUpdateTodoStatus`、`getSchedulerTodos`、`getRunningTodos` — `src/utils/database/todos.ts:65/246/255`

### 常量类
- `ACTIVE_TASKS_MIN_HEIGHT` — `src/components/dashboard/constants.ts:77`
- `VIRTUAL_NODE_SIZE`、`VIRTUAL_NODE_RADIUS` — `src/components/loop-flow/flowConstants.ts:12`、`src/components/loop-flow/FlowVirtualNodes.tsx:7`
- `SWIPE`、`TIMER`、`TEXT_TRUNCATE`、`COPY_FEEDBACK_DELAY`、`LAST_EXECUTOR_STORAGE_KEY` — `src/constants.ts:184/194/204/227/236`
- `RESUMABLE_EXECUTORS`、`RESUMABLE_EXECUTOR_OPTIONS` — `src/types/execution.tsx:16/18` 与 `src/utils/executors.tsx:92/98`（重复定义）
- `EXECUTOR_ALIASES` — `src/utils/executors.tsx:58`

### 主题/Provider 类
- `lightTheme`、`darkTheme`、`darkPalette`、`darkAccent` — `src/themes/index.ts:84/100/201/202`
- `useTodos`、`TodoProvider`、`useExecution`、`ExecutionProvider`、`useUI`、`UIProvider` — `src/hooks/useApp.tsx:12/14/16`

### logParserStrategy 类
- `ParsingContext`、`UserLogStrategy`、`AssistantLogStrategy`、`ThinkingLogStrategy`、`ToolCallLogStrategy`、`ToolResultLogStrategy`、`ResultLogStrategy`、`SystemLogStrategy`、`LOG_PARSER_STRATEGIES`、`createLogParsers`、`parseLogsWithStrategies` — `src/utils/logParserStrategy.ts`
