# 运行管理

> **位置**：设置 →运行管理
> **前端**：`frontend/src/components/settings/RuntimePanel.tsx`
> **后端**：`backend/src/handlers/execution.rs`（force-fail子命令）

「运行管理」是一个**实时面板**，展示当前正在跑的 Todo +给你一个**批量熔断**按钮。

---

## 1.实时运行列表

### 1.1字段

|字段 |含义 |
|------|------|
| Todo标题 |关联 Todo名 |
| 执行器 |跑这个 Todo的执行器（claudecode/codex/...） |
|状态 |running（只有这一个状态，没有 `pending`） |
| 已运行时长 |启动到现在的时间 |
| Token消耗 | input + output（实时） |
|进度 |解析日志得到的进度条（如果有） |

### 1.2数据来源

- WebSocket `/api/events`推送「任务开始 /输出 /完成」事件
-页面 mount 时同时拉一次 `GET /api/execution-records/running`兜底
- 新事件触发局部刷新（不需要轮询）

### 1.3自动刷新

WebSocket断了的话，**10s** 自动重连（`RuntimePanel.tsx` 用 `setInterval` 每10000ms 重拉）。

>文档之前写的「30s 自动重连」是错的，真实值是10 秒。

---

## 2.批量操作

### 2.1批量停止

-勾选多个 → 「**停止选中**」按钮
- 前端对每条选中的记录**逐个**调 `POST /api/execute/force-fail`，body `{record_id: <id>}`
- 子进程收到 SIGTERM，**3 秒后未退出转 SIGKILL**

>没有 `POST /api/execute/stop` 这种批量接口（该路径不存在，参见 `mod.rs::create_app`）。停止一个执行记录走 `force-fail`，副作用是同时把记录标记为 `failed`。

### 2.2强制失败

- 「**强制失败**」：不走优雅停止，直接 kill +标记 failed
-适用：执行器卡死、跑出预期外结果想终止
-调 `POST /api/execute/force-fail`

### 2.3全部停止

-顶部「**全部停止**」一键停掉所有 running
- 高危操作，会弹二次确认

---

## 3.并发与超时

这里同时承担**运行时配置展示**：

|配置 |来源 |说明 |
|------|------|------|
| `max_concurrent_todos` |运行管理面板 |同时跑的最大数（默认3） |
| `execution_timeout_secs` |运行管理面板 |单个 Todo最长执行（默认3600s） |

面板只读展示，修改要去「运行管理」tab顶部的「运行配置」卡片，**不是**「系统设置」。

---

## 4.日志查看

-列表点 Todo →跳到 Todo详情 →日志 tab
- 或点执行器图标 →看当前 task 的实时输出

---

## 5.故障排查

### 5.1停止按钮无反应

- 子进程可能僵死（zombie进程）
- 用「强制失败」按钮

### 5.2列表一直显示已停止的 Todo

- WebSocket断开了，刷新页面

### 5.3并发数到上限，新 Todo排不上

-调高 `max_concurrent_todos`（去运行管理的「运行配置」）
- 或停掉一些不重要的

---

## 6.相关 API

| Method | Path |用途 |
|--------|------|------|
| GET | `/api/execution-records/running` |当前运行列表 |
| POST | `/api/execute/force-fail` |强制失败（body `{record_id}`） |
| WS | `/api/events` |实时事件流 |

> `/api/running-todos`仍存在路由（兼容旧前端），实际接口同 `/api/execution-records/running`。
