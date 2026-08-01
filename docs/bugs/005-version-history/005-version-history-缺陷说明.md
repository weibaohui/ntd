# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-08-01 | 初始版本：BUG-005 缺陷说明 |

# 1. 缺陷标题

process versions/diff 名存实亡：无版本历史存储

# 2. 缺陷描述

`versions` 永远只返回当前 1 条；`diff` 跨版本 404，同版本返回全量 unchanged 的伪 diff。`process_templates` 一行一 guid（当前版本），没有历史快照。

# 3. 修复方案

新增 `process_template_versions` 表 + 保存时落快照 + versions/diff 从快照读取。
