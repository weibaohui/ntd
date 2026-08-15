# MEMORY.md — 环境与长期约定记忆

> 本文件是 AI 工具跨会话记忆。修改需谨慎，保持简洁、只记稳定事实。

## Chrome CDP 调试环境（2026-08-15 配置）

- **用途**：通过 CDP 操作真实 Chrome（agent-browser CLI，`--cdp 9222`）。
- **启动命令**：
  ```bash
  open -a "Google Chrome" --args --remote-debugging-port=9222 --user-data-dir="$HOME/.chrome-cdp-profile"
  ```
- **原因**：Chrome 136+ 安全策略——默认用户数据目录下会**忽略** `--remote-debugging-port`，必须配独立 `--user-data-dir` 才能开调试端口。
- **重要约束**：
  - 调试实例用独立 profile（`~/.chrome-cdp-profile`），**无原 Chrome 的标签页/扩展/登录态**。
  - 原环境恢复：退出调试实例后 `open -a "Google Chrome"`（不带参数）即回默认 profile，但 9222 端口随之关闭。
- **验证**：`curl http://127.0.0.1:9222/json/version` 应返回 Chrome/151.x JSON。
- **agent-browser**：v0.32.2，已装好（doctor 8/8）。连接调试实例：`agent-browser open <url> --cdp 9222 [--session <名称>]`。

## 开发环境

- dev 实例：`make dev`（18088，embedded 模式，前端 dist 打进后端二进制）；`make stop` 停止。
- 生产实例：`ntd daemon`（8088），配置 `~/.ntd/`。
- 当前 Chrome 调试端口 Chrome 版本：151.0.7922.138。
