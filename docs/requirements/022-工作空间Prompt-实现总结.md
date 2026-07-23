# 022 工作空间 Prompt 实现总结

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AtomCode | 2026-07-24 | 初始版本 — 工作空间级 system_prompt 共识机制实现总结 |

---

## 1. 实现了什么

为每个 workspace 增加一份共享前置 prompt（`system_prompt`）。该 workspace 下任意 todo 执行时，执行器适配层把这段 prompt 拼到 message 最前面，使所有 todo 共享、遵守、共识同一份 workspace 上下文（产物目录、认证信息、基本文件路径等）。

最终 message 层次：

```
workspace 共识 prompt       ← 本期新增，最外层
  └─ 专家上下文              ← 既有 inject_expert_context
      └─ 原 todo message     ← 既有 substitute_message_placeholders
```

---

## 2. 与需求的对应关系

| 需求条目（022-*‑需求.md §5） | 实现位置 | 状态 |
|------------------------------|----------|------|
| DB 层新增 system_prompt 列 | `db/migration/v70.rs` | ✅ |
| Entity 加 system_prompt 字段 | `db/entity/workspace_settings.rs` | ✅ |
| upsert_workspace_settings 加参数 | `db/workspace_setting.rs` | ✅ |
| 所有 upsert 调用点同步 | `handlers/agent_bot.rs`, `services/feishu_listener.rs` ×2 | ✅ |
| Handler get 返回 system_prompt | `handlers/agent_bot.rs::get_workspace_settings` | ✅ |
| Handler update 透传 system_prompt | `handlers/agent_bot.rs::update_workspace_settings` | ✅ |
| 执行器注入 workspace prompt | `executor_service/pre_spawn.rs::inject_workspace_prompt` | ✅ |
| 编排层接入（prepare_execution_state 第 4.5 步） | `executor_service/stages.rs` | ✅ |
| 前端类型扩展 | `utils/database/bots.ts` | ✅ |
| 前端 UI TextArea | `components/settings/workspace/WorkspaceSettingsPanel.tsx` | ✅ |
| 单元测试 | pre_spawn tests ×5 + workspace_setting tests ×3 | ✅ |
| 后端零告警 | `cargo clippy --all-targets -- -D warnings` | ✅ |
| 前端零错误 | `npx tsc --noEmit` | ✅ |

---

## 3. 关键实现点

### 3.1 V70 迁移幂等性

`v70.rs` 通过 `add_column_if_missing` 判断列是否存在后再执行 `ALTER TABLE`，保证重复执行不报错。存量数据保持 NULL，读取时视为无 prompt 跳过拼接。

### 3.2 upsert 增量语义

`upsert_workspace_settings` 的 `system_prompt` 参数遵循增量语义：

- `Some(p)`（含空串）→ 覆写为 `p`
- `None` → 不动该列，保留既有 prompt

这样飞书 listener 等非 handler 调用点传 `None` 即可，不会意外清空用户配的 prompt。

### 3.3 注入层次设计

`prepare_execution_state` 在第 4.5 步调用 `inject_workspace_prompt`，位于第 5 步 `inject_expert_context` **之前**。设计意图：workspace 共识是最外层（用户配的全局约定），专家上下文次之（todo 级 Agent 定义），原任务在最内层。

### 3.4 降级策略

`inject_workspace_prompt` 在以下任一情况静默回退到原 message，不阻断 todo 执行：

- `workspace_id` 为 None（独立环节执行场景）
- `get_workspace_settings` DB 查询失败
- workspace_settings 行不存在
- system_prompt 为 None 或空串

### 3.5 安全处理

- 注入函数不在日志中打印 prompt 内容（可能含认证信息）
- 仅在 DB 查询失败时 `tracing::warn`，warn 消息不含 prompt 内容
- 前端 UI 加 ⚠️ Alert 警示语，提醒用户谨慎填写敏感信息

### 3.6 ActiveModel 创建分支修复

原 `upsert_workspace_settings` 创建分支用 `..Default::default()` 简写，新增 `system_prompt` 字段后暴露出 `id` 主键未显式设置的问题。修复：显式加 `id: ActiveValue::NotSet`，让 DB 自增主键。

---

## 4. 验证结果

### 4.1 后端

- `cargo clippy --all-targets -- -D warnings` → 零告警
- `cargo test --lib workspace_setting` → 3/3 通过
  - `test_upsert_with_system_prompt`
  - `test_upsert_none_system_prompt_keeps_old`
  - `test_upsert_empty_string_clears_prompt`
