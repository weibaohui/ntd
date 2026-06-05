# 系统设置

> **位置**：设置 → 系统设置
> **前端**：`frontend/src/components/settings/SystemSettingsPanel.tsx`
> **后端**：`backend/src/handlers/config.rs`

ntd 的核心运行时配置都在这里。改完点「**保存**」→ `PUT /api/config` → 大部分配置需要重启服务才生效（界面会标红「需重启」）。

---

## 1. 基础设置

| 字段 | 默认 | 含义 | 需重启 |
|------|------|------|--------|
| `server_host` | `0.0.0.0` | 监听地址 | ✅ |
| `server_port` | `8088`（dev: 18088） | 监听端口 | ✅ |
| `database_url` | `sqlite://...` | 数据库路径 | ✅ |
| `log_level` | `info` | 日志级别（trace/debug/info/warn/error） | ❌ |
| `timezone` | `Asia/Shanghai` | 业务时区（Cron 解析用） | ❌ |

---

## 2. 执行设置

| 字段 | 默认 | 含义 | 需重启 |
|------|------|------|--------|
| `max_concurrent_todos` | `3` | 同时跑的最大 Todo 数 | ❌ |
| `execution_timeout_secs` | `3600` | 单个 Todo 最长执行时间 | ❌ |
| `default_response_todo_id` | null | 飞书 Bot 默认回复关联的 Todo | ❌ |
| `auto_sync_custom_templates_enabled` | false | 自动同步自定义模板 | ❌ |
| `auto_sync_custom_templates_cron` | `0 0 4 * * *` | 同步周期 | ❌ |

> 超出 `execution_timeout_secs` 的执行会被强制失败（走 `execution::force_fail_execution_handler`）。

---

## 3. SLASH 命令规则

> 把 `/xxx` 形式的斜杠命令**绑到某个 Todo** 上，触发时执行该 Todo。

### 3.1 数据格式

```yaml
slash_command_rules:
  - command: "/review"
    todo_id: 5
    description: "代码审查"
  - command: "/deploy"
    todo_id: 12
    description: "部署到测试环境"
```

### 3.2 触发方式

- 飞书 Bot 收到 `/review xxx` → 触发 todo 5，prompt = `xxx`
- 后续在 Todo 详情 → 执行记录能看到「来自 /review」

### 3.3 校验规则

- 命令必须以 `/` 开头
- 命令不能为空
- 自动去重（同样的 `/xxx` 只保留一条）
- 关联的 todo_id 必须存在（否则保存时报错）

### 3.4 入口

设置 → 系统设置 → 「**SLASH 命令规则**」折叠面板

- 列表显示已配置的规则
- 点「+ 新增」→ 填 command + 选 todo
- 单条编辑 / 删除

---

## 4. 飞书相关

| 字段 | 含义 |
|------|------|
| `default_response_todo_id` | 飞书 Bot 收到非命令消息时，是否关联到某个 Todo 跑 |
| `history_message_max_age_secs` | 历史消息保留时长（默认 600s），过期清理 |

详细配置看 [messages-feishu.md](messages-feishu.md)。

---

## 5. 云端同步

| 字段 | 含义 |
|------|------|
| `cloud_sync.server_url` | 详细配置看 [cloud-sync.md](cloud-sync.md) |
| `cloud_sync.sync_token` | 同上（不返回明文） |
| `cloud_sync.default_conflict_mode` | 默认冲突策略（`overwrite` / `skip` / `rename`） |
| `cloud_sync.last_sync_at` | 最后成功同步时间（只读） |

> 注意：这些字段虽然在「系统设置」的 YAML 里，但**实际配置入口在「云端同步」Tab**，因为有特殊校验和 UI 交互。

---

## 6. 备份与日志

| 字段 | 默认 | 含义 |
|------|------|------|
| `backup.auto_database_backup` | false | 自动备份数据库 |
| `backup.database_backup_cron` | `0 0 4 * * *` | 数据库备份周期 |
| `backup.database_backup_max_files` | 10 | 保留文件数 |
| `backup.auto_log_cleanup` | false | 自动日志清理 |
| `backup.log_retention_days` | 30 | 日志保留天数 |

详细看 [backup-and-restore.md](backup-and-restore.md)。

---

## 7. 故障排查

### 7.1 改了端口没生效

- 系统设置改 `server_port` 后需要**重启服务**
- 命令：`ntd daemon restart`

### 7.2 时区不对

- 改 `timezone`（IANA 名，如 `Asia/Shanghai`）
- 影响 Cron 定时任务的触发时间
- 改完不需要重启，但新任务才生效

### 7.3 并发数太小

- 调大 `max_concurrent_todos`
- 注意：执行器本身也有限制（如 Claude Code 串行），调大了也不一定跑得快

---

## 8. 相关 API

| Method | Path |
|--------|------|
| GET | `/api/config` |
| PUT | `/api/config` |
