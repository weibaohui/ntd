# 死代码扫描报告

**扫描日期**：2026-07-19  
**扫描范围**：`backend/src/` + `frontend/src/` 全量引用分析  
**扫描方法**：grep 静态扫描 + 跨文件 import 关系追踪

> ⚠️ **本报告仅是扫描结果，未做任何代码改动。** 清理死代码请另起 PR，分批进行，每批改完跑测试再下一批。

---

## 一、后端 Rust 死代码

### 1. 🔴 `adapters/agent_event.rs` — 整文件死代码（112 行）

**文件路径**：`backend/src/adapters/agent_event.rs`

**问题描述**：该文件定义了通用事件结构体 `AgentEvent`、`AgentPart`、`AgentToolState`、`AgentToolInput`、`AgentTokens`、`AgentCacheTokens`，并声明为 `pub mod`，但 **没有任何其他文件 import 或使用它们**。

其他 adapter（kilo、opencode、zhanlu、mobilecoder、mimo、pi）都各自定义了独立的 `*_event.rs` 变体（如 `KiloAgentEvent`、`OpencodeAgentEvent`），而 `agent_event.rs` 作为通用基类却从未被引用。

**引用追踪**：
```
backend/src/adapters/mod.rs:302 → pub mod agent_event;  // 声明了模块
# 无任何文件 import agent_event::* 或 agent_event::AgentEvent
```

**建议**：直接删除整个文件及 `mod.rs` 中的 `pub mod agent_event;` 声明。

---

### 2. 🔴 `db/entity/feishu_homes.rs` — 整实体死代码

**文件路径**：`backend/src/db/entity/feishu_homes.rs`

**问题描述**：`feishu_homes` 实体只在以下位置出现：
- `src/db/entity/feishu_homes.rs` — 定义
- `src/db/entity/mod.rs` — `pub mod` + prelude export
- `src/db/migration/v1.rs` / `v2_v5.rs` — 建表 SQL

**没有任何业务代码查询或操作 `feishu_homes` 表**。飞书 home view 功能已废弃。

**引用追踪**：
```
# 全项目 grep "feishu_homes" 仅返回 8 行，全部在 entity/ 和 migration/ 中
# 无任何 handler/service 调用 feishu_homes 相关操作
```

**建议**：保留迁移 SQL（回滚需要），但 entity 定义和 prelude export 可删除。

---

### 3. 🔴 `db/feishu_group_whitelist.rs` — 整模块死代码（88 行）

**文件路径**：`backend/src/db/feishu_group_whitelist.rs`

**问题描述**：定义了 `get_feishu_group_whitelist_entries`、`upsert_feishu_group_whitelist_entry`、`delete_feishu_group_whitelist_entry` 等函数，但 **handlers 和 services 中没有任何调用**。entity 在 `feishu_group_whitelist.rs` 中也有定义。

**引用追踪**：
```
backend/src/db/mod.rs:319 → mod feishu_group_whitelist;  // 声明为私有模块
# 无任何 handler/service 调用 feishu_group_whitelist 相关函数
```

**建议**：删除 `db/feishu_group_whitelist.rs`、`db/entity/feishu_group_whitelist.rs` 及 `db/mod.rs` 中的 `mod feishu_group_whitelist;` 声明。

---

### 4. 🟡 `db/feishu_response_config.rs` 中的 `get_feishu_response_configs` 函数

**文件路径**：`backend/src/db/feishu_response_config.rs:57`

**问题描述**：该函数定义了但从未被任何地方调用：

```rust
pub async fn get_feishu_response_configs(&self, bot_id: i64) 
    -> Result<Vec<feishu_response_config::Model>, sea_orm::DbErr>
```

其余函数（`get_feishu_response_enabled`、`set_feishu_response_enabled`、`get_debounce_secs`、`set_debounce_secs`）均在 handler/service 中使用，仅 `get_feishu_response_configs` 是死代码。

