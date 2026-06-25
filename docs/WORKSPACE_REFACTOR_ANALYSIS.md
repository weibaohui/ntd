# 工作空间重构分析报告（v2 — 一次性到位方案）

> 分析日期：2026-06-25
> 前置条件：无历史用户，无兼容包袱。一次性做彻底。

---

## 1. 当前架构问题（现状）

### 1.1 数据归属混乱

```
agent_bots (全局，无 workspace 字段)
  │
  └── feishu_project_bindings  (桥接：bot_id + chat_id → project_dir_id → todo_id)
         │
         ├── project_directories  (workspace，仅存 path/name)
         │
         └── todos  (有 workspace 字段)

Config.slash_command_rules   (全局，persisted in config.yaml)
Config.default_response_todo_id  (全局)
```

**问题**：
- bot 可以同时绑定多个 workspace 的 chat，没有归属概念
- 斜杠命令全局匹配，消息来了不知道该查哪个 workspace 的命令
- 设置页的"消息"Tab 列出所有 bot 的所有绑定，混合展示，混乱

### 1.2 设置页扁平化

14 个 Tab 平铺，消息和工作空间是并列的，没有层级关系。

---

## 2. 目标架构

### 2.1 数据模型

```
project_directories (workspace)
  │
  ├── agent_bots.workspace_id → project_directories.id  (1:N, 一 workspace 可有多个 bot)
  │     │
  │     └── feishu_project_bindings  (bot_id + chat_id → todo_id)
  │            * project_dir_id 移除，由 bot.workspace_id 推导
  │            * 变更 bot 的 workspace_id 时，其全部 binding 失效
  │
  ├── workspace_slash_commands  (workspace_id + slash_command → todo_id)
  │     * Config 中删除 slash_command_rules
  │
  └── workspace_default_todo  (workspace_id → todo_id)
        * Config 中删除 default_response_todo_id
```

### 2.2 bot 与 workspace 的关系：严格一对一

**正常模式：每个 workspace 独立拥有自己的 bot**

```
workspace-A  →  bot-1 (聊天绑定 A1, A2)
workspace-B  →  bot-2 (聊天绑定 B1)
```

每个 bot 创建时归属一个 workspace，在该 workspace 下建立聊天绑定、处理消息。不同 workspace 的 bot 完全独立，互不干扰。**不存在跨 workspace 共享 bot 的场景。**

**复用模式：bot 变更 workspace（非默认，需显式操作）**

当用户决定将一个 bot 从 workspace-A 变更到 workspace-B 时：
1. bot-1 在 workspace-A 的所有聊天绑定全部失效（disabled）
2. bot-1 的 workspace_id 更新为 workspace-B
3. bot-1 在 workspace-B 下重新建立聊天绑定（重新 /bind）

**约束规则**：
- bot 创建时必须选择 workspace（不可为 NULL）
- bot 禁止同时服务两个 workspace（`agent_bots.workspace_id` 是单值，不是数组）
- bot 变更 workspace 是显式的、破坏性的操作：用户确认后，旧绑定全部失效

### 2.3 设置页结构

```
设置
├── 系统（全局）
│   ├── 系统设置   → 端口、日志、超时等（去掉 slash_command_rules 和 default_response_todo_id）
│   ├── 执行器管理 → 全局，不变
│   ├── 标签管理   → 全局，不变
│   ├── 模板管理   → 全局，不变
│   ├── 评审模板   → 全局，不变
│   ├── 备份与恢复 → 全局，不变
│   ├── Skills 管理 → 全局，不变
│   ├── 运行管理   → 全局，不变
│   ├── Session    → 全局，不变
│   └── 云端同步   → 全局，不变
│
├── 工作空间
│   ├── 工作空间列表 (原 ProjectDirectoriesPanel)
│   │
│   └── [进入某个工作空间]
│       ├── 智能体     → 该 workspace 下的 bot 列表 + 创建/删除/设置
│       │   └── [选择 bot]
│       │       ├── 基本设置（app_id, app_secret, 推送等）
│       │       ├── 聊天绑定 → chat bindings 管理
│       │       └── 消息记录 → 该 bot 的历史消息
│       │
│       ├── 斜杠命令   → 该 workspace 的命令规则
│       └── 默认响应   → 该 workspace 默认回复的 Todo
│
└── 关于
```

