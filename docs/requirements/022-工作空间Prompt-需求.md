# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AtomCode | 2026-07-24 | 初始版本 — 工作空间级 system_prompt 共识机制 |

---

# 1. 背景（Why）

当前 ntd 在执行 workspace 下的 todo 时，每个 todo 都是「裸 prompt」——执行器拿到 todo.title / todo.prompt 就开始干，没有 workspace 维度的共识上下文。

用户实际期望执行器在执行任意 todo 时都能遵守一些 workspace 级的约定：
- **产物放哪**：编译输出 / 报告 / 截图的固定目录
- **认证信息是什么**：访问内部服务用的 token / API key
- **基本文件在哪里**：项目根路径、关键配置文件位置
- **其他共识**：代码风格、提交约定、禁止行为等

如果不把这些共识集中沉淀，要么用户在每个 todo 的 prompt 里重复写（易遗漏、难维护），要么执行器在不同 todo 间行为不一致。本需求通过给 `workspace_settings` 增加一个自由文本 `system_prompt` 列，让 workspace 拥有一份共享前置 prompt，执行器在拼装 CLI 命令前把这段 prompt 注入到 message 前面，达成所有 todo 共享、遵守、共识同一份 workspace 上下文。

---

# 2. 目标（What，必须可验证）

- [ ] workspace_settings 表新增 `system_prompt TEXT` 列（迁移 V70）
- [ ] `GET /api/v1/workspaces/{id}/settings` 返回的 JSON 包含 `system_prompt` 字段（可能为 null）
- [ ] `PUT /api/v1/workspaces/{id}/settings` 支持传入 `system_prompt` 字段进行更新
- [ ] 执行器适配层在拼装 CLI 命令时，若 workspace 存在非空 `system_prompt`，则将其拼接到 message 开头，使用分隔符 `\n---\n` 与原 message 分隔
- [ ] 注入逻辑发生在 `select_executor_and_build_command`，在调用 `build_executor_command_args` 之前对 message 进行前缀拼接
- [ ] workspace_settings 现有的 default_response_* 字段行为保持不变（不破坏既有 upsert 路径）
- [ ] 前端在「工作空间设置」页面（`WorkspaceSettingsPanel`）增加「工作空间 Prompt」文本域区块
- [ ] 后端 cargo clippy 零告警；前端 tsc --noEmit 零错误
- [ ] 新增/修改的公开函数有对应单元测试

---

# 3. 非目标（Explicitly Out of Scope）

- ❌ 不做 prompt 内容结构化拆分（产物目录、认证、文件路径等不分独立列），统一作为一段自由文本
- ❌ 不对 prompt 内容做加密存储——用户自己负责，需求澄清第 5 项已明确
- ❌ 不做多版本 / 历史 prompt 管理，每个 workspace 仅一条当前 prompt
- ❌ 不在 prompt 中提供模板变量替换（如 `{{workspace_path}}`），保持纯文本
- ❌ 不修改 Loop 执行路径的 prompt 注入（Loop 已有 `build_blackboard_text` 等自己的上下文机制），本期仅覆盖 todo 执行路径
- ❌ 不新建 `workspace_prompts` 表——按需求澄清第 1 项 A 选择，复用 workspace_settings 新增列

---

# 4. 使用场景 / 用户路径

### 场景 1：用户首次配置 workspace prompt

1. 用户在「设置 → 工作空间 → 消息配置」页面找到「工作空间 Prompt」区块
2. 在文本域中写入约定内容，例如：

   ```
   ## 工作空间共识

   - 产物目录：所有编译输出放在 ./target/release，报告放在 ./reports
   - 认证：访问内部 GitLab 用 token `glpat-xxxxx`，放在 Authorization 头
   - 项目根：/Users/weibh/projects/rust/nothing-todo
   - 提交规范：使用 Conventional Commits，禁止 --no-verify
   ```

3. 点击「保存」，前端 PUT `/api/v1/workspaces/{id}/settings`，body 携带 `system_prompt` 字段
4. 后端 upsert_workspace_settings 把 system_prompt 写入 DB

### 场景 2：执行 todo 时注入 workspace prompt

1. 用户在工作空间下触发某个 todo 执行
2. 执行器适配层进入 `select_executor_and_build_command`
3. 系统读取 workspace_settings.system_prompt
4. 若非空，将 prompt 拼到原 message 前，形成新 message：

   ```
   <system_prompt 内容>
   ---
   <原 message 内容>
   ```

