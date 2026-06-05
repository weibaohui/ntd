# 数据库优化

SQLite 跑久了会膨胀。定期 optimize 保持性能。

## 1. SQLite 为什么会膨胀

- DELETE 不会归还磁盘空间（标记为可复用）
- UPDATE 实际是 DELETE + INSERT
- 长期累积，文件大小 ≠ 实际数据大小

## 2. 优化操作

### 2.1 VACUUM

重建数据库文件，**真正归还**未使用空间。

```sql
VACUUM;
```

### 2.2 REINDEX

重建所有索引（碎片化后查询会变慢）。

```sql
REINDEX;
```

### 2.3 PRAGMA optimize

让 SQLite 自动跑一些优化（`ANALYZE` 等）。

```sql
PRAGMA optimize;
```

## 3. ntd 的优化入口

设置 → 备份与恢复 → 数据库备份 Tab → 「**优化数据库**」按钮

后端依次执行：
1. 备份当前数据库（先备份再操作）
2. `PRAGMA wal_checkpoint(TRUNCATE)` — 把 WAL 写回主库
3. `VACUUM`
4. `REINDEX`
5. `PRAGMA optimize`
6. 记录日志

执行期间 ntd **短暂不可用**（因为 SQLite 锁）。

## 4. 什么时候该优化

- 数据库 > 1GB
- 查询明显变慢
- 大量 Todo 被软删（`deleted_at != null`）
- 大量执行记录（> 10万条）后想清理

## 5. 优化前的硬性准备

1. **必须先备份**（ntd 帮你做了，但建议同时手动再备份一次）
2. 停掉所有跑着的 Todo（VACUUM 期间会卡）
3. 预留数据库文件 **2 倍大小**的磁盘空间（VACUUM 期间会有临时副本）

## 6. 手动优化（高级）

```bash
# 停 ntd
ntd daemon stop

# 用 sqlite3 CLI（如果有装）
sqlite3 ~/.ntd/data.db
> VACUUM;
> REINDEX;
> PRAGMA optimize;
> .exit

# 启动 ntd
ntd daemon start
```

或一行命令：

```bash
sqlite3 ~/.ntd/data.db "VACUUM; REINDEX; PRAGMA optimize;"
```

## 7. 定期清理执行记录

如果只是 execution_records 占空间，可以单独清：

```sql
-- 删 90 天前的
DELETE FROM execution_records WHERE created_at < datetime('now', '-90 days');

-- 清空全部（危险）
DELETE FROM execution_records;
```

删完跑 `VACUUM` 才会真正释放磁盘。

## 8. WAL 模式

ntd 默认开 SQLite WAL（Write-Ahead Logging）：

- `.db` 主文件
- `.db-wal` 写日志文件
- `.db-shm` 共享内存文件

WAL 提供更好的并发读，但**不会自动 checkpoint**。可以用：

```sql
PRAGMA wal_checkpoint(TRUNCATE);
```

把 WAL 写回主库并截断。ntd 的「优化数据库」会自动做这一步。

## 9. 性能监控

```sql
-- 数据库大小
SELECT page_count * page_size AS size FROM pragma_page_count(), pragma_page_size();

-- 各表行数
SELECT name, (SELECT count(*) FROM sqlite_master WHERE type='table' AND name=m.name) 
FROM sqlite_master m WHERE type='table';

-- 索引使用情况
EXPLAIN QUERY PLAN SELECT * FROM todos WHERE id = 5;
```

## 10. 故障排查

### 10.1 VACUUM 卡住

- 有其他进程持锁
- 杀 sqlite3 进程或重启 ntd

### 10.2 优化后还是慢

- 看是不是真有大量数据：`SELECT count(*) FROM execution_records;`
- 考虑只删老数据 + 重建索引
- 加磁盘（SSD > HDD）

### 10.3 数据库损坏

- 提示：`database disk image is malformed`
- 解决：从最近的备份恢复
- 没备份：试 `PRAGMA integrity_check;` 看损坏程度
- 极端情况：`sqlite3 old.db ".dump" | sqlite3 new.db` 导出再导入

## 11. 相关 API

| Method | Path | 用途 |
|--------|------|------|
| POST | `/api/backup/database/optimize` | 触发完整优化 |

## 12. 相关文档

- [备份策略](backup-strategy.md)
- [备份与恢复 - 数据库](../settings/backup-and-restore.md#1-数据库备份)
