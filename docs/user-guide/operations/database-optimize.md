# 数据库优化

SQLite跑久了会膨胀。定期 optimize保持性能。

---

## 1.SQLite为什么会膨胀

-DELETE不会归还磁盘空间（标记为可复用）
- UPDATE实际是 DELETE + INSERT
-长期累积，文件大小 ≠实际数据大小

---

## 2.优化操作（SQL层面）

### 2.1 VACUUM

重建数据库文件，**真正归还**未使用空间。

```sql
VACUUM;
```

> VACUUM期间 ntd **短暂不可用**（SQLite排他锁），且需要原数据库 **2 倍大小**的磁盘临时空间。

### 2.2 REINDEX

重建所有索引（碎片化后查询会变慢）。

```sql
REINDEX;
```

### 2.3 PRAGMA optimize

让 SQLite 自动跑一些优化（`ANALYZE` 等，**只更新统计信息**，**不**重建表、不收缩文件）。

```sql
PRAGMA optimize;
```

---

## 3.ntd 的优化入口

> ⚠️ **当前实现只跑 `PRAGMA optimize`**，**不**包含 VACUUM / REINDEX / wal_checkpoint 等。

UI入口：备份与恢复 → 数据库备份 Tab →「**优化数据库**」按钮

后端实际行为（`backend/src/handlers/backup.rs::database_optimize`）：

1. 检查数据库文件是否存在
2. 执行 `PRAGMA optimize`（更新查询计划统计信息）
3. 返回「数据库优化完成」

**不会**：
-备份当前数据库
- 执行 `VACUUM`
- 执行 `REINDEX`
- 执行 `wal_checkpoint(TRUNCATE)`
-收缩文件大小

>真正想释放空间需**手动** `sqlite3 data.db "VACUUM"`（参考第6 节）。备份是另一个独立入口 `POST /api/backup/database/trigger`。

---

## 4.什么时候该优化

- 数据库 >1GB
- 查询明显变慢
-大量 Todo被软删（`deleted_at != null`）
-大量执行记录（>10万条）后想清理

> 注意：「优化」只更新统计信息，**不会**让磁盘文件变小。如果文件已膨胀（VACUUM一次能瘦身），请手动跑 VACUUM。

---

## 5.优化前的硬性准备

1. **必须先备份**（**当前实现不会自动备份**，需手动）
2.停掉所有跑着的 Todo（VACUUM期间会卡）
3.预留数据库文件 **2 倍大小**的磁盘空间（VACUUM期间会有临时副本）

---

## 6.手动优化（高级）

```bash
# 停 ntd
ntd daemon stop

#  用 sqlite3 CLI（如果有装）
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

---

## 7.定期清理执行记录

如果只是 execution_records 占空间，可以单独清：

```sql
--删90 天前的
DELETE FROM execution_records WHERE created_at < datetime('now', '-90 days');

--清空全部（危险）
DELETE FROM execution_records;
```

>删完跑 `VACUUM`才会真正释放磁盘。

---

## 8.WAL模式

ntd 默认开 SQLite WAL（Write-Ahead Logging）：

- `.db` 主文件
- `.db-wal`写日志文件
- `.db-shm`共享内存文件

WAL 提供更好的并发读，但**不会自动 checkpoint**。可以用：

```sql
PRAGMA wal_checkpoint(TRUNCATE);
```

把 WAL写回主库并截断。

> ntd 的「优化数据库」按钮**不会**自动跑这一步（参见第3 节）。

---

## 9.性能监控

```sql
--数据库大小
SELECT page_count * page_size AS size FROM pragma_page_count(), pragma_page_size();

--各表行数
SELECT name, (SELECT count(*) FROM sqlite_master WHERE type='table' AND name=m.name)
FROM sqlite_master m WHERE type='table';

--索引使用情况
EXPLAIN QUERY PLAN SELECT * FROM todos WHERE id =5;
```

---

## 10.故障排查

### 10.1 VACUUM卡住

- 有其他进程持锁
-杀 sqlite3进程或重启 ntd

### 10.2优化后还是慢

- 看是不是真有大量数据：`SELECT count(*) FROM execution_records;`
-考虑只删老数据 +重建索引
- 加磁盘（SSD > HDD）

### 10.3数据库损坏

-提示：`database disk image is malformed`
-解决：从最近的备份恢复
- 没备份：试 `PRAGMA integrity_check;`看损坏程度
-极端情况：`sqlite3 old.db ".dump" | sqlite3 new.db`导出再导入

---

## 11.相关 API

| Method | Path |用途 |
|--------|------|------|
| POST | `/api/backup/database/optimize` | ⚠️ 当前实现仅执行 `PRAGMA optimize`，**不含** VACUUM/REINDEX/wal_checkpoint |
| POST | `/api/backup/database/trigger` |立即触发数据库备份（zip压缩到 `~/.ntd/backups/db/`） |

---

## 12.相关文档

- [备份策略](backup-strategy.md)
- [备份与恢复 - 数据库](../settings/backup-and-restore.md)
- [日志清理](log-cleanup.md)