- `cargo test --lib inject_workspace_prompt` → 5/5 通过
  - `test_inject_workspace_prompt_none_workspace_id`
  - `test_inject_workspace_prompt_no_settings`
  - `test_inject_workspace_prompt_empty_prompt`
  - `test_inject_workspace_prompt_null_prompt`
  - `test_inject_workspace_prompt_normal_inject`

### 4.2 前端

- `npx tsc --noEmit` → 零错误
- `npm run build` → 构建成功

### 4.3 验收标准对照

| 验收标准（需求 §9） | 验证方式 | 结果 |
|--------------------|----------|------|
| 迁移成功，PRAGMA table_info 含 system_prompt | V70 迁移 + 集成测试 | ✅ |
| PUT 后 GET 能拿到 system_prompt | workspace_setting 单元测试 | ✅ |
| PUT 空串后 GET 返回空串 | `test_upsert_empty_string_clears_prompt` | ✅ |
| 执行注入：prompt + 分隔符 + message | `test_inject_workspace_prompt_normal_inject` | ✅ |
| system_prompt 为空/null 时 message 保持原样 | 3 个注入测试覆盖 | ✅ |
| 既有字段不受影响 | `test_upsert_none_system_prompt_keeps_old` | ✅ |
| 前端 UI 有 TextArea 区块 | WorkspaceSettingsPanel 改动 | ✅ |
| 零告警零错误 | clippy + tsc | ✅ |

---

## 5. 已知限制 / 待改进点

### 5.1 prompt 明文落库

按需求澄清第 5 项 A 选择，prompt 中允许明文写认证信息，由用户自己负责。前端加 ⚠️ 警示语缓解。后续若安全审计要求，可新增 `system_prompt_encrypted` 列做 AES 加密存储。

### 5.2 Loop 执行路径未注入

本期仅覆盖 todo 执行路径（`prepare_execution_state`）。Loop 执行路径（`loop_runner.rs` 等）有自己的 blackboard 上下文机制，未注入 workspace prompt。若未来 Loop 场景也需要共识 prompt，可在 Loop 执行入口同样调用 `inject_workspace_prompt`。

### 5.3 长度上限软限制

前端 `maxLength={8000}` 是软限制，后端不做硬校验。CLI 参数限制远大于 8K，实际不会触发问题。

### 5.4 无模板变量替换

prompt 是纯自由文本，不支持 `{{workspace_path}}` 等模板变量替换。YAGNI——用户实际需要在 prompt 里写明具体路径，不需要变量替换。

---

## 6. 修改文件清单

### 后端

| 文件 | 改动 |
|------|------|
| `backend/src/db/migration/v70.rs` | 新建：V70 迁移 |
| `backend/src/db/migration/mod.rs` | 注册 V70 |
| `backend/src/db/entity/workspace_settings.rs` | Model 加 system_prompt 字段 |
| `backend/src/db/workspace_setting.rs` | upsert 加参数 + 修复 ActiveModel 创建分支 |
| `backend/src/handlers/agent_bot.rs` | Request 加字段 + get 返回字段 + update 透传 |
| `backend/src/services/feishu_listener.rs` | 两处 upsert 调用补 None 参数 |
| `backend/src/executor_service/pre_spawn.rs` | 新增 inject_workspace_prompt 函数 + 5 个单元测试 |
| `backend/src/executor_service/stages.rs` | prepare_execution_state 接入注入 + import |

### 前端

| 文件 | 改动 |
|------|------|
| `frontend/src/utils/database/bots.ts` | WorkspaceSettings / UpdateWorkspaceSettingsParams 加 system_prompt 字段 |
| `frontend/src/components/settings/workspace/WorkspaceSettingsPanel.tsx` | UI 加 TextArea + Alert 警示语 |

### 文档

| 文件 | 改动 |
|------|------|
| `docs/requirements/022-工作空间Prompt-需求.md` | 新建：需求文档 |
| `docs/design/022-工作空间Prompt-设计.md` | 新建：设计文档 |
| `docs/requirements/022-工作空间Prompt-实现总结.md` | 新建：本实现总结 |

---

## 7. 与 PR 的对应关系

本实现总结对应 PR：`feat/workspace-prompt`（worktree: `nothing-todo-workspace-prompt`）。

PR 包含：
- 完整的后端 + 前端代码改动
- 需求文档、设计文档、实现总结文档
- 8 个新增单元测试，全部通过
- clippy 零告警 + tsc 零错误
