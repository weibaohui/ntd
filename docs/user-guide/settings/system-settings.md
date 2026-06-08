# 系统设置

> **位置**：设置 → 系统设置
> **前端**：`frontend/src/components/settings/SystemSettingsPanel.tsx`
> **后端**：`backend/src/handlers/config.rs`

ntd 的核心运行时配置都在这里。改完点「**保存**」→ `PUT /api/config` → 大部分配置需要重启服务才生效（界面会标红「需重启」）。

---

## 1.基础设置

|字段 | 默认 |含义 |需重启 |
|------|------|------|--------|
| `host` | `0.0.0.0` |监听地址 | ✅ |
| `port` | `8088`（dev:18088） |监听端口 | ✅ |
| `db_path` | `sqlite://...` | 数据库路径 | ✅ |
| `log_level` | `info` |日志级别（trace/debug/info/warn/error） | ❌ |
| `scheduler_default_timezone` | `null` |业务时区（Cron解析用，可选 IANA 名，如 `Asia/Shanghai`） | ❌ |

> 「**基础设置**」之外的其他字段已迁移到对应面板，下面表格列出**实际位置**而不是「系统设置」入口。

---

## 2. 执行设置

>实际入口：**设置 → 运行管理 →顶部「运行配置」卡片**（参考 `frontend/src/components/settings/RuntimePanel.tsx`）

|字段 | 默认 |含义 |需重启 |
|------|------|------|--------|
| `max_concurrent_todos` | `3` | 同时跑的最大 Todo数 | ❌ |
| `execution_timeout_secs` | `3600` |单个 Todo最长执行时间 | ❌ |

>超出 `execution_timeout_secs` 的执行会被强制失败（走 `execution::force_fail_execution_handler`）。

---

## 3. SLASH 命令规则

> 把 `/xxx`形式的斜杠命令**绑到某个 Todo**上，触发时执行该 Todo。

>实际入口：**设置 →消息 →绑定 Tab → 「斜杠命令规则」卡片**（参考 `frontend/src/components/settings/messages/BindTab.tsx`）

### 3.1 数据格式

```yaml
slash_command_rules:
 - slash_command: "/review"
 todo_id:5
 enabled:true
 - slash_command: "/deploy"
 todo_id:12
 enabled:true
```

字段说明：
- `slash_command`：斜杠命令（如 `/review`），必须以 `/`开头
- `todo_id`：命中后执行的 Todo ID
- `enabled`：是否启用

>历史上曾用 `command`字段命名，现在统一改名为 `slash_command`；旧的 `description`字段已废弃，**不存储**。

### 3.2触发方式

-飞书 Bot收到 `/review xxx` →触发 todo5，prompt = `xxx`
-后续在 Todo详情 →执行记录能看到「来自 /review」

### 3.3校验规则

-命令必须以 `/`开头
- 命令不能为空
- 自动去重（同样的 `/xxx`只保留一条）
-关联的 todo_id必须存在（否则保存时报错）

---

## 4.飞书相关

>实际入口：**设置 →消息 →绑定 Tab → 「默认响应」卡片** 和 **「历史消息处理」卡片**（参考 `frontend/src/components/settings/messages/BindTab.tsx`）

|字段 | 默认 |含义 |
|------|------|------|
| `default_response_todo_id` | `null` |飞书 Bot收到非命令消息时执行的 Todo |
| `history_message_max_age_secs` | `600` |拉历史消息时跳过超过该秒数的旧消息 |

详细配置看 [messages-feishu.md](messages-feishu.md)。

---

## 5. 云端同步

>实际入口：**设置 → 云端同步** Tab

|字段 |含义 |
|------|------|
| `cloud_sync.server_url` |详细配置看 [cloud-sync.md](cloud-sync.md) |
| `cloud_sync.sync_token` | 同上（不返回明文） |
| `cloud_sync.default_conflict_mode` | 默认冲突策略（`overwrite` / `skip` / `rename`） |
| `cloud_sync.last_sync_at` | 最后成功同步时间（只读） |

> 注意：这些字段虽然在配置 YAML 里，但**实际配置入口在「云端同步」Tab**，因为有特殊校验和 UI交互。

---

## 6.备份与日志

>实际入口：**设置 →备份与恢复** Tab；本节只列字段，不重复 UI 说明。

|字段 | 默认 |含义 |
|------|------|------|
| `auto_backup_enabled` | `false` | 自动备份数据库 |
| `auto_backup_cron` | `003 * * *` | 数据库备份周期（每天3 点） |
| `auto_backup_max_files` | `30` | 数据库保留文件数 |
| `auto_todo_backup_enabled` | `false` | 自动备份 Todo |
| `auto_todo_backup_cron` | `004 * * *` | Todo备份周期（每天4 点） |
| `auto_todo_backup_max_files` | `30` | Todo保留文件数 |
| `auto_skill_backup_enabled` | `false` | 自动备份 Skill |
| `auto_skill_backup_cron` | `005 * * *` | Skill备份周期（每天5 点） |
| `auto_skill_backup_max_files` | `30` | Skill保留文件数 |
| `auto_cleanup_logs_days` | `null` 或 `30` | `execution_logs` 表保留天数；`null` = 不清理 |

>字段是扁平的（在 `Config` 结构体顶层），不再使用 `backup.*`嵌套。日志清理作用于数据库 `execution_logs` 表（不是磁盘日志文件），详见 [operations/log-cleanup.md](../operations/log-cleanup.md)。

详细看 [backup-and-restore.md](backup-and-restore.md)。

---

## 7.故障排查

### 7.1改了端口没生效

- 系统设置改 `port`后需要**重启服务**
-命令：`ntd daemon restart`

### 7.2时区不对

-改 `scheduler_default_timezone`（IANA名，如 `Asia/Shanghai`）
-影响 Cron定时任务的触发时间
-改完不需要重启，但新任务才生效

### 7.3并发数太小

-调大 `max_concurrent_todos`（去运行管理面板的「运行配置」）
-注意：执行器本身也有限制（如 Claude Code串行），调大了也不一定跑得快

---

## 8. 相关 API

| Method | Path |
|--------|------|
| GET | `/api/config` |
| PUT | `/api/config` |