---

## 3. 改动清单

### 3.1 数据库变更

```sql
-- 1. agent_bots 加 workspace_id（不再允许 NULL）
ALTER TABLE agent_bots ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 0;
-- 迁移：已有 bot 按 feishu_project_bindings 中 project_dir_id 最多的来设定

-- 2. 新建 workspace_slash_commands 表
CREATE TABLE workspace_slash_commands (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id INTEGER NOT NULL,
    slash_command TEXT NOT NULL,     -- 如 "/todo"
    todo_id INTEGER NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(workspace_id, slash_command)
);

-- 3. 新建 workspace_settings 表（存储 default_response_todo_id 等）
CREATE TABLE workspace_settings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id INTEGER NOT NULL UNIQUE,
    default_response_todo_id INTEGER,
    updated_at TEXT NOT NULL
);

-- 4. feishu_project_bindings 不变（project_dir_id 可保留但不强制校验，
--    因为 bot.workspace_id 已确定归属，project_dir_id 仅作为冗余验证）
```

### 3.2 后端变更 — Config

```rust
// config.rs — 删除这两个字段
pub struct Config {
    // ❌ 删除: pub slash_command_rules: Vec<SlashCommandRule>,
    // ❌ 删除: pub default_response_todo_id: Option<i64>,
    // ✓ 保留: 端口、日志、超时等其他全局配置
}
```

### 3.3 后端变更 — 消息路由

**feishu_listener.rs 核心改动**：

```rust
// 阶段 5: 项目绑定路由 → 不变，仍然按 bot_id + chat_id 查 binding
//         但 binding 的 project_dir_id 应与 bot.workspace_id 一致

// 阶段 6a: 斜杠命令匹配 → 改为按 workspace 查询
fn find_slash_rule(db: &Database, workspace_id: i64, command: &str)
    -> Option<WorkspaceSlashCommand>
{
    db.get_workspace_slash_command(workspace_id, command).await
}

// 阶段 6b: 默认回复 → 改为按 workspace 查询
fn get_default_todo(db: &Database, workspace_id: i64) -> Option<i64> {
    db.get_workspace_setting(workspace_id).await?.default_response_todo_id
}

// 消息路由中需要注入 workspace_id：
// bot_id → 查 agent_bots.workspace_id → 作为后续所有查询的上下文
```

### 3.4 后端变更 — API

| API | 变更 |
|-----|------|
| `POST /api/agent-bots` | 新增必填参数 `workspace_id` |
| `PUT /api/agent-bots/:id` | 新增可选参数 `workspace_id`；变更时级联禁用 binding |
| `GET /api/agent-bots` | 新增可选过滤参数 `workspace_id` |
| `PUT /api/config` | 不再接受 `slash_command_rules` 和 `default_response_todo_id` |
| 新增 `GET /api/workspace/:id/slash-commands` | 获取 workspace 斜杠命令 |
| 新增 `POST /api/workspace/:id/slash-commands` | 创建斜杠命令 |
| 新增 `PUT /api/workspace/:id/slash-commands/:cmd_id` | 更新斜杠命令 |
| 新增 `DELETE /api/workspace/:id/slash-commands/:cmd_id` | 删除斜杠命令 |
| 新增 `GET /api/workspace/:id/settings` | 获取 workspace 设置 |
| 新增 `PUT /api/workspace/:id/settings` | 更新 workspace 设置 |

### 3.5 后端变更 — Bot 变更 workspace 的级联逻辑

> 仅在用户显式变更 bot 的 workspace 时触发，不是运行时的默认行为。

