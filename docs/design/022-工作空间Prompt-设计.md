# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AtomCode | 2026-07-24 | 初始版本 — 工作空间级 system_prompt 注入设计 |

---

# 1. 设计目标

为每个 workspace 增加一份共享前置 prompt（`system_prompt`），在该 workspace 下任意 todo 执行时，执行器适配层把这段 prompt 拼接到 message 最前面，使所有 todo 共享、遵守、共识同一份 workspace 上下文（产物目录、认证信息、基本文件路径等）。

详见 `docs/requirements/022-工作空间Prompt-需求.md`。

---

# 2. 总体架构

## 2.1 注入层次

```
最终 message（传给 CLI）
  ├─ workspace system_prompt   ← 本期新增，最外层共识
  ├─ 专家上下文（Agent MD + 技能）  ← 既有 inject_expert_context
  └─ 原 todo message（占位符替换后）   ← 既有 substitute_message_placeholders
```

workspace prompt 是最外层共识，专家上下文次之，原任务在最内层。

## 2.2 模块改动一览

| 层 | 文件 | 改动 |
|----|------|------|
| 迁移 | `db/migration/v70.rs` | 新建：workspace_settings 加 `system_prompt TEXT` 列 |
| 迁移注册 | `db/migration/mod.rs` | 追加 `mod v70;` + `all_migrations()` 末尾 push |
| Entity | `db/entity/workspace_settings.rs` | Model 加 `pub system_prompt: Option<String>` |
| DB 访问层 | `db/workspace_setting.rs` | `upsert_workspace_settings` 加 `system_prompt: Option<String>` 参数；逻辑同步更新该列 |
| Handler | `handlers/agent_bot.rs` | `get_workspace_settings` JSON 加 `system_prompt`；`UpdateWorkspaceSettingsRequest` 加字段；`update_workspace_settings` 透传 |
| 注入辅助 | `executor_service/pre_spawn.rs` | 新增 `inject_workspace_prompt` 函数 |
| 编排 | `executor_service/stages.rs` | `prepare_execution_state` 在 `inject_expert_context` 之前调用 `inject_workspace_prompt` |
| 前端类型 | `utils/database/bots.ts` | `WorkspaceSettings` 加 `system_prompt`；`UpdateWorkspaceSettingsParams` 加 `system_prompt?` |
| 前端 UI | `components/settings/workspace/WorkspaceSettingsPanel.tsx` | 新增「工作空间 Prompt」TextArea Form.Item |

---

# 3. 数据库设计

## 3.1 V70 迁移

```rust
pub struct V70AddWorkspaceSettingsSystemPrompt;

#[async_trait]
impl Migration for V70AddWorkspaceSettingsSystemPrompt {
    fn version(&self) -> i64 { 70 }
    fn name(&self) -> &'static str { "V70AddWorkspaceSettingsSystemPrompt" }
    async fn up(&self, db: &Database) -> Result<(), sea_orm::DbErr> {
        add_column_if_missing(
            db,
            "workspace_settings",
            "system_prompt",
            "ALTER TABLE workspace_settings ADD COLUMN system_prompt TEXT",
        ).await?;
        tracing::info!("V70: workspace_settings.system_prompt 列已添加");
        Ok(())
    }
}
```

- `add_column_if_missing` 通过 `PRAGMA table_info` 判断列是否存在，保证幂等
- 新列默认 NULL，存量 workspace 不受影响（读取时视为空 prompt，跳过拼接）

## 3.2 Entity 改动

```rust
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub workspace_id: i64,
    pub default_response_type: String,
    pub default_response_todo_id: Option<i64>,
    pub default_response_loop_id: Option<i64>,
    pub default_response_executor: Option<String>,
    pub system_prompt: Option<String>,   // ← 新增
    pub updated_at: Option<String>,
}
```

---

# 4. DB 访问层改动

## 4.1 `upsert_workspace_settings` 签名扩展

```rust
pub async fn upsert_workspace_settings(
    db: &Database,
    workspace_id: i64,
    default_response_type: Option<String>,
    default_response_todo_id: Option<i64>,
    default_response_loop_id: Option<i64>,
    default_response_executor: Option<String>,
    system_prompt: Option<String>,   // ← 新增参数
) -> Result<(), sea_orm::DbErr>
```

