# Todo 生命周期

ntd 的核心实体就是 Todo。本文档覆盖 Todo 的**创建、状态机、执行、查看**全流程。

## 1. 状态机

> 状态枚举实现见 `backend/src/models/mod.rs`（`TodoStatus`），共 6 个值。

```
                    ┌──────────┐
       create       │          │  start
   ─────────────────► Pending  ├──────────► InProgress ──► Running
                    │          │                          │
                    └──────────┘                          │
                                                           │
                            ┌──────────────┬──────────────┼──────────────┐
                            ▼              ▼              ▼              ▼
                         Completed       Failed      Cancelled
                                              ▲
                                              │
                                       force-fail
```

| 状态 | 含义 |
|------|------|
| `pending` | 已创建但未跑 |
| `in_progress` | 执行器已接活、尚未进入 `Running`（如解析阶段） |
| `running` | 正在跑 |
| `completed` | 跑成功 |
| `failed` | 跑失败（执行器报错 / 超时 / 强制失败） |
| `cancelled` | 用户主动停止（运行管理 → 停止 / force-cancel） |

## 2. 创建

### 2.1 入口

- 主界面右上「+ 新建」
- Todo 列表底部「+ 新建」
- 看板：拖到对应列创建
- **Smart Create**：用自然语言描述，AI 帮你建

### 2.2 字段

| 字段 | 必填 | 含义 |
|------|------|------|
| `title` | ✅ | 标题 |
| `prompt` | ✅ | 完整指令 |
| `executor` | ✅ | 默认 claudecode |
| `tags` |  | 多个 |
| `workspace` |  | 工作目录（项目目录白名单） |
| `worktree_enabled` |  | 是否开 git worktree |
| `scheduler_enabled` |  | 是否启用定时 |
| `scheduler_config` |  | Cron 表达式（如 `0 0 9 * * 1`） |
| `scheduler_timezone` |  | 时区（如 `Asia/Shanghai`），缺省继承 `config.scheduler_default_timezone`（`frontend/src/types/todo.ts:5-23`、`backend/src/handlers/todo.rs:96-104`） |
| `hooks` |  | 前置/后置 hook（[设计文档](../../../hook-system-design.md)） |
| `template_id` |  | 从哪个模板创建的（追溯用） |

### 2.3 Smart Create

自然语言输入框 → AI 解析 → 字段自动填好 → 你确认 → 创建。

例：「每周一早上提醒我 review 一下 GitHub PR」 → 自动建一个带 scheduler（`0 0 9 * * 1`）的 Todo。

## 3. 执行

### 3.1 触发

- Todo 详情 → 右上「**执行**」
- Todo 列表 → 行的快速执行按钮
- Webhook 外网触发
- 飞书 Bot 收到 / 命令触发
- 定时任务自动触发

### 3.2 流程

1. 校验 Todo 存在 + 未在跑
2. 校验执行器可用
3. 调执行器 CLI 进程（`tokio::process::Command`）
4. 解析执行器 JSON 输出（`ChatMessage`: user/assistant/thinking/tool）
5. 实时通过 WebSocket 推送给前端
6. 完成后写 execution_records

### 3.3 进度追踪

- 后端监听执行器的 todo 工具调用（`TodoWrite`、`write_todo` 等），从中提取 `TodoItem` 数组（`backend/src/todo_progress.rs`）
- 通过 `TodoProgress` 事件推送
- 前端在 Todo 详情显示进度条 / 步骤列表

## 4. 查看详情

### 4.1 头部

- 标题、状态、标签
- 执行器、最后运行时间
- 进度条（如有）

### 4.2 历史链

- 所有执行记录按时间倒序
- 每条记录可展开看：开始时间、运行时长、Token、退出码
- 点「**查看日志**」看完整流

### 4.3 Chat 视图

- 解析后的对话式渲染
- 支持 Markdown、代码高亮
- 可以折叠/展开 thinking / tool 块

### 4.4 Log 视图

- 原始 stdout 流
- 适合调试执行器

### 4.5 Token 统计

- input / output / cache_read / cache_write 分别统计
- 估算成本（按 model 单价）

## 5. 编辑

- 抽屉里改
- 改完保存，不影响历史执行记录
- prompt 改了之后再跑，**用新 prompt**

## 6. 关系图

- Todo 之间可以建关联（[`hook-system-design.md`](../../../hook-system-design.md)）
- 关系图（`relation-map`）展示整个图谱
- 适用：把一个大任务拆成几个 Todo + 关联

## 7. 看板（Kanban）

入口：Todo 列表 → 顶部「**看板**」按钮 → 切换到「看板视图」标签

- 按状态分列：Pending / Running / Completed / Failed
- **拖拽**改状态
- 时间筛选（6h / 12h / 24h / 3d / 7d）
- 项目维度过滤、标题/prompt 搜索框、移动端 Tab 滑动切换
- 详细见 [kanban-board.md](kanban-board.md)

## 8. 纪念板

> 「纪念板」与「看板」实际是同一个合并页面（默认进入结论视图）。详细见 [memorial-board.md](memorial-board.md)。

## 9. 删除

- 单删：抽屉底部「删除」→ 软删除（`deleted_at` 标记）
- 真删：数据库 `DELETE FROM todos WHERE id = ?`
- 软删除的 Todo **不显示**在列表，但 execution_records 还在

## 10. 故障排查

### 10.1 Todo 卡在 running 不动

- 执行器僵死
- 解决：运行管理 → 强制失败
- 看后端日志 `execution::execute_handler` 关键字

### 10.2 Todo 跑失败但日志说成功

- 解析逻辑漏了某种退出码
- 看执行器原始输出（log 视图）

### 10.3 改 prompt 不生效

- Todo 详情里的 prompt 改了，**正在跑的实例仍用老 prompt**
- 等当前跑完，**新跑**才会用新 prompt
