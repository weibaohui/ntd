# 0. 文件修改记录表

| 修改人 | 修改时间 | 修改内容 |
|--------|---------|---------|
| AI | 2026-08-01 | 初始版本：根因分析 |

# 1. 根因分析

`build_create_body` 在 `--stdin` 时直接调 `read_stdin_json()` 期望 JSON body。`--name` 帮助说「--stdin 模式下可省略，从 body 读」，但用户更可能用 heredoc 写 YAML。

# 2. 修复策略

1. `--stdin` 帮助改为「JSON body 或 YAML 正文」
2. 有 `--name` 时 stdin 视作 YAML 正文（与 --file 同语义），无 --name 时视作 JSON body