更新分支：
- `Some(p)` → 写入 `p`（包括空串 `""`，用户清空 prompt 时显式写入空串）
- `None` → 不动该列（增量更新语义：未传字段不覆盖）

创建分支：
- 写入 `system_prompt`（None 时存 NULL，空串时存 `""`）

## 4.2 所有调用点同步更新

通过 trace_callers 已知调用点：

1. `handlers/agent_bot.rs::update_workspace_settings` —— 透传 req.system_prompt
2. `services/feishu_listener.rs` 多处：
   - `ensure_default_response_executor` —— 传 `None`（不动 system_prompt）
   - `handle_default_response_executor` 切 workspace 时的 upsert —— 传 `None`
   - 其他 upsert 调用 —— 传 `None`

所有非 handler 的 upsert 调用一律传 `None`，保持 system_prompt 列不被意外清空。

---

# 5. Handler 改动

## 5.1 `get_workspace_settings` 返回字段扩展

```rust
Some(s) => Ok(ApiResponse::ok(serde_json::json!({
    "workspace_id": s.workspace_id,
    "default_response_type": s.default_response_type,
    "default_response_todo_id": s.default_response_todo_id,
    "default_response_loop_id": s.default_response_loop_id,
    "default_response_executor": s.default_response_executor,
    "system_prompt": s.system_prompt,   // ← 新增
    "updated_at": s.updated_at,
}))),
None => Ok(ApiResponse::ok(serde_json::json!({
    "workspace_id": workspace_id,
    "default_response_type": "todo",
    "default_response_todo_id": null,
    "default_response_loop_id": null,
    "default_response_executor": null,
    "system_prompt": null,              // ← 新增
    "updated_at": null,
}))),
```

## 5.2 `UpdateWorkspaceSettingsRequest` 字段扩展

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateWorkspaceSettingsRequest {
    pub default_response_type: Option<String>,
    pub default_response_todo_id: Option<i64>,
    pub default_response_loop_id: Option<i64>,
    pub default_response_executor: Option<String>,
    pub system_prompt: Option<String>,   // ← 新增
}
```

## 5.3 `update_workspace_settings` 透传

```rust
crate::db::workspace_setting::upsert_workspace_settings(
    &state.db,
    workspace_id,
    req.default_response_type,
    req.default_response_todo_id,
    req.default_response_loop_id,
    req.default_response_executor,
    req.system_prompt,    // ← 新增
)
.await
.map_err(|e| AppError::Internal(e.to_string()))?;
```

---

# 6. 执行器适配层注入设计

## 6.1 新增 `inject_workspace_prompt` 函数

位置：`backend/src/executor_service/pre_spawn.rs`

```rust
/// 注入工作空间级共识 prompt。
///
/// 读取 workspace_settings.system_prompt，若非空则拼接到 message 最前面，
/// 用 Markdown 水平分割线 `\n---\n` 与原 message 分隔。
/// 这样 workspace 下的所有 todo 执行时都共享同一份前置上下文（产物目录、
/// 认证信息、基本文件路径等），达成 workspace 维度的共识。
///
/// workspace_id 为 None 或读取失败或 prompt 为空时，静默返回原 message——
/// workspace prompt 是增强项，不应阻断 todo 执行。
pub(crate) async fn inject_workspace_prompt(
    db: &Database,
    workspace_id: Option<i64>,
    message: &str,
) -> String {
    let Some(wid) = workspace_id else {
        return message.to_string();
    };
    let settings = match crate::db::workspace_setting::get_workspace_settings(db, wid).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("读取 workspace {} settings 失败，跳过 prompt 注入: {}", wid, e);
            return message.to_string();
        }
    };
    let Some(prompt) = settings.and_then(|s| s.system_prompt) else {
        return message.to_string();
    };
    if prompt.is_empty() {
        return message.to_string();
    }
    format!("{}\n---\n{}", prompt, message)
}
```

函数 ≤ 30 行，单一职责，失败静默回退。

## 6.2 编排层接入

`prepare_execution_state` 现状（第 5 步注入专家）：

```rust
let expert_message = inject_expert_context(&request, &todo, &substituted.message).await;
let selected = select_executor_and_build_command(&request, &todo, &expert_message).await?;
```

改造后（新增第 4.5 步：注入 workspace prompt）：

```rust
// 4.5) 注入工作空间级共识 prompt：workspace 下所有 todo 共享的前置上下文。
//      放在 inject_expert_context 之前：workspace 共识是最外层，专家上下文次之。
let workspace_message = inject_workspace_prompt(
    &request.db,
    request.workspace_id,
    &substituted.message,
).await;
// 5) 注入专家上下文：如果 todo 关联了 expert_name，把 Agent MD 和技能信息
//    拼到 message 前面。失败时静默回退到原 message，不阻断执行。
let expert_message = inject_expert_context(&request, &todo, &workspace_message).await;
// 6) 选定 executor 并构造 command_args（传入注入后的 message）。
let selected = select_executor_and_build_command(&request, &todo, &expert_message).await?;
```

最终 message 层次：`workspace_prompt → 专家上下文 → 原任务`。

## 6.3 为什么不在 `select_executor_and_build_command` 内部注入

- **职责单一**：`select_executor_and_build_command` 已有「选 executor + 注入 model + 构造 argv」三件事，再加 prompt 注入违反 ≤30 行约束
- **可测性**：独立函数 `inject_workspace_prompt` 容易单元测试
- **可复用**：未来 Loop 执行路径需要注入时，可直接调用

---

# 7. 前端设计

## 7.1 类型扩展

`frontend/src/utils/database/bots.ts`：

```typescript
export interface WorkspaceSettings {
  workspace_id: number;
  default_response_type: 'todo' | 'loop' | 'executor';
  default_response_todo_id: number | null;
  default_response_loop_id: number | null;
  default_response_executor: string | null;
  system_prompt: string | null;    // ← 新增
  updated_at: string | null;
}

