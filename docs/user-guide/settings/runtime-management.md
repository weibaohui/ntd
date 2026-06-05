# 运行管理

> **位置**：设置 → 运行管理
> **前端**：`frontend/src/components/settings/RuntimePanel.tsx`
> **后端**：`backend/src/handlers/execution.rs`（stop / force-fail 子命令）

「运行管理」是一个**实时面板**，展示当前正在跑的 Todo + 给你一个**批量熔断**按钮。

---

## 1. 实时运行列表

### 1.1 字段

| 字段 | 含义 |
|------|------|
| Todo 标题 | 关联 Todo 名 |
| 执行器 | 跑这个 Todo 的执行器（claudecode/codex/...） |
| 状态 | running / pending |
| 已运行时长 | 启动到现在的时间 |
| Token 消耗 | input + output（实时） |
| 进度 | 解析日志得到的进度条（如果有） |

### 1.2 数据来源

- WebSocket `/api/events` 推送「任务开始 / 输出 / 完成」事件
- 页面 mount 时同时拉一次 `GET /api/running-todos` 兜底
- 新事件触发局部刷新（不需要轮询）

### 1.3 自动刷新

WebSocket 断了的话，30s 自动重连。重连期间靠 setInterval 拉一次 `/api/running-todos` 兜底。

---

## 2. 批量操作

### 2.1 批量停止

- 勾选多个 → 「**停止选中**」按钮
- 后端调 `POST /api/execute/stop`，把 task_id 列表传过去
- 子进程收到 SIGTERM，**3 秒后未退出转 SIGKILL**

### 2.2 强制失败

- 「**强制失败**」：不走优雅停止，直接 kill + 标记 failed
- 适用：执行器卡死、跑出预期外结果想终止
- 调 `POST /api/execute/force-fail`

### 2.3 全部停止

- 顶部「**全部停止**」一键停掉所有 running
- 高危操作，会弹二次确认

---

## 3. 并发与超时

这里同时承担**运行时配置展示**：

| 配置 | 来源 | 说明 |
|------|------|------|
| `max_concurrent_todos` | 系统设置 | 同时跑的最大数（默认 3） |
| `execution_timeout_secs` | 系统设置 | 单个 Todo 最长执行（默认 3600s） |

面板只读展示，修改要去「系统设置」tab。

---

## 4. 日志查看

- 列表点 Todo → 跳到 Todo 详情 → 日志 tab
- 或点执行器图标 → 看当前 task 的实时输出

---

## 5. 故障排查

### 5.1 停止按钮无反应

- 子进程可能僵死（zombie 进程）
- 用「强制失败」按钮

### 5.2 列表一直显示已停止的 Todo

- WebSocket 断开了，刷新页面

### 5.3 并发数到上限，新 Todo 排不上

- 调高 `max_concurrent_todos`（去系统设置）
- 或停掉一些不重要的

---

## 6. 相关 API

| Method | Path | 用途 |
|--------|------|------|
| GET | `/api/running-todos` | 当前运行列表 |
| POST | `/api/execute/stop` | 停止一个或多个 |
| POST | `/api/execute/force-fail` | 强制失败 |
| WS | `/api/events` | 实时事件流 |
