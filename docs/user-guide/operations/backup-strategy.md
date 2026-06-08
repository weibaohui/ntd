# 备份策略

ntd 的数据分三类，需要**分别考虑**备份策略。

## 1. 备份对象

| 对象 | 大小 | 变化频率 | 丢失影响 |
|------|------|----------|----------|
| SQLite 数据库 | 几 MB ~ 几 GB | 中（每次 Todo 写） | **灾难性** |
| Todo YAML 备份 | 几 KB | 低 | 中（可重建） |
| Skill zip | 几 MB | 低 | 中（可重装） |
| 飞书历史 | 几 MB | 高 | 低（可重拉） |

## 2. 推荐策略

### 2.1 个人用户

```
每天 04:00  数据库自动备份（保留 30 个 — 默认值）
每周日 03:00 Todo YAML 自动备份（保留 4 个）
每月       手动导出一次 Skills zip
```

### 2.2 团队用户

```
每天 04:00  数据库自动备份
           备份完 push 到 S3/NAS（rsync 脚本）
每天 23:00  日志清理
每周日 03:00 Todo YAML 自动备份
每周六 02:00 Skills 自动备份
```

### 2.3 跨地域容灾

- 主：本地磁盘
- 异地：对象存储（S3 / 阿里云 OSS / 腾讯云 COS）
- 工具：`rclone`、`rsync`、`aws s3 sync`

## 3. Cron 表达式速查

| 表达式 | 含义 |
|--------|------|
| `0 0 4 * * *` | 每天 4:00 |
| `0 0 3 * * 0` | 每周日 3:00 |
| `0 0 2 * * 6` | 每周六 2:00 |
| `0 0 4 1 * *` | 每月 1 号 4:00 |
| `0 30 23 * * *` | 每天 23:30 |

> ntd 用的是 6 位 Cron（带秒），与 Linux crontab 的 5 位略有不同。

## 4. 备份文件命名

| 类型 | 格式 |
|------|------|
| 数据库 | `data-YYYY-MM-DD-HHMMSS.db.gz` |
| Todo | `todos-YYYY-MM-DD-HHMMSS.yaml` |
| Skill | `skills-YYYY-MM-DD-HHMMSS.zip` |

按时间倒序，**最新的文件名最特殊**（含时间戳）。

## 5. 容量估算

假设每天做 100 个 Todo、每次 5K token：

- SQLite 增长：~ 5MB/天（含 execution_records 和日志）
- 压缩后数据库：~ 1.5MB/天
- 10 个备份：~ 15MB

完全在可控范围内。

## 6. 异地备份脚本（rsync 示例）

```bash
#!/bin/bash
# 每天跑一次
SRC=~/.ntd/backups/db/
DEST=user@nas:/volume1/ntd-backups/

rsync -avz --delete "$SRC" "$DEST"
```

把这个脚本放进 `cron` / `launchd`。

## 7. 恢复演练

**每季度做一次**：

1. 选一个最近的备份
2. 在测试环境解包
3. 启动 ntd，看数据是否完整
4. 跑几个 Todo 看是否能用

不演练的备份等于没备份。

## 8. 故障排查

### 8.1 备份失败

- 看后端日志 `backup::` 关键字
- 检查目录权限
- 检查磁盘空间：`df -h ~/.ntd/`

### 8.2 备份没触发

- 确认开关开启
- 看 Cron 是否注册（`crontab -l` 对比）
- ntd 启动后会重新注册所有 Cron

### 8.3 备份文件损坏

- 下载时网络断了
- 重新触发一次备份
- 恢复前用 `gzip -t` 测一下

## 9. 相关文档

- [备份与恢复](../settings/backup-and-restore.md) — UI 操作详解
- [日志清理](log-cleanup.md) — daemon.log 清理策略
- [数据库优化](database-optimize.md) — 备份前的 vacuum
