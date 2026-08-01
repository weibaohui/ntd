# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-08-01 | 初始版本：根因分析 |

# 1. 根因分析

`get_process_versions`/`diff_process_versions` 按 guid 过滤 `process_templates`，但一行一 guid 无历史。保存时只覆盖当前行，不存快照。

# 2. 修复策略

1. 迁移 v86：新建 `process_template_versions`（guid, version, definition, created_at, UNIQUE(guid,version)）
2. `update_process` 保存时写入快照
3. `get_process_versions`/`diff_process_versions` 优先从快照读取，空则回退当前行