```rust
// bot 的 workspace_id 变更时（仅用户显式操作触发）：
async fn move_bot_to_workspace(db: &Database, bot_id: i64, new_workspace_id: i64) {
    // 1. 查询该 bot 的所有 project_bindings
    let bindings = db.get_feishu_project_bindings(bot_id).await?;
    
    // 2. pending binding 直接删除（没绑定到实际聊天，无需保留）
    for b in &bindings {
        if b.chat_id == PENDING_CHAT_ID {
            db.delete_feishu_project_binding(b.id).await?;
        }
    }
    
    // 3. 已生效的 binding 设为 disabled（保留记录，可重新启用）
    for b in &bindings {
        if b.enabled && b.chat_id != PENDING_CHAT_ID {
            db.update_feishu_project_binding_enabled(b.id, false).await?;
            // 通知飞书聊天 "该绑定因 bot 变更 workspace 已失效"
        }
    }
    
    // 4. 更新 bot.workspace_id
    db.update_agent_bot(bot_id, UpdateAgentBot { workspace_id: Some(new_workspace_id) }).await?;
}
```

### 3.6 前端变更 — 文件清单

| 文件 | 变更类型 | 说明 |
|------|----------|------|
| `SettingsPage.tsx` | 重构 | 分组 Tab 结构，新增 workspace 层级 |
| `settings/MessagesPanel.tsx` | 重构 | 改为接收 workspace_id，移到 workspace 层级下 |
| `settings/messages/BindTab.tsx` | 修改 | 创建 bot 时必选 workspace |
| `settings/messages/ProjectBindsTab.tsx` | 修改 | binding 的 project_dir_id 由 bot.workspace_id 决定 |
| `settings/messages/RecordTab.tsx` | 修改 | 按 workspace 过滤消息 |
| `settings/SystemSettingsPanel.tsx` | 简化 | 删除斜杠命令和默认响应配置项 |
| `settings/WebhooksPanel.tsx` | 删除 | Webhook 功能已移除 |
| **新增** `settings/workspace/WorkspaceDetailPage.tsx` | 新增 | 工作空间详情页（含智能体、命令子 Tab） |
| **新增** `settings/workspace/WorkspaceAgentPanel.tsx` | 新增 | 工作空间下的智能体管理 |
| **新增** `settings/workspace/WorkspaceSlashCommandsPanel.tsx` | 新增 | 工作空间下的斜杠命令管理 |

### 3.7 前端变更 — 关键交互

**Bot 创建流程**（在 workspace 上下文内）：
```
工作空间详情 → 智能体 Tab → "新建智能体" 按钮
  → Modal: 填写 bot 信息（bot 类��、app_id、app_secret）
  → workspace_id 自动设为当前工作空间
  → 创建成功后刷新 bot 列表
```

**Bot 变更 workspace**：
```
bot 详情 → "变更到其他工作空间" → 选择目标 workspace
  → 确认弹窗："此操作将使该 bot 在原有工作空间的所有聊天绑定失效，确定？"
  → 确认后：级联禁用 binding + 更新 bot.workspace_id
```

---

## 4. 冲突和风险分析

### 4.1 确定性冲突

| 冲突项 | 解决方案 |
|--------|----------|
| `Config.slash_command_rules` 被多处读取 | 删除字段，改为 workspace_slash_commands 表 |
| `Config.default_response_todo_id` 被 feishu_listener 读取 | 删除字段，改为 workspace_settings 表 |
| 系统设置表单中有 slash_command_rules 字段 | 移除表单项，迁移到 workspace 设置页 |
| `feishu_history_fetcher.rs` 读取 slash_command_rules | 改为按 workspace 查询 |

### 4.2 风险

| 风险 | 等级 | 缓解 |
|------|------|------|
| bot 变更 workspace 时级联禁用 binding，用户需重新 /bind | 低 | 这是预期行为，有明确提示 |
| 已有数据库中 bot 没有 workspace_id | 中 | 迁移脚本：按 bot 的 binding 中 project_dir_id 最多的设为默认 |
| 迁移后 bot 的 workspace_id 可能不对 | 中 | 提醒用户手动检查修正 |
| 前端改造工作量大 | 中 | 分组件拆解，逐个替换 |