5. 把新 message 传给 `build_executor_command_args`，最终拼成 CLI 命令参数

### 场景 3：用户清空 workspace prompt

1. 用户在 UI 把文本域清空
2. 前端 PUT 携带 `system_prompt: ""`（空串）
3. 后端写入空串
4. 后续 todo 执行时读取到空串，跳过拼接，message 保持原样

---

# 5. 功能需求清单（Checklist）

- [ ] **DB 层**：新增 migration V70，给 `workspace_settings` 表加 `system_prompt TEXT DEFAULT NULL` 列；幂等性通过 `table_has_column` 保证
- [ ] **Entity 层**：`workspace_settings::Model` 增加 `pub system_prompt: Option<String>` 字段
- [ ] **DB 访问层**：`upsert_workspace_settings` 函数签名增加 `system_prompt: Option<String>` 参数；upsert 逻辑同步更新该列
- [ ] **Handler 层**：
  - `get_workspace_settings` 返回 JSON 增加 `system_prompt` 字段
  - `UpdateWorkspaceSettingsRequest` 增加 `system_prompt: Option<String>` 字段
  - `update_workspace_settings` 把 `system_prompt` 透传给 upsert
- [ ] **执行器注入**：
  - `RunTodoExecutionRequest` 或 `select_executor_and_build_command` 中读取 workspace system_prompt
  - 在 `build_executor_command_args` 调用前对 message 做前缀拼接
  - 拼接规则：`format!("{}\n---\n{}", system_prompt, message)`；system_prompt 为空或 None 时跳过拼接
- [ ] **前端类型**：`WorkspaceSettings` 接口增加 `system_prompt: string | null` 字段；`UpdateWorkspaceSettingsParams` 增加 `system_prompt?: string` 字段
- [ ] **前端 UI**：`WorkspaceSettingsPanel` 组件增加一个 Form.Item，内嵌 `Input.TextArea`，label 为「工作空间 Prompt」，placeholder 给出示例提示
- [ ] **单元测试**：
  - upsert_workspace_settings 写入 system_prompt 后再读取能拿到相同值
  - 注入辅助函数：空 prompt 跳过；非空 prompt 正确拼接分隔符
- [ ] **集成测试**：PUT settings 带 system_prompt → GET settings 能拿到 system_prompt

---

# 6. 约束条件（非常关键）

### 技术约束
- 后端 Rust + SeaORM，新增列必须通过迁移框架注册（不能直接 ALTER）
- 前端 React + Ant Design + TypeScript，UI 组件遵循既有 WorkspaceSettingsPanel 风格
- `system_prompt` 长度无硬性上限，但前端 TextArea 设置 `maxLength={8000}` 作为软限制（约 8K 字符，足够容纳常规共识）

### 架构约束
- 注入点必须在 `select_executor_and_build_command`（统一入口），不得在每个 executor 的 `command_args_with_session` 里重复实现
- workspace_settings 的 upsert 函数签名变更需要同步更新所有调用点（agent_bot handler / feishu_listener / execution handler 等）

### 安全约束
- prompt 内容允许明文写入认证信息（按需求澄清第 5 项 A 选择，用户自己负责）
- 但在日志 / 执行记录中，注入的 system_prompt 不单独打印（避免认证信息泄露到 execution_logs）——执行器输出本身可能携带 prompt，这部分由执行器自身的日志策略处理，本期不额外脱敏
- 后端 API 路径仍走 `v1_workspace_routes`，权限校验沿用现有 workspace 路由

### 性能约束
- workspace_settings 在 todo 执行时被读取一次（已经在 `select_executor_and_build_command` 路径中），不引入额外查询
- 若 select_executor_and_build_command 当前没有读 workspace_settings，则新增一次 DB 查询——可接受，单条 SELECT by workspace_id 走索引，延迟 < 1ms

---

# 7. 可修改 / 不可修改项

### ❌ 不可修改
- workspace_settings 表已有的列（id / workspace_id / default_response_* / updated_at）
- `command_args_with_session` / `command_args` 函数签名（保持 message: &str 入参不变）
- 现有 `update_workspace_settings` handler 的 URL 路径与 HTTP 方法
- 现有迁移文件 V1 ~ V69