export interface UpdateWorkspaceSettingsParams {
  default_response_type?: 'todo' | 'loop' | 'executor';
  default_response_todo_id?: number;
  default_response_loop_id?: number;
  default_response_executor?: string;
  system_prompt?: string;          // ← 新增
}
```

## 7.2 UI 改动

`frontend/src/components/settings/workspace/WorkspaceSettingsPanel.tsx`：

在现有 Form 中新增一个 Form.Item：

```tsx
<Form.Item
  label="工作空间 Prompt"
  name="system_prompt"
  tooltip="该工作空间下所有 todo 执行时作为前置 prompt 注入。可填写产物目录约定、认证信息、基本文件路径等共识内容。"
>
  <Input.TextArea
    rows={8}
    maxLength={8000}
    showCount
    placeholder={
      '## 工作空间共识\n\n' +
      '- 产物目录：编译输出放在 ./target/release\n' +
      '- 认证：访问内部服务用 token xxx\n' +
      '- 项目根：/path/to/project'
    }
  />
</Form.Item>
```

- label 下方提示：「⚠️ 此处写入的内容将作为执行器前置 prompt 注入到该工作空间下所有 todo 的执行中，请谨慎填写敏感信息」
- 加载时：`form.setFieldsValue({ system_prompt: s.system_prompt ?? '' })`
- 保存时：`db.updateWorkspaceSettings(workspaceId, { system_prompt: values.system_prompt ?? '' })` —— 空串显式清空

---

# 8. 单元测试设计

## 8.1 `inject_workspace_prompt` 测试

测试位置：`backend/src/executor_service/pre_spawn.rs` 的 `#[cfg(test)] mod tests`

| 测试名 | 场景 | 期望 |
|--------|------|------|
| `test_inject_workspace_prompt_none_workspace_id` | workspace_id = None | 返回原 message |
| `test_inject_workspace_prompt_db_error` | db 查询返回 Err | 静默返回原 message |
| `test_inject_workspace_prompt_no_settings` | get_workspace_settings 返回 None | 返回原 message |
| `test_inject_workspace_prompt_empty_prompt` | system_prompt = Some("") | 返回原 message |
| `test_inject_workspace_prompt_null_prompt` | system_prompt = None | 返回原 message |
| `test_inject_workspace_prompt_normal_inject` | system_prompt = "共识" | 返回 `"共识\n---\n原message"` |

