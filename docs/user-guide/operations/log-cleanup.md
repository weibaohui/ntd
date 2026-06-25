# 日志清理

>本文档说明 ntd **「日志清理」**功能清理的是什么、不会清理什么，并提供手工清理指引。

ntd跑久了会在数据库里堆积大量执行日志。**自动清理的是 `execution_logs` 表中的行**（数据库表），**不是**磁盘上的 `.log`文件（`~/.ntd/run.log` 等由系统 logrotate / daemon自身管理）。

---

## 1.清理目标

|清理对象 |说明 |接口 |
|----------|------|------|
| 数据库表 `execution_logs` | 每条执行记录对应若干日志行（stdout / stderr / result 等） | `POST /api/backup/log-cleanup/trigger` |
| 文件 `~/.ntd/run.log` |macOS launchd接管，文件由 daemon写、由系统 logrotate 处理 | **不在** ntd清理范围内 |

> 「**日志清理**」的清理目标是数据库 `execution_logs` 表（参见 `backend/src/handlers/backup.rs::cleanup_old_logs`）。它跑的是 `DELETE FROM execution_logs WHERE timestamp < ...`，不是磁盘文件 mtime 删除。

---

## 2.日志位置（磁盘文件，**不**自动清理）

|日志 |位置 |
|------|------|
| 开发模式后端日志 | `backend.dev.log`（在仓库根） |
| 生产 daemon 日志（macOS） | `~/.ntd/run.log`（launchd `StandardOutPath`/`StandardErrorPath`，参见 `backend/src/daemon.rs::generate_launchd_plist`） |
| 生产 daemon 日志（Linux） | `journalctl --user -u ntd`（systemd `StandardOutput=journal`，参见 `backend/src/daemon.rs`） |
|错误日志（macOS） | `~/.ntd/run.error.log` |

> daemon **不**写 `daemon.log`。文档中所有 `~/.ntd/daemon.log`引用都是历史遗留，**实际路径是 `~/.ntd/run.log`**。

---

## 3.自动清理配置

唯一字段（`backend/src/config.rs::auto_cleanup_logs_days`）：
- 类型 `Option<usize>`，`None` = 不清理（默认在 `Config::default()` 是 `Some(30)`，但用户设为 `null` 后自动清理就停了）
- 设成数字 `N` 后，每次数据库自动备份后会跑 `DELETE FROM execution_logs WHERE timestamp < datetime('now', '-N days')`

UI入口：备份与恢复 → 日志清理 Tab

PUT body：
```json
{ "days":30 }
```

> 历史字段 `enabled` / `retention_days` / `cron` 已废弃，**当前只有一个 `auto_cleanup_logs_days: Option<usize>`字段**。

---

## 4.手动清理

### 4.1通过 ntd 接口

```bash
#  设成30 天并立即触发
curl -X PUT http://localhost:8088/api/backup/log-cleanup \
 -H "Content-Type: application/json" \
 -d '{"days":30}'

curl -X POST http://localhost:8088/api/backup/log-cleanup/trigger
```

返回：被删除的行数。

### 4.2直接跑 SQL

```bash
sqlite3 ~/.ntd/data.db "DELETE FROM execution_logs WHERE timestamp < datetime('now', '-30 days');"
```

> 注意：删完**不会**自动回收磁盘空间（SQLite默认行为），要回收空间需额外跑 `VACUUM`（参考 `database-optimize.md`）。

---

## 5.日志轮转（高级，针对磁盘 `.log` 文件）

>这里**不**涉及 ntd 「日志清理」功能，仅作为系统级 logrotate 参考。

如果想限制 `~/.ntd/run.log`体积，可以配系统级 logrotate：

`/etc/logrotate.d/ntd`：

```
/Users/me/.ntd/run.log {
 daily
 rotate7
 compress
 missingok
 notifempty
 copytruncate
}
```

- `daily`：每天轮转
- `rotate7`：保留7 个
- `copytruncate`：保留文件句柄，ntd继续写

---

## 6.ntd 日志格式

```
[2026-06-04T20:00:00.123Z] INFO ntd::handlers: 定时清理已完成
[2026-06-04T20:00:00.234Z] ERROR ntd::db: connection lost
```

-级别：TRACE / DEBUG / INFO / WARN / ERROR
- 模块路径：方便定位

---

## 7.调试技巧

### 7.1实时跟踪

```bash
#  macOS
tail -f ~/.ntd/run.log | grep -i "sync"

#  Linux（systemd）
journalctl --user -u ntd -f | grep -i "sync"
```

### 7.2找错误

```bash
#  macOS
grep -i "ERROR|panic" ~/.ntd/run.log

#  Linux（systemd）
journalctl --user -u ntd | grep -i "ERROR|panic"
```

### 7.3找特定请求

```bash
#  macOS
grep "todo_id=5" ~/.ntd/run.log

#  Linux（systemd）
journalctl --user -u ntd | grep "todo_id=5"
```

---

## 8.故障排查

### 8.1「日志清理」不工作

- 看后端日志 `backup_scheduler`关键字
- 检查 `auto_cleanup_logs_days` 是否被设为 `None`（即关闭）
- 看 `execution_logs` 表是否真的有老数据

### 8.2磁盘 `.log` 文件没自动清理

- ntd **不**自动清理磁盘日志文件（设计如此）
- 用系统 logrotate（macOS / Linux都有）

---

## 9.相关文档

- [备份与恢复](../settings/backup-and-restore.md)
- [数据库优化](database-optimize.md)
- [备份策略](backup-strategy.md)