**引用追踪**：
```
# get_feishu_response_enabled → handlers/agent_bot.rs:549-550, services/feishu_listener.rs:391
# set_feishu_response_enabled → handlers/agent_bot.rs:584-587
# get_debounce_secs → handlers/agent_bot.rs:553-554, services/message_debounce.rs:257
# set_debounce_secs → handlers/agent_bot.rs:590-593
# get_feishu_response_configs → 无任何调用方
```

**建议**：删除 `get_feishu_response_configs` 函数。

---

### 5. 🟡 `services/master_switch.rs` — 整模块死代码（97 行）

**文件路径**：`backend/src/services/master_switch.rs`

**问题描述**：定义了 `NTD_CONNECT_DISPATCHER_ENABLED` 环境变量开关和 `is_dispatcher_enabled()` 函数，但 **除了 `services/mod.rs` 的 `pub mod` 声明外，没有任何其他地方引用**。

**引用追踪**：
```
backend/src/services/mod.rs:13 → pub mod master_switch;
# 无任何文件调用 is_dispatcher_enabled() 或 _set_for_test()
```

**建议**：如果 ntd-connect dispatcher 功能已弃用，可删除整个文件及 `services/mod.rs` 中的声明。

---

### 6. 🟢 Entity prelude 中的未使用导出

**文件路径**：`backend/src/db/entity/mod.rs:62-64`

**问题描述**：prelude 中导出了 `UsageModelBreakdowns` 和 `UsageExecutorDaily`，但实际业务代码中从未使用这两个 prelude 别名（`db/usage.rs` 直接通过完整路径 `usage_model_breakdown::` 引用）。

**引用追踪**：
```
# prelude 导出：
src/db/entity/mod.rs:62 → pub use super::usage_model_breakdown::Entity as UsageModelBreakdowns;
src/db/entity/mod.rs:64 → pub use super::usage_executor_daily::Entity as UsageExecutorDaily;
# 无任何文件 import prelude::UsageModelBreakdowns 或 prelude::UsageExecutorDaily
# db/usage.rs 使用完整路径：use crate::db::entity::usage_model_breakdown;
```

**建议**：从 prelude 中移除这两个导出，减少 API 噪音。

---

### 7. 🟢 `handlers/sub_states.rs` — 空占位文件（2 行）

**文件路径**：`backend/src/handlers/sub_states.rs`

**内容**：
```rust
// sub_states — 占位模块，由 #604 引入。
// 当前无实质内容，保留声明以兼容 handlers/mod.rs 的 pub mod 引用。
```

**建议**：如果 issue #604 已关闭且不需要此模块，可删除。

---

## 二、前端 React 死代码

### 1. 🔴 `utils/logParserStrategy.ts` — 策略类死代码（约 200 行）

**文件路径**：`frontend/src/utils/logParserStrategy.ts`

**问题描述**：文件内的以下导出从未被 import，只有 `parseLogsToMessages` 函数被 `ChatView.tsx` 使用：

| 死代码 | 说明 |
|--------|------|
| `ParsingContext` | 策略解析上下文 |
| `UserLogStrategy` | 用户日志策略 |
| `AssistantLogStrategy` | 助手日志策略 |
| `ThinkingLogStrategy` | 思考日志策略 |
| `ToolCallLogStrategy` | 工具调用日志策略 |
| `ToolResultLogStrategy` | 工具结果日志策略 |
| `ResultLogStrategy` | 结果日志策略 |
| `SystemLogStrategy` | 系统日志策略 |
| `LOG_PARSER_STRATEGIES` | 策略注册表 |
| `createLogParsers` | 工厂函数 |

**引用追踪**：
```
# 仅 ChatView.tsx 使用了 parseLogsToMessages
frontend/src/components/ChatView.tsx:16 → import { parseLogsToMessages } from '@/utils/logParserStrategy';
# 以上 10 个策略类/函数无任何 import 方
```

