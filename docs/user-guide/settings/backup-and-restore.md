# 备份与恢复

> **位置**：设置 →备份与恢复
> **前端**：`frontend/src/components/settings/BackupPanel.tsx` + `backup/*.tsx`
> **后端**：`backend/src/handlers/backup.rs`

ntd 提供三类备份，**每类独立配置自动备份策略**：

|类型 |备份什么 |后端默认 cron |存储位置 |
|------|----------|--------------|----------|
| **数据库备份** |整库 `data.db`（zip压缩 level9） |每天3 点 `003 * * *` | `~/.ntd/backups/db/` |
| **Todo备份** | todos序列化为 YAML |每天4 点 `004 * * *` | `~/.ntd/backups/todo/` |
| **Skill备份** |8 个执行器下的 skills打 zip |每天5 点 `005 * * *` | `~/.ntd/backups/skills/` |

>上述 cron表达式**没有指定星期几**，所以是「每天X 点」而不是「每周X」。例如 `003 * * *`表示「每天3:00触发」。

另外有：**日志清理**（清理 `execution_logs` 表中超过 N天的行，**不是删磁盘日志文件**）。

---

## 1.数据库备份

### 1.1手动触发

入口：备份与恢复 →数据库备份 Tab

- **立即备份**：点「**立即备份**」→ 后端把 SQLite 数据库文件打成 zip（`deflate`压缩，level9）
- **下载**：列表里点文件名 →浏览器下载
- **删除**：单条删除
- **优化**：点「**优化数据库**」→触发 `PRAGMA optimize`（**仅更新 SQLite统计信息**，不会重建表、不收缩文件）

### 1.2自动备份

- 「**自动备份**」开关 + Cron表达式
- 「**最大保留文件数**」：超过自动删最旧的（默认30，不是10）

### 1.3状态查询

`GET /api/backup/database/status` 返回：
```json
{
 "auto_backup_enabled":true,
 "auto_backup_cron": "003 * * *",
 "auto_backup_max_files":30,
 "last_backup": "2026-06-04T20:00:00Z",
 "files": [{ "name": "data-20260604-200000.zip", "size":1234, "created_at": "..." }]
}
```

>备份文件扩展名是 `.zip`（不是 `.db.gz`），文件名格式 `{dbFilename}-{YYYYMMDD-HHMMSS}.zip`（例如 `data-20260604-200000.zip`）。

### 1.4恢复

**警告：恢复会覆盖当前数据库！**

- 把备份文件下载到本地
-停止 ntd 服务（`ntd daemon stop`）
- 把下载的 `.zip` 解压得到 `database.db`，覆盖 `~/.ntd/data.db`（或 `data.dev.db`）
- 重启 ntd（`ntd daemon start`）
- 不支持在 UI 里一键恢复（避免误操作）

---

## 2.Todo备份

### 2.1手动导出

入口：备份与恢复 → Todo备份 Tab

- **全部导出**：`GET /api/backup/export` → 下载 YAML
- **选中导出**：`POST /api/backup/export-selected` body `{"todo_ids": [1,2,3]}` → 下载 YAML
- YAML格式：包含 todo 的所有字段（title/prompt/status/executor/scheduler/...）

### 2.2导入

两种模式：

|模式 |行为 |何时用 |
|------|------|------|
| **导入** |完全替换本地 todos（先删后插） |从零开始的新环境 |
| **合并** |保留本地，新加云端的，更新相同的 |跨设备同步数据 |

入口：点「**导入**」→选 YAML →选模式

### 2.3自动备份

-周期：默认每天4 点 `004 * * *`
-文件名：`todo-backup-YYYYMMDD-HHMMSS.zip`
-文件结构：单个 zip，内含 `backup.yaml`

### 2.4 API

| Method | Path |用途 |
|--------|------|------|
| GET | `/api/backup/export` |全量导出 |
| POST | `/api/backup/export-selected` | 选择性导出（body `{"todo_ids":[...]}`） |
| POST | `/api/backup/import` |导入（替换） |
| POST | `/api/backup/merge` |合并 |
| GET | `/api/backup/todo/status` |状态查询 |
| POST | `/api/backup/todo/trigger` |立即触发 |
| PUT | `/api/backup/todo/auto` |改自动策略 |
| GET | `/api/backup/todo/file` |下载文件 |
| DELETE | `/api/backup/todo/file` |删文件 |

---

## 3.Skill备份

### 3.1手动触发

入口：备份与恢复 → Skills备份 Tab

-立即备份：把8 个执行器（Claude Code / Codex / Hermes / Kimi / ...）的 `~/.{executor}/skills/` 全打成一个 zip
-一个 zip包含所有执行器的 skills

### 3.2何时用

-重装系统前保留所有 skills
- 把本机的 skills分享给同事
-跨执行器迁移（先备份再「Skills同步」到新执行器）

### 3.3 API

| Method | Path |
|--------|------|
| GET | `/api/backup/skills/status` |
| POST | `/api/backup/skills/trigger` |
| PUT | `/api/backup/skills/auto` |
| GET | `/api/backup/skills/file` |
| DELETE | `/api/backup/skills/file` |

---

## 4.日志清理

>入口：备份与恢复 →日志清理 Tab

>清理目标是数据库 `execution_logs` 表（不是磁盘上的 `.log`文件）。详情参考 [operations/log-cleanup.md](../operations/log-cleanup.md)。

### 4.1配置

唯一字段（`backend/src/config.rs::auto_cleanup_logs_days`）：
- 类型 `Option<usize>`，`None` 表示不清理
- 默认 `Some(30)`（保留30 天）
-调成 `null`关闭自动清理

PUT body：
```json
{ "days":30 }
```

### 4.2手动触发

点「**立即清理**」→ 后端跑：

```sql
DELETE FROM execution_logs
WHERE timestamp < datetime('now', '-30 days')
```

返回删除的行数。

### 4.3 API

| Method | Path |
|--------|------|
| GET | `/api/backup/log-cleanup/status` |
| PUT | `/api/backup/log-cleanup` |
| POST | `/api/backup/log-cleanup/trigger` |

---

## 5.最佳实践

1. **生产环境**：开启数据库每日自动备份 +保留30 个文件
2. **跨设备同步**：用 Todo合并模式而不是替换模式
3. **升级前**：手动触发一次数据库备份，再 `ntd daemon upgrade`
4. **磁盘空间**：数据库备份文件 = 数据库大小 × ~0.3（压缩后），按30 个文件算
5. **异地备份**：用 `tunnel.sh` 把 `~/.ntd/backups/`同步到 NAS（`rsync` 或类似工具）

---

## 6.故障排查

### 6.1备份失败「Permission denied」

-目录权限：`chmod700 ~/.ntd/backups/`
-进程用户要跟目录 owner一致

### 6.2恢复后数据不全

- 检查 YAML文件是否完整（vim 看末尾）
-导入时如果选了「替换」，会把当前库**全清**再插，**空文件**会清空数据库
- 用「合并」更安全

### 6.3自动备份不触发

- 检查 Cron表达式（6 位：秒 分 时 日 月 周）
- 看后端日志 `backup_scheduler`关键字
- 服务重启后 Cron重新注册
