# 备份与恢复

> **位置**：设置 → 备份与恢复
> **前端**：`frontend/src/components/settings/BackupPanel.tsx` + `backup/*.tsx`
> **后端**：`backend/src/handlers/backup.rs`

ntd 提供三类备份，**每类独立配置自动备份策略**：

| 类型 | 备份什么 | 后端默认周期 | 存储位置 |
|------|----------|--------------|----------|
| **数据库备份** | 整库 `data.db`（压缩 level 9） | 每天 04:00 | `~/.ntd/backups/db/` |
| **Todo 备份** | todos 序列化为 YAML | 每周日 03:00 | `~/.ntd/backups/todo/` |
| **Skill 备份** | 8 个执行器下的 skills 打 zip | 每周六 02:00 | `~/.ntd/backups/skills/` |

另外有：**日志清理**（按保留天数删 daemon.log）。

---

## 1. 数据库备份

### 1.1 手动触发

入口：备份与恢复 → 数据库备份 Tab

- **立即备份**：点「**立即备份**」→ 后端用 SQLite 备份 API 拷出压缩文件
- **下载**：列表里点文件名 → 浏览器下载
- **删除**：单条删除
- **优化**：点「**优化数据库**」→ 触发 `VACUUM` + 重建索引

### 1.2 自动备份

- 「**自动备份**」开关 + Cron 表达式
- 「**最大保留文件数**」：超过自动删最旧的

### 1.3 状态查询

`GET /api/backup/database/status` 返回：
```json
{
  "enabled": true,
  "cron": "0 0 4 * * *",
  "max_files": 10,
  "last_backup_at": "2026-06-04T20:00:00Z",
  "files": [{ "name": "data-2026-06-04.db.gz", "size": 1234, "created_at": "..." }]
}
```

### 1.4 恢复

**警告：恢复会覆盖当前数据库！**

- 把备份文件下载到本地
- 停止 ntd 服务（`ntd daemon stop`）
- 把下载的 `.db.gz` 解压后覆盖 `~/.ntd/data.db`（或 `data.dev.db`）
- 重启 ntd（`ntd daemon start`）
- 不支持在 UI 里一键恢复（避免误操作）

---

## 2. Todo 备份

### 2.1 手动导出

入口：备份与恢复 → Todo 备份 Tab

- **全部导出**：`GET /api/backup/export` → 下载 YAML
- **选中导出**：`POST /api/backup/export-selected` body `{"ids": [1,2,3]}` → 下载 YAML
- YAML 格式：包含 todo 的所有字段（title/prompt/status/executor/scheduler/...）

### 2.2 导入

两种模式：

| 模式 | 行为 | 何时用 |
|------|------|--------|
| **导入** | 完全替换本地 todos（先删后插） | 从零开始的新环境 |
| **合并** | 保留本地，新加云端的，更新相同的 | 跨设备同步数据 |

入口：点「**导入**」→ 选 YAML → 选模式

### 2.3 自动备份

- 周期：默认每周日 03:00
- 文件名：`todos-YYYY-MM-DD.yaml`
- 文件结构：单个 YAML 文件，可直接 `cat` 看内容

### 2.4 API

| Method | Path | 用途 |
|--------|------|------|
| GET | `/api/backup/export` | 全量导出 |
| POST | `/api/backup/export-selected` | 选择性导出 |
| POST | `/api/backup/import` | 导入（替换） |
| POST | `/api/backup/merge` | 合并 |
| GET | `/api/backup/todo/status` | 状态查询 |
| POST | `/api/backup/todo/trigger` | 立即触发 |
| PUT | `/api/backup/todo/auto` | 改自动策略 |
| GET | `/api/backup/todo/file` | 下载文件 |
| DELETE | `/api/backup/todo/file` | 删文件 |

---

## 3. Skill 备份

### 3.1 手动触发

入口：备份与恢复 → Skills 备份 Tab

- 立即备份：把 8 个执行器（Claude Code / Codex / Hermes / Kimi / ...）的 `~/.{executor}/skills/` 全打成一个 zip
- 一个 zip 包含所有执行器的 skills

### 3.2 何时用

- 重装系统前保留所有 skills
- 把本机的 skills 分享给同事
- 跨执行器迁移（先备份再「Skills 同步」到新执行器）

### 3.3 API

| Method | Path |
|--------|------|
| GET | `/api/backup/skills/status` |
| POST | `/api/backup/skills/trigger` |
| PUT | `/api/backup/skills/auto` |
| GET | `/api/backup/skills/file` |
| DELETE | `/api/backup/skills/file` |

---

## 4. 日志清理

> 入口：备份与恢复 → 日志清理 Tab

### 4.1 配置

- 「**保留天数**」：超过 N 天的日志文件会被删
- 「**自动清理**」：开启后按 Cron 跑

### 4.2 手动触发

点「**立即清理**」→ 删 `backend.dev.log`、`daemon.log` 等的过期部分。

### 4.3 API

| Method | Path |
|--------|------|
| GET | `/api/backup/log-cleanup/status` |
| PUT | `/api/backup/log-cleanup` |
| POST | `/api/backup/log-cleanup/trigger` |

---

## 5. 最佳实践

1. **生产环境**：开启数据库每日自动备份 + 保留 10 个文件
2. **跨设备同步**：用 Todo 合并模式而不是替换模式
3. **升级前**：手动触发一次数据库备份，再 `ntd daemon upgrade`
4. **磁盘空间**：数据库备份文件 = 数据库大小 × ~0.3（压缩后），按 10 个文件算
5. **异地备份**：用 `tunnel.sh` 把 `~/.ntd/backups/` 同步到 NAS（`rsync` 或类似工具）

---

## 6. 故障排查

### 6.1 备份失败「Permission denied」

- 目录权限：`chmod 700 ~/.ntd/backups/`
- 进程用户要跟目录 owner 一致

### 6.2 恢复后数据不全

- 检查 YAML 文件是否完整（vim 看末尾）
- 导入时如果选了「替换」，会把当前库**全清**再插，**空文件**会清空数据库
- 用「合并」更安全

### 6.3 自动备份不触发

- 检查 Cron 表达式（6 位：秒 分 时 日 月 周）
- 看后端日志 `backup_scheduler` 关键字
- 服务重启后 Cron 重新注册