**建议**：删除策略类导出，只保留 `parseLogsToMessages` 函数（或将其内联到 `ChatView.tsx`）。

---

### 2. 🔴 `utils/database/backup.ts` — 备份函数死代码

**文件路径**：`frontend/src/utils/database/backup.ts`

**问题描述**：以下函数从未被 import：

| 死代码 | 说明 |
|--------|------|
| `exportBackup` | 导出备份 |
| `importBackup` | 导入备份 |
| `exportSelectedBackup` | 导出选定备份 |

**引用追踪**：
```
# 全项目 grep "exportBackup\|importBackup\|exportSelectedBackup" 无调用方
```

**建议**：删除这三个函数。

---

### 3. 🔴 `utils/database/bots.ts` — 废弃函数死代码

**文件路径**：`frontend/src/utils/database/bots.ts`

**问题描述**：以下函数从未被 import：

| 死代码 | 说明 |
|--------|------|
| `getFeishuBindings` | 获取飞书绑定 |
| `createFeishuBinding` | 创建飞书绑定 |
| `deleteFeishuBinding` | 删除飞书绑定 |
| `updateFeishuBindingEnabled` | 更新飞书绑定启用状态 |
| `PENDING_CHAT_ID` | 待处理聊天 ID 常量 |
| `checkBackendHealth` | 检查后端健康状态 |

**引用追踪**：
```
# 全项目 grep 以上标识符，无任何 import 方
# 仅 createFeishuHistoryChat 和 deleteFeishuHistoryChat 被 WorkspaceSettingsPanel.tsx 使用
```

**建议**：删除上述 6 个死代码。

---

### 4. 🔴 `utils/database/todos.ts` — 废弃函数死代码

**文件路径**：`frontend/src/utils/database/todos.ts`

**问题描述**：以下函数从未被 import：

| 死代码 | 说明 |
|--------|------|
| `forceUpdateTodoStatus` | 强制更新事项状态 |
| `getRunningTodos` | 获取运行中的事项 |

**引用追踪**：
```
# getSchedulerTodos → 被 CronTodosCard.tsx 使用
# forceUpdateTodoStatus → 无调用方
# getRunningTodos → 无调用方
```

**建议**：删除 `forceUpdateTodoStatus` 和 `getRunningTodos`。

---

### 5. 🔴 `utils/database/loops.ts` — 废弃函数死代码

**文件路径**：`frontend/src/utils/database/loops.ts`

**问题描述**：以下函数从未被 import：

| 死代码 | 说明 |
|--------|------|
| `updateLoopTags` | 更新 Loop 标签 |
| `listTriggers` | 列出触发器 |
| `listLoopSteps` | 列出 Loop 步骤 |
| `reorderLoopSteps` | 重排序 Loop 步骤 |
| `exportLoopsSelected` | 导出选定的 Loops |

**引用追踪**：
```
# 全项目 grep 以上标识符，无任何 import 方
```

**建议**：删除这 5 个函数。

---

### 6. 🔴 `utils/database/skills.ts` — 废弃函数死代码

**文件路径**：`frontend/src/utils/database/skills.ts`

**问题描述**：以下函数从未被 import：

| 死代码 | 说明 |
|--------|------|
| `recordSkillInvocation` | 记录技能调用 |
| `detectAllExecutors` | 检测所有执行器 |

**引用追踪**：
```
# 全项目 grep 以上标识符，无任何 import 方
```

**建议**：删除这两个函数。

---

### 7. 🔴 `utils/database/reviewTemplates.ts` — 废弃函数死代码

**文件路径**：`frontend/src/utils/database/reviewTemplates.ts`

**问题描述**：以下函数从未被 import：

| 死代码 | 说明 |
|--------|------|
| `getReviewTemplate` | 获取评审模板 |

**引用追踪**：
```
# listReviewTemplates → 被 ReviewTemplatesPanel.tsx 和 TemplateCountCard.tsx 使用
# getReviewTemplate → 无调用方
```

