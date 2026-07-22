# Hook 系统设计文档

> ⚠️ **此为初始设计，实际实现已大幅简化**
>
> 最后核对日期：2026-06-08
>
> 本文档最初设想了一套 4 张表、3 层配置（Global / Rule / Per-Todo）、6 种触发点 + 模板变量 + 异步执行队列的复杂系统。
> 落地后做了**大幅简化**：hooks 不再是独立的 4 张表，而是 `todos` 表的一个 JSON 列；触发点从 6 种（before/after × 3）缩减为 4 种状态变化触发；文件结构从 11 个缩减为 3 个；无 inherit/custom/disabled 模式、无模板变量系统。
>
> 下方为与代码对齐的当前实现描述。

---

## 一、概述

Hook 系统让 Todo 状态变化时能够自动触发预设的命令（HTTP 通知、清理脚本等）。
每个 Todo 独立挂载自己的 hooks 配置，存为 `todos.hooks` 的 JSON 文本列。

| 项 | 实现 |
|----|------|
| 存储位置 | `todos` 表的 `hooks` 列（TEXT，JSON 数组） |
| 文件结构 | 3 个：`backend/src/hooks/mod.rs` / `models.rs` / `service.rs` |
| 配置模式 | Per-Todo 独立配置（**无** Global 共享、**无** inherit/custom/disabled 三种模式） |
| 触发点数量 | 4 种（全部为状态变化触发） |
| 模板变量 | **无**（当前只传递原始 `todo_id` / `old_status` / `new_status`） |
| 异步执行 | 同步执行（不阻塞主流程则 fire-and-forget） |

---

## 二、触发点（4 种）

> 实际由 `backend/src/hooks/models.rs` 的 `TodoHookTrigger` 枚举定义。

| 触发点 | 含义 |
|--------|------|
| `state_changed_to_pending` | 状态变为 `pending`（待执行）时 |
| `state_changed_to_in_progress` | 状态变为 `in_progress`（执行中）时 |
| `state_changed_to_completed` | 状态变为 `completed`（已完成）时 |
| `state_changed_to_failed` | 状态变为 `failed`（执行失败）时 |

> 旧设计中的 `before_create` / `after_create` / `before_status_change` / `after_status_change` / `before_delete` / `before_execute` / `after_delete` 全部**未实现**。

---

## 三、数据模型

### 3.1 Hook 项（JSON 结构，存储在 `todos.hooks` 列）

```json
[
  {
    "trigger": "state_changed_to_completed",
    "command": "curl",
    "args": ["-X", "POST", "https://example.com/notify"],
    "env": {
      "TODO_ID": "123",
      "NEW_STATUS": "completed"
    },
    "timeout_secs": 30,
    "enabled": true
  }
]
```

### 3.2 存储

hooks 是 `todos` 表的一个 JSON 列，**没有独立的 4 张表**（`hooks` / `global_hook_config` / `todo_hooks` / `hook_logs` 全部未创建）。

修改入口：`db/todo.rs:178-198 update_todo_hooks`：

```rust
pub async fn update_todo_hooks(
    id: i64,
    items: &[crate::hooks::TodoHookItem],
) -> Result<(), sea_orm::DbErr> {
    let wrapped = crate::hooks::TodoHooks { items };
    let json = serde_json::to_string(&wrapped).map_err(|e|
        sea_orm::DbErr::Custom(format!("failed to encode hooks for todo #{}: {}", id, e))
    )?;
    todoes::ActiveModel { id: ..., hooks: ActiveValue::Set(Some(json)), ... }
}
```

---

## 四、执行流程

```
Todo 状态变更（任何途径：用户点击 / 调度器 / 执行器回调）
       │
       ▼
读取 todos.hooks JSON 列
       │
       ▼
按 trigger 字段过滤（精确匹配 state_changed_to_*）
       │
       ▼
遍历匹配的 hook items
       │
       ├─ enabled == false → 跳过
       │
       └─ spawn 子进程执行 command + args + env
              │
              └─ 超时（timeout_secs）→ kill
```

- 失败 / 超时：当前仅记录日志（通过 `tracing::warn!`），**不**影响主状态变更。
- `old_status` / `new_status` 通过 env 注入到子进程。

---

## 五、文件结构

```
backend/src/hooks/
├── mod.rs         # 模块入口 + 公共 API
├── models.rs      # TodoHookTrigger 枚举、TodoHookItem、TodoHooks 结构
└── service.rs     # 触发匹配、子进程派发
```

> 旧设计中的 `executor.rs` / `template.rs` / `filter.rs` / `db/hooks.rs` / `db/global_config.rs` / `db/todo_hooks.rs` / `db/hook_logs.rs` 全部**未创建**。

---

## 六、与初始设计的差异

| 维度 | 初始设计 | 实际实现 |
|------|----------|----------|
| 存储 | 4 张独立表 | `todos.hooks` JSON 列 |
| 文件 | 11 个（含 db 子目录） | 3 个 |
| 触发点 | 7 种（before/after × 3 + before_execute） | 4 种（state_changed_to_*） |
| 配置层级 | Global + Rule + Per-Todo | 仅 Per-Todo |
| 模式 | inherit / custom / disabled | 无 |
| 模板变量 | `{{todo_id}}` 等 10+ 变量 | 无（env 注入原始值） |
| 异步队列 | mpsc 队列 + worker | 直接 spawn |
| 取消操作 | before_* 可拒绝 | 不可拒绝（仅 fire-and-forget） |
| 日志表 | `hook_logs` 表 | 走 `tracing` 日志 |
| 访问控制 | 名单 / token | 无 |

---

## 七、扩展方向（待办）

若未来需要把 hooks 做强，可考虑：

1. **增加触发点**：`state_changed_to_cancelled` / `execution_started` / `execution_finished`
2. **引入模板变量**：`{{todo.title}}` / `{{executor}}` 等
3. **拆出 `hook_logs` 表**：审计与回放
4. **共享 hooks**：增加 `shared_hooks` 表允许复用同一组规则到多个 Todo
5. **HTTP webhook 类型**：用现成的 `webhooks` 表作为远程通知通道
