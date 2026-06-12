# Session 管理功能设计文档

## 概述

在设置页面新增「Session 管理」标签页，提供 Claude Code 会话的只读浏览功能。展示所有会话的元信息，帮助用户了解各个执行器的使用情况、会话历史等。

## 数据来源

Claude Code 的会话存储在 `~/.claude/` 目录下：

| 路径 | 说明 |
|------|------|
| `~/.claude/sessions/<pid>.json` | 活跃会话注册表，包含 PID、sessionId、cwd、启动时间、版本等 |
| `~/.claude/projects/<encoded-path>/<uuid>.jsonl` | 会话对话记录（JSONL 格式，每行一个消息） |
| `~/.claude/projects/<encoded-path>/<uuid>/subagents/` | 子代理目录 |
| `~/.claude/history.jsonl` | 全局命令历史 |

### 会话 JSONL 关键字段

每行 JSON 包含 `type` 字段，主要类型：
- `user` - 用户消息，含 `message.role`、`message.content`、`sessionId`、`cwd`、`gitBranch`、`version`、`entrypoint`
- `assistant` - 助手回复，含 `message.model`、`message.usage`（token 用量）、`message.stop_reason`
- `queue-operation` - 队列操作，含用户输入的 prompt
- `last-prompt` - 最后一条 prompt
- `attachment` - 附件信息

### 活跃会话 PID 文件字段

```json
{
  "pid": 74512,
  "sessionId": "uuid",
  "cwd": "/path/to/project",
  "startedAt": 1778538856977,
  "procStart": "Mon May 11 22:34:16 2026",
  "version": "2.1.138",
  "kind": "interactive",
  "entrypoint": "sdk-cli"
}
```

## 功能列表

### P0 - 核心功能（必须完成）

#### F1. Session 列表 API
- **后端** 新增 `/api/sessions` GET 接口
- 扫描 `~/.claude/sessions/` 获取活跃会话
- 扫描 `~/.claude/projects/` 下所有项目的 JSONL 文件
- 解析 JSONL 提取会话元信息：sessionId、项目路径、创建时间、最后活跃时间、消息数、token 用量、模型名称、git 分支、entrypoint
- 支持分页参数 `page`、`page_size`
- 支持过滤参数：`executor`（entrypoint 过滤）、`project`（项目路径过滤）、`status`（active/completed）、`search`（搜索 prompt 内容）

#### F2. Session 列表 UI
- 在设置页 Tabs 中新增「Session 管理」标签
- 使用 Ant Design Table 展示会话列表
- 列：状态指示灯（活跃/已完成）、Session ID（缩短显示）、项目路径、执行器类型、模型、Git 分支、消息数、Token 用量、创建时间、最后活跃时间
- 支持分页
- 支持按状态过滤（全部/活跃/已完成）
- 支持按执行器过滤
- 支持搜索

#### F3. Session 详情 API
- **后端** 新增 `/api/sessions/:id` GET 接口
- 读取指定 session 的 JSONL 文件
- 返回完整的消息列表（用户消息 + 助手回复，按时间排序）
- 包含每条消息的 role、content 摘要、model、token 用量、时间戳

#### F4. Session 详情 UI（只读查看器）
- 点击列表中的 session 行，弹出 Drawer 或 Modal 显示详情
- 按时间线展示对话记录
- 用户消息和助手回复用不同样式区分
- 显示每条消息的元信息（模型、token 数、时间）
- 只读模式，不允许编辑

### P1 - 增强功能

#### F5. Session 统计概览
- 在列表上方显示统计卡片
- 总会话数、活跃会话数、今日新会话、总 Token 消耗
- 按执行器分组的会话数量饼图或统计条

#### F6. Session 详情 - 子代理信息
- 在详情页展示该会话产生的子代理列表
- 显示子代理类型、描述、执行时间
- 可展开查看子代理的对话记录

#### F7. Session 批量清理
- 支持选择多个已完成的旧会话进行清理
- 清理前确认对话框
- 仅删除 JSONL 文件和对应目录，不影响数据库

## 技术方案

### 后端

新增文件：
- `backend/src/handlers/session.rs` - 会话管理 API 处理器
- 在 `handlers/mod.rs` 中注册模块和路由

路由：
```
GET  /api/sessions           - 列表（支持分页和过滤）
GET  /api/sessions/:id       - 详情
GET  /api/sessions/stats     - 统计数据
DELETE /api/sessions/:id     - 删除（仅 JSONL 文件）
```

数据结构：
```rust
struct SessionInfo {
    session_id: String,
    project_path: String,       // 从 cwd 或目录名解码
    status: String,             // "active" | "completed"
    executor: String,           // entrypoint 字段
    model: String,              // 从 assistant 消息提取
    git_branch: Option<String>,
    message_count: u32,
    total_tokens: u64,          // input + output tokens
    created_at: String,         // 第一条消息的时间
    last_active_at: String,     // 最后一条消息的时间
    file_size: u64,             // JSONL 文件大小
    version: Option<String>,    // Claude Code 版本
}

struct SessionDetail {
    info: SessionInfo,
    messages: Vec<SessionMessage>,
    subagents: Vec<SubAgentInfo>,
}

struct SessionMessage {
    role: String,               // "user" | "assistant"
    content: String,            // 内容摘要（截断）
    model: Option<String>,
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
    timestamp: String,
    stop_reason: Option<String>,
}

struct SubAgentInfo {
    agent_type: String,
    description: String,
    message_count: u32,
}
```

性能考量：
- 使用 `tokio::task::spawn_blocking()` 包装文件 I/O
- 大 JSONL 文件只读取头部和尾部来提取元信息，避免全量解析
- 列表接口只返回元信息，不包含对话内容
- 分页在服务端实现

### 前端

修改文件：
- `frontend/src/components/SettingsPage.tsx` - 新增 Session 管理标签页
- `frontend/src/utils/database.ts` - 新增 session API 调用

UI 组件：
- Ant Design Table 作为主列表
- Ant Design Drawer 作为详情查看器
- Ant Design Statistic + Card 作为统计概览
- Ant Design Tag 作为状态/执行器标签

## 进度追踪

- [x] F1. Session 列表 API
- [x] F2. Session 列表 UI
- [x] F3. Session 详情 API
- [x] F4. Session 详情 UI
- [x] F5. Session 统计概览
- [x] F6. Session 子代理信息
- [x] F7. Session 批量清理
- [x] F8. 多工具 Session 扫描（Claude Code / Codex / Hermes / Kimi / AtomCode / MobileCoder / Opencode / CodeWhale）

> 注：F8 描述原本包含 "CC-Connect"，实际未实现（`backend/src/handlers/session.rs` 注释里 `source` 字段枚举提到了 "cc-connect"，但 `scan_*` 函数族没有 `scan_cc_connect`，启动时也不会扫描 `~/.cc-connect/sessions`）。实际支持 8 个来源：claude-code、codex、hermes、kimi、atomcode、mobilecoder、opencode、codewhale。

---

**最后更新**: 2026-06-08