### ✅ 可调整
- `upsert_workspace_settings` 函数签名（新增参数）—— 需同步所有调用点
- `UpdateWorkspaceSettingsRequest` 结构体（新增字段）
- `WorkspaceSettings` / `UpdateWorkspaceSettingsParams` 前端接口（新增字段）
- 新建 V70 迁移文件
- `select_executor_and_build_command` 内部逻辑（注入 prompt 拼接）

---

# 8. 接口与数据约定

### 8.1 数据库表 workspace_settings

| 列名 | 类型 | 说明 |
|------|------|------|
| id | INTEGER PK | 既有 |
| workspace_id | INTEGER | 既有 |
| default_response_type | TEXT | 既有 |
| default_response_todo_id | INTEGER | 既有 |
| default_response_loop_id | INTEGER | 既有 |
| default_response_executor | TEXT | 既有 |
| **system_prompt** | **TEXT** | **新增，可为 NULL，存储 workspace 级共识 prompt** |
| updated_at | TEXT | 既有 |

### 8.2 API: GET /api/v1/workspaces/{id}/settings

Response 200:
```json
{
  "code": 0,
  "data": {
    "workspace_id": 1,
    "default_response_type": "todo",
    "default_response_todo_id": null,
    "default_response_loop_id": null,
    "default_response_executor": null,
    "system_prompt": "## 工作空间共识\n\n- 产物目录：...",
    "updated_at": "2026-07-24T10:00:00Z"
  }
}
```

### 8.3 API: PUT /api/v1/workspaces/{id}/settings

Request body（增量更新，未传字段不动）:
```json
{
  "system_prompt": "## 新的共识内容\n..."
}
```

Response 200:
```json
{ "code": 0, "data": { "success": true } }
```

### 8.4 message 拼接规则

```
final_message = if system_prompt.is_empty() {
    message.to_string()
} else {
    format!("{}\n---\n{}", system_prompt, message)
}
```

- 分隔符 `\n---\n` 是 Markdown 水平分割线，让执行器把 prompt 和 todo 内容理解为两个独立段落
- system_prompt 末尾若已有换行，不再额外加换行；分隔符固定为 `\n---\n`

---

# 9. 验收标准（Acceptance Criteria）

1. **迁移成功**：升级到 V70 后，`PRAGMA table_info(workspace_settings)` 包含 `system_prompt` 列；已有数据未被破坏
2. **API 写入读取**：
   - 如果 PUT `/api/v1/workspaces/1/settings` body 为 `{"system_prompt": "hello"}`，则 GET 同一资源返回 `system_prompt: "hello"`
   - 如果 PUT body 为 `{"system_prompt": ""}`，则 GET 返回 `system_prompt: ""`（空串，非 null）
3. **执行注入**：
   - 如果 workspace 1 的 system_prompt = "共识 A"，todo 1 的 prompt = "做某事"，则传给执行器的 message 为 `"共识 A\n---\n做某事"`
   - 如果 system_prompt 为空串或 null，则 message 保持原样 `"做某事"`
4. **既有字段不受影响**：更新 system_prompt 后，default_response_type / todo_id / loop_id / executor 字段保持原值不变
5. **前端 UI**：在「工作空间设置」页面能看到「工作空间 Prompt」文本域，能编辑、保存、回显
6. **零告警**：`cd backend && cargo clippy --all-targets -- -D warnings` 零告警；`cd frontend && npx tsc --noEmit` 零错误

---

# 10. 风险与已知不确定点

### 风险 1：system_prompt 中可能包含敏感认证信息

- 缓解：UI 在文本域下方加提示「⚠️ 此处写入的内容将作为执行器前置 prompt 注入到该工作空间下所有 todo 的执行中，请谨慎填写敏感信息」
- 不引入加密：保持最简方案，用户自负责

### 风险 2：prompt 过长导致 CLI 参数超限

- 缓解：前端 maxLength=8000；后端不额外校验长度（YAGNI，CLI 参数限制远大于 8K）

### 风险 3：upsert_workspace_settings 签名变更影响多调用点

- 缓解：通过 trace_callers 提前列出所有调用点，逐个同步更新；编译器会强制所有调用点更新

### 不确定点

- Loop 执行路径是否也需要注入 workspace prompt？本期明确不做，待 Loop 场景有真实需求再开新需求

---

# 11. 非目标（重申）

- 不做 prompt 模板变量替换
- 不做 prompt 版本管理 / 历史回滚
- 不做 prompt 内容结构化拆分
- 不对 prompt 做加密存储
- 不修改 Loop 执行路径
- 不引入新的 workspace_prompts 表
