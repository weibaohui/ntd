# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-08-01 | 初始版本：BUG-006 缺陷说明 |

# 1. 缺陷标题

CLI `create --stdin` 要求全量 JSON body，与用法暗示不符

# 2. 缺陷描述

`--stdin` 用法行暗示是 `--file` 的替代（YAML），实际要求全量 JSON body（含 name/definition）。直接 pipe YAML 报 "Invalid JSON from stdin"。

# 3. 修复方案

更新帮助文本；支持「--name + stdin=YAML」组合（heredoc 友好）。