### 4.3 不用担心的点

- API 兼容性：无用户，不需要兼容
- 数据丢失：无生产数据
- 回滚：git revert 即可，数据迁移单向前进

---

## 5. 实施计划

| 阶段 | 内容 | 依赖 | 产出 | 状态 |
|------|------|------|------|------|
| 1 | 后端 DB 迁移 + 实体层 | 无 | migration.rs + entity 文件 | ✅ 已完成 |
| 2 | 后端 CRUD 层（新表） | 1 | db/workspace_slash_command.rs, db/workspace_setting.rs | ✅ 已完成 |
| 3 | 后端 Config 清理（删除 2 个字段） | 1 | config.rs 简化 | ✅ 已完成 |
| 4 | 后端 feishu_listener 改造（workspace 路由） | 2,3 | feishu_listener.rs | 🔄 部分完成 |
| 5 | 后端 API handlers（新接口 + bot 接口加 workspace_id） | 2 | handlers/ 目录 | ✅ 已完成 |
| 6 | 后端 agent_bot handler（级联逻辑） | 2,5 | handlers/agent_bot.rs | ✅ 已完成 |
| 7 | 前端类型定义更新 | 5 | types/ 目录 | ⏳ 待完成 |
| 8 | 前端 API 调用层更新 | 7 | utils/database/ 目录 | ⏳ 待完成 |
| 9 | 前端 SettingsPage 重构（分组 Tab） | 8 | SettingsPage.tsx | ⏳ 待完成 |
| 10 | 前端 workspace 详情页 + 智能体面板 | 8,9 | 新增文件 | ⏳ 待完成 |
| 11 | 前端斜杠命令面板 | 8,9 | 新增文件 | ⏳ 待完成 |
| 12 | 集成测试 + 数据迁移测试 | 全部 | tests/ | ⏳ 待完成 |

**图例**：✅ 已完成 | 🔄 部分完成 | ⏳ 待完成

建议顺序：1→2→3→4→5→6→7→8→9→10→11→12

### 阶段完成说明

**阶段1-3**：已完成
- V30 迁移：创建 `workspace_slash_commands` 和 `workspace_settings` 表
- `agent_bots` 表新增 `workspace_id` 字段
- Config 删除 `slash_command_rules` 和 `default_response_todo_id` 字段

**阶段4**：部分完成
- `feishu_listener.rs` 已改为从数据库查询斜杠命令和默认响应
- `feishu_history_fetcher.rs` 的 `resolve_todo_id` 暂时返回 `None`，待实现 workspace 查询

**阶段5-6**：已完成
- 新增 workspace 斜杠命令 CRUD API：
  - `GET /api/workspace/{id}/slash-commands` - 获取列表
  - `POST /api/workspace/{id}/slash-commands` - 创建
  - `PUT /api/workspace/{id}/slash-commands/{cmd_id}` - 更新
  - `DELETE /api/workspace/{id}/slash-commands/{cmd_id}` - 删除
- 新增 workspace 设置 API：
  - `GET /api/workspace/{id}/settings` - 获取设置
  - `PUT /api/workspace/{id}/settings` - 更新设置
- Bot 变更 workspace 级联 API：
  - `PUT /api/agent-bots/{id}/workspace` - 移动 bot 到新 workspace
  - pending binding 直接删除
  - 已生效 binding 设为 disabled
  - 更新 bot.workspace_id 并重启 listener

---

## 6. 总结

| 项目 | 结论 |
|------|------|
| 是否可行？ | **完全可行**，目标架构清晰 |
| 最大改动 | 后端 6 个，前端 8 个文件 |
| 核心原则 | bot 一对一绑定 workspace；变更 workspace 时级联解绑所有 chat binding |
| 数据迁移 | agent_bots 加 workspace_id 列，按历史 binding 推断归属 |
| 不回退 | 单向前进，git 管控版本 |