## 8.2 `upsert_workspace_settings` 测试

| 测试名 | 场景 | 期望 |
|--------|------|------|
| `test_upsert_with_system_prompt` | 创建时传 system_prompt | 读取能拿到相同值 |
| `test_upsert_update_system_prompt` | 已存在记录，更新 system_prompt | DB 中值被更新 |
| `test_upsert_none_system_prompt_keeps_old` | 已存在 system_prompt，再次 upsert 传 None | 旧值保持不变 |

---

# 9. 错误处理与降级策略

| 故障 | 降级策略 |
|------|---------|
| `get_workspace_settings` DB 查询失败 | `tracing::warn`，返回原 message，todo 正常执行 |
| workspace_settings 行不存在 | 视作无 prompt，返回原 message |
| system_prompt 为空串或 NULL | 跳过拼接，返回原 message |
| system_prompt 内容超长 | 不在后端校验，前端 maxLength=8000 软限制；CLI 参数限制远大于此 |

---

# 10. 安全反思

## 10.1 prompt 内容明文落库

- 按需求澄清第 5 项 A 选择，允许用户在 prompt 中写入认证信息，明文存储
- 缓解：前端 UI 在文本域下方加 ⚠️ 警示语，提醒用户谨慎填写敏感信息
- 后续可演化：若安全审计要求，可增加 `system_prompt_encrypted` 列做 AES 加密存储，但本期 YAGNI

## 10.2 注入逻辑不引入越权

- `inject_workspace_prompt` 仅读取当前 request.workspace_id 的 settings，不跨 workspace
- handler `update_workspace_settings` 沿用既有 `v1_workspace_routes` 路由权限校验，不新增越权面

## 10.3 不在日志中单独打印 prompt

- 注入函数本身不 `tracing::info!` 打印 prompt 内容（避免认证信息泄露到日志）
- 仅在 DB 查询失败时 `tracing::warn`，warn 消息不含 prompt 内容

---

# 11. 性能影响评估

| 项 | 影响 |
|----|------|
| 新增一次 `get_workspace_settings` 查询 | 单条 SELECT by workspace_id，走索引，< 1ms，可接受 |
| message 字符串拼接 | O(n) 字符串拼接，prompt 长度通常 < 8K 字符，影响可忽略 |
| 前端 TextArea 渲染 | React 受控组件，maxLength=8000，无性能问题 |

---

# 12. 实现顺序

1. V70 迁移 + Entity 改动（独立可编译验证）
2. `upsert_workspace_settings` 签名扩展 + 所有调用点同步
3. Handler `get_workspace_settings` / `update_workspace_settings` 改动
4. `inject_workspace_prompt` 函数 + `prepare_execution_state` 接入
5. 单元测试
6. `cd backend && cargo clippy --all-targets -- -D warnings && cargo test`
7. 前端类型扩展 + UI 改动
8. `cd frontend && npx tsc --noEmit`
9. 功能总结文档 `docs/requirements/022-工作空间Prompt-实现总结.md`

---

# 13. 非目标（重申）

- 不做 prompt 模板变量替换 / 多版本管理 / 加密存储
- 不引入新表，复用 workspace_settings 新增列
- 不修改 `command_args_with_session` / `command_args` 函数签名

## 13.1 Loop 路径覆盖说明（任务 10 增补）

本期 PR 在任务 10 增补了 Loop 执行路径的 prompt 注入，与最初稿不同：

- **Loop 正常 step 路径**：`loop_runner.rs::run_inner_from` 第 4d-bis 步注入 `inject_workspace_prompt`，workspace 共识拼到 `enhanced_prompt` 最外层
- **Loop 异常 handler 路径**：`loop_runner.rs::trigger_abnormal_handler` 第 5 步同样注入 `inject_workspace_prompt`，异常处理 todo 也共享 workspace 共识（P2 修复）
- 两条路径均使用 `loop_.workspace_id.filter(|&id| id != 0)` 作 workspace_id 参数，与同文件第 127/817 行的过滤逻辑保持一致
- `workspace_id = None/0`、DB 查询失败、prompt 为空时 `inject_workspace_prompt` 静默回退原 prompt
