# 日志清理

ntd 跑久了会留下大量日志。`backend.dev.log` 和 `daemon.log` 几天就能上 GB。

## 1. 日志位置

| 日志 | 位置 |
|------|------|
| 开发模式后端日志 | `backend.dev.log`（在仓库根） |
| 生产 daemon 日志 | `~/.ntd/daemon.log` |
| ntd-cloud 日志 | `/path/to/ntd-cloud/backend/ntd_cloud.log` |

## 2. 自动清理

设置 → 备份与恢复 → 日志清理 Tab：

| 字段 | 默认 | 含义 |
|------|------|------|
| `enabled` | false | 是否开启 |
| `retention_days` | 30 | 保留天数 |
| `cron` | - | 清理周期 |

### 2.1 行为

- 按修改时间删：**mtime 超过 retention_days** 的日志文件**被删**
- 活跃文件（正在写的）**不会被删**
- 只删日志文件（`.log` / `.log.gz`），其他文件不动

### 2.2 手动触发

点「**立即清理**」按钮。

## 3. 手动清理

### 3.1 找大文件

```bash
du -sh ~/.ntd/*.log
```

### 3.2 截断活跃文件

```bash
# 把当前日志清空，但保留文件句柄
: > ~/.ntd/daemon.log
```

比直接 `rm` 安全 —— 进程不会因为找不到文件而停止写日志。

### 3.3 找过期文件

```bash
find ~/.ntd -name "*.log" -mtime +30 -ls
find ~/.ntd -name "*.log.gz" -mtime +30 -ls
```

### 3.4 删

```bash
find ~/.ntd -name "*.log" -mtime +30 -delete
find ~/.ntd -name "*.log.gz" -mtime +30 -delete
```

## 4. 日志轮转（高级）

如果不想用 ntd 内置清理，可以配系统级 logrotate：

`/etc/logrotate.d/ntd`：

```
/Users/me/.ntd/daemon.log {
    daily
    rotate 7
    compress
    missingok
    notifempty
    copytruncate
}
```

- `daily`：每天轮转
- `rotate 7`：保留 7 个
- `copytruncate`：保留文件句柄，ntd 继续写

## 5. ntd 日志格式

```
[2026-06-04T20:00:00.123Z] INFO ntd::handlers: Webhook records cleanup completed
[2026-06-04T20:00:00.234Z] ERROR ntd::db: connection lost
```

- 级别：TRACE / DEBUG / INFO / WARN / ERROR
- 模块路径：方便定位

## 6. 调试技巧

### 6.1 实时跟踪

```bash
tail -f ~/.ntd/daemon.log | grep -i "sync\|webhook"
```

### 6.2 找错误

```bash
grep -i "ERROR\|panic" ~/.ntd/daemon.log
```

### 6.3 找特定请求

```bash
grep "todo_id=5" ~/.ntd/daemon.log
```

## 7. 故障排查

### 7.1 自动清理不工作

- 看后端日志 `log_cleanup` 关键字
- 验证 Cron 表达式
- 手动触发试试

### 7.2 删了日志后服务异常

- 一般不会：ntd 日志是只写的
- 但如果进程持有的句柄被强制 close，可能出错
- 重启服务恢复

## 8. 相关文档

- [备份与恢复](../settings/backup-and-restore.md#4-日志清理)
- [备份策略](backup-strategy.md)