**建议**：删除 `getReviewTemplate`。

---

### 8. 🟡 `utils/workspaceDisplay.ts` — `getWorkspacePathById` 死代码

**文件路径**：`frontend/src/utils/workspaceDisplay.ts:24`

**问题描述**：该函数被 export 但从未被 import。

**引用追踪**：
```
# 全项目 grep "getWorkspacePathById" 仅在定义文件中出现
```

**建议**：删除。

---

### 9. 🟡 `constants.ts` 中的未用常量

**文件路径**：`frontend/src/constants.ts`

**问题描述**：以下常量（非 export）定义但未被任何地方引用：

| 死代码 | 行号 | 说明 |
|--------|------|------|
| `SWIPE` | 184 | 滑动相关常量 |
| `TIMER` | 194 | 计时器相关常量 |
| `TEXT_TRUNCATE` | 204 | 文本截断相关常量 |
| `COPY_FEEDBACK_DELAY` | 227 | 复制反馈延迟 |

**引用追踪**：
```
# 全项目 grep 以上标识符，无任何引用方
```

**建议**：删除这四个常量。

---

### 10. 🟡 `types/stats.ts` 中的 `LeaderboardItem`

**文件路径**：`frontend/src/types/stats.ts:77`

**问题描述**：该 interface 被 export 但从未被 import。`EnhancedCards.tsx` 自己定义了一个同名局部接口。

**引用追踪**：
```
# types/stats.ts:77 → export interface LeaderboardItem { ... }
# EnhancedCards.tsx:89 → 自己定义了同名局部 interface LeaderboardItem
# 无任何文件 import LeaderboardItem from '@/types' 或 '@/types/stats'
```

**建议**：删除 `types/stats.ts` 中的 `LeaderboardItem` 接口。

---

### 11. 🟡 `types/execution.tsx` 中的 `RESUMABLE_EXECUTOR_OPTIONS`

**文件路径**：`frontend/src/types/execution.tsx:18`

**问题描述**：该常量被 export 但从未被 import。

**引用追踪**：
```
# types/execution.tsx:18 → export const RESUMABLE_EXECUTOR_OPTIONS = ...
# utils/executors.tsx:140 → 同样定义了 RESUMABLE_EXECUTOR_OPTIONS
# 全项目 grep "RESUMABLE_EXECUTOR_OPTIONS" 无 import 方
```

**建议**：删除 `types/execution.tsx` 中的 `RESUMABLE_EXECUTOR_OPTIONS`。

---

### 12. 🟡 `loop-flow/FlowEdge.tsx` 中的未用导出

**文件路径**：`frontend/src/components/loop-flow/FlowEdge.tsx`

**问题描述**：以下函数被 export 但从未被 import：

| 死代码 | 行号 | 说明 |
|--------|------|------|
| `buildEdgePath` | 100 | 构建边路径 |
| `getEdgeMidX` | 145 | 获取边中点 X |
| `getEdgeMidY` | 167 | 获取边中点 Y |

**引用追踪**：
```
# 全项目 grep "buildEdgePath\|getEdgeMidX\|getEdgeMidY" 仅在 FlowEdge.tsx 中出现
# 仅作为注释提及："同时也是 buildEdgePath 中回环控制点的 Y 偏移"
```

**建议**：删除这三个函数。

---

### 13. 🟡 `todo-detail/helpers.ts` 中的 `hasLogsStatic`

**文件路径**：`frontend/src/components/todo-detail/helpers.ts:36`

**问题描述**：该函数被 export 但从未被 import。

**引用追踪**：
```
# todo-detail/helpers.ts:36 → export function hasLogsStatic(record: ExecutionRecord): boolean
# 全项目 grep "hasLogsStatic" 仅在定义文件中出现
```

**建议**：删除。

---

## 三、测试文件残留

### 1. `tests/issue-648-mount.ts`

**文件路径**：`frontend/tests/issue-648-mount.ts`

