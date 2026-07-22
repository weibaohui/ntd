# Loop 异常处理 Todo 功能需求

## 背景

Loop 在运行过程中可能因各种异常情况而终止，例如：
- 超出最大执行步数限制（`capped_step`）
- 超出最大 Token 限制（`capped_token`）
- 执行失败（`failed`）
- 其他未预期的异常

当这些异常发生时，用户希望有机会对这些异常情况做统一的处理，比如：
- 清理 Loop 过程中产生的临时文件
- 保存中间态产物
- 发送通知
- 记录异常日志供后续分析

## 功能设计

### 1. 数据模型扩展

#### 后端 `loops` 表新增字段

```sql
ALTER TABLE loops ADD COLUMN abnormal_handler_todo_id BIGINT REFERENCES todos(id) ON DELETE SET NULL;
```

#### `limits_config` JSON 可选新增字段

为了灵活性，异常处理条件也可以内嵌到 `limits_config` 中：

```json
{
  "max_step_executions": 100,
  "max_total_tokens": 1000000,
  "abnormal_handler_todo_id": 123,
  "abnormal_handler_trigger_on": ["capped_step", "capped_token", "failed"]
}
```

**方案选择**：采用 `abnormal_handler_todo_id` 直接加在 `loops` 表的独立字段方式，而非 `limits_config` 内嵌，因为：
- 异常处理是一个独立概念，与限制配置是组合关系而非包含关系
- 独立字段更方便索引和查询
- 与 `review_template_id` 并列，概念对称

### 2. 前端表单设计

#### 位置

在 `LoopFormModal.tsx` 中，"评审模板"下方，"全局限制"区域上方（或下方），新增"异常处理 Todo"选择器。

#### UI 布局

```
┌─────────────────────────────────────────────┐
│ 评审模板                                     │
│ [下拉选择评审模板________________________▼]   │
│ + 新建模板                                   │
│                                             │
│ ← 新增区域 →                                 │
│ 异常处理 Todo                                │
│ [下拉选择 Todo _________________________▼]   │
│ 触发条件：☐ 超步数 ☐ 超Token ☐ 执行失败     │
│                                             │
│ 全局限制                                     │
│ ┌────────────────┬────────────────┐        │
│ │ 最大执行步数    │ 最大 Token 数  │        │
│ │ [100________]  │ [1000000___]  │        │
│ └────────────────┴────────────────┘        │
└─────────────────────────────────────────────┘
```

#### 触发条件选项

- `capped_step`：超出最大执行步数
- `capped_token`：超出最大 Token 数
- `failed`：执行失败

用户可以多选，表示任一条件触发时都执行异常处理 Todo。默认全选。

### 3. 后端执行逻辑

#### 修改文件

- `backend/src/db/entity/loops.rs`：新增字段
- `backend/src/models/loop_.rs`：`LoopDto`、`CreateLoopRequest`、`UpdateLoopRequest` 新增字段
- `backend/src/db/loop_.rs`：CRUD 操作支持新字段
- `backend/src/services/loop_runner.rs`：在 Loop 异常结束时检查并触发异常处理 Todo

#### 触发时机

在 `finish_loop_execution` 被调用时（即 Loop 即将结束时），判断状态是否为异常状态：
- `capped_step`
- `capped_token`
- `failed`
- `partial`（部分成功，可能也需要处理）

如果配置了 `abnormal_handler_todo_id`，则：
1. 创建异常处理 Todo 的执行实例
2. 使用 `loop_abnormal_handler` 作为 `trigger_type`
3. 向 Todo 执行器传递上下文信息（Loop 执行 ID、异常状态、消耗的步数/Token 等）

#### 上下文变量

异常处理 Todo 可以使用以下变量：
- `{{loop_execution_id}}`：本次 Loop 执行 ID
- `{{loop_id}}`：Loop ID
- `{{loop_name}}`：Loop 名称
- `{{abnormal_status}}`：异常状态（capped_step/capped_token/failed/partial）
- `{{total_executed_steps}}`：已执行步数
- `{{total_tokens_used}}`：已消耗 Token 数
- `{{failed_step_name}}`：失败的环节名称（如果有）

### 4. API 设计

#### 创建/更新 Loop

`POST /api/loops` 和 `PATCH /api/loops/:id` 请求体新增：

```json
{
  "name": "我的 Loop",
  "abnormal_handler_todo_id": 123,
  "abnormal_handler_trigger_on": ["capped_step", "capped_token", "failed"],
  ...
}
```

#### 获取 Loop

`GET /api/loops/:id` 返回值新增：

```json
{
  "id": 1,
  "name": "我的 Loop",
  "abnormal_handler_todo_id": 123,
  "abnormal_handler_trigger_on": ["capped_step", "capped_token", "failed"],
  ...
}
```

### 5. 边界情况处理

1. **异常处理 Todo 不存在或被删除**：忽略配置，不执行任何操作
2. **异常处理 Todo 执行失败**：记录错误日志，不影响 Loop 本身的结束流程
3. **异常处理 Todo 本身就是待执行的 Todo**：正常执行，使用系统默认执行器
4. **循环触发**：异常处理 Todo 执行时不应再次触发同一个 Loop 的异常处理（通过 trigger_type 区分）
5. **并发执行**：同一个 Loop 的多次执行可以并发，各自触发自己的异常处理

## 实现计划

### Phase 1: 数据模型 & API
1. 数据库 migrations 添加字段
2. 后端 entity/model 更新
3. CRUD API 支持新字段
4. 前端类型定义更新

### Phase 2: 前端表单
1. `LoopFormModal` 新增表单字段
2. Todo 选择器组件（可复用现有的 Selector）
3. 触发条件 Checkbox 组

### Phase 3: 后端执行逻辑
1. `loop_runner.rs` 中在异常结束时触发异常处理 Todo
2. 上下文变量注入
3. 错误处理和日志

### Phase 4: 测试
1. 单元测试
2. 手动验证
3. Playwright 自动化测试
