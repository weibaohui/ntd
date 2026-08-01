# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-08-01 | 初始版本：BUG-001 缺陷说明 |

# 1. 缺陷标题

新建任务接口中文标题 UTF-8 切片 panic

# 2. 缺陷描述

`POST /api/v1/workspaces/{ws}/tasks`，当 requirement 首行超过 60 **字节**且第 60 字节落在多字节 UTF-8 字符内部时，handler 按字节切片 `&title[..60]` 触发 Rust str 边界检查 panic，导致连接被掐断（客户端收到空响应 HTTP 000）。

# 3. 影响范围

- 所有含 CJK 字符且首行 >60 字节的需求文本都会触发 panic
- 严重度：P1（运行时崩溃，影响可用性）

# 4. 复现方式

```bash
curl -X POST http://localhost:18088/api/v1/workspaces/2/tasks \
  -H 'Content-Type: application/json' \
  -d '{"loop_id":12,"requirement":"【E2E-REQUIREMENT-MARKER】端到端验证需求：交付一个演示特性的 PRD 与笔记。"}'
# 预期：连接断开 HTTP 000，服务器日志含 panicked at src/handlers/tasks.rs:55
```

# 5. 修复方案

按字符截断（`title.chars().take(60).collect()`）替代字节切片，补 CJK 长标题回归测试。