**问题描述**：Issue #648 已关闭，此 mount 测试脚本已废弃。

**建议**：删除。

---

### 2. `tests/issue-652-mount.ts`

**文件路径**：`frontend/tests/issue-652-mount.ts`

**问题描述**：Issue #652 已关闭，此 mount 测试脚本已废弃。

**建议**：删除。

---

### 3. `tests/issue-657-mount.ts`

**文件路径**：`frontend/tests/issue-657-mount.ts`

**问题描述**：Issue #657 已关闭，此 mount 测试脚本已废弃。

**建议**：删除。

---

## 四、清理建议优先级

| 优先级 | 项 | 收益 | 风险 |
|---|---|---|---|
| 🔴 高 | 删后端 `adapters/agent_event.rs`、`feishu_homes` entity、`feishu_group_whitelist` 模块 | 减 ~300 行 | 低，已确认无引用 |
| 🔴 高 | 删前端策略类 `logParserStrategy.ts` 中 10 个未用导出 | 减 ~200 行 | 低，已确认无引用 |
| 🔴 高 | 删前端 `utils/database/` 中 18 个未用函数 | 减 ~150 行 | 低，已确认无引用 |
| 🟡 中 | 删后端 `services/master_switch.rs`（需确认 ntd-connect 是否弃用） | 减 ~100 行 | 需确认功能状态 |
| 🟡 中 | 删 3 个废弃测试文件 `issue-*-mount.ts` | 减 ~10 行 | 极低 |
| 🟡 中 | 删前端 `constants.ts` 中 4 个未用常量 | 减噪音 | 低 |
| 🟡 中 | 删前端 `types/` 中 2 个未用导出 | 减噪音 | 低 |
| 🟡 中 | 删前端 `FlowEdge.tsx` 中 3 个未用导出 | 减噪音 | 低 |
| 🟢 低 | 后端 `db/feishu_response_config.rs` 中 `get_feishu_response_configs` | 减 ~15 行 | 低 |
| 🟢 低 | 后端 prelude 中 2 个未用导出 | 减噪音 | 极低 |
| 🟢 低 | 后端 `handlers/sub_states.rs` 空占位文件 | 减 2 行 | 极低 |

---

## 五、此前报告已失效条目（已删除，仅供参考）

以下条目来自 2026-07-06 的 knip 扫描，经本次逐项验证确认**早已不存在**：

| 此前条目 | 当前状态 |
|---|---|
| `src/components/common/WorkspaceSelect.tsx` | ❌ 文件已删除 |
| `src/components/shell/QuickCaptureButton.tsx` | ❌ 文件已删除 |
| `src/components/skills/SkillTree.tsx` | ❌ 文件已删除 |
| `src/components/todo-detail/ContinuationLogsLoader.tsx` | ❌ 文件已删除 |
| `src/components/todo-detail/ContinuationLogView.tsx` | ❌ 文件已删除 |
| `src/components/todo-detail/NarrowLogView.tsx` | ❌ 文件已删除 |
| `src/utils/clipboard.ts` | ❌ 文件已删除 |
| `src/utils/device.ts` | ❌ 文件已删除 |
| `@xyflow/react` 未引用依赖 | ❌ 已从 package.json 移除 |
| `clipboard` / `@types/clipboard` 未引用依赖 | ❌ 已从 package.json 移除 |
| `dayjs` 未声明依赖 | ❌ 已在 package.json 中 |
| `backup-size-format` 相关测试文件 | ❌ 文件已删除 |
| `hook-pre-trigger.spec.ts` | ❌ 文件已删除 |
| `clipboard.spec.ts` | ❌ 文件已删除 |
| `issue-645-worktree-path-display.spec.ts` | ❌ 文件已删除 |
| `tests/` 下 8 个 `.cjs`/`.js` 调试脚本 | ❌ 已全部删除 |
| 123 个"未用 export" | ⚠️ knip 静态分析，大部分是 re-export 链或隐式消费，准确性存疑 |
