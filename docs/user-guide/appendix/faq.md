# 常见问题 (FAQ)

## 安装

**Q: npm 全局安装需要 sudo 吗？**
A: 默认 prefix 在 `/usr/local/lib/node_modules` 需要 sudo。可以用 `npm config set prefix '~/.npm-global'` 改到用户目录。

**Q: 装完后 `ntd` 命令找不到？**
A: PATH 没包含 npm bin 目录。`npm prefix -g` 看路径，加到 `~/.zshrc` 或 `~/.bash_profile`。

**Q: 可以 Docker 部署吗？**
A: 可以，但官方不提供镜像。自行 build：参考 `Dockerfile` 模式（基于 rust:1 + node:20）。

## 启动

**Q: `ntd daemon install` 和 `ntd daemon start` 区别？**
A: `install` 注册为系统服务（launchd / systemd），开机自启；`start` 立即启动当前进程。生产两步都要做。

**Q: 启动报「address already in use」？**
A: 端口被占。`lsof -i :8088` 找占用的进程，杀掉或改 `server_port`。

**Q: 数据库锁错误？**
A: SQLite 写锁冲突。检查是否同时跑了 dev 和 prod 用同一个数据库。

## Todo

**Q: Todo 跑完但状态还是 running？**
A: 执行器没正常退出。检查 `execution_timeout_secs` 是否太长，或执行器卡死（后端默认 3600 秒；前端的 SystemSettingsPanel 表单默认值 1800 秒）。

**Q: 怎么恢复软删的 Todo？**
A: UI 没提供。直接 SQL：`UPDATE todos SET deleted_at = NULL WHERE id = ?`

**Q: 同一个 Todo 跑多次算几个执行记录？**
A: 每次都是独立 execution_record，不覆盖。

## 执行器

**Q: 加新的执行器要改代码吗？**
A: 不用。9 个内置执行器（Claude Code / CodeBuddy / OpenCode / AtomCode / Hermes / Kimi / JoinAI / Codex / CodeWhale）开箱即用。新增执行器需要改后端（继承 Executor trait）。

**Q: 怎么知道执行器是否真在跑？**
A: 看「运行管理」面板。状态为 running 就是真在跑。

**Q: Claude Code 报错「API key not set」？**
A: `claude` 工具第一次跑会引导你登录/配 key，按提示走。

## 云端同步

**Q: 自建 ntd-cloud 必须吗？**
A: 是。云端服务不是公共的，需要你自己跑 ntd-cloud-server 进程。

**Q: 同步历史清空了还能找回吗？**
A: 不能。清空是物理删除。

**Q: 推送 5 条但云端只显示 3 条？**
A: 看云端返回的 detail，可能被云端 dedup 了。

## 备份

**Q: 数据库备份恢复后数据不一致？**
A: 备份是「快照」语义。恢复时确保 ntd 完全停止再覆盖文件。

**Q: 自动备份没触发？**
A: 检查开关、Cron 表达式、服务是否在跑。

## 飞书 Bot

**Q: 飞书后台的回调地址怎么填？**
A: `https://你的ntd公网地址/api/agent-bots/feishu/callback`（ntd-cloud 端处理，ntd 本体只调 OAuth）

**Q: 群里 @ 机器人无反应？**
A: 检查群白名单、Bot 状态、消息是否被 message_debounce 去重。

**Q: 飞书消息丢失？**
A: ntd 用 message_debounce 默认 20 秒（按 bot × p2p/group 可独立配置）内同群高频消息去重。看 `feishu_messages` 表。

## 性能

**Q: 跑久了 dashboard 变慢？**
A: 删老 execution_records + VACUUM。

**Q: SQLite 适合生产吗？**
A: 单机生产够用（万级 Todo 没问题）。多节点需要换 PostgreSQL（ntd 暂不支持）。

## 升级

**Q: 跨大版本升级要注意什么？**
A: 配置文件可能有 breaking change，看 release notes。最好先备份再升级。

**Q: 升级失败怎么回滚？**
A: `npm install -g @weibaohui/nothing-todo@<老版本>`，然后 `ntd daemon restart`。

## 其他

**Q: 怎么贡献代码？**
A: 仓库根有 `CONTRIBUTING.md`（如有）。一般流程：fork → 改 → PR。

**Q: 商用需要授权吗？**
A: 看 LICENSE 文件。本项目按开源协议。

**Q: 有移动端 App 吗？**
A: 暂无。Web UI 在手机浏览器也能用，响应式布局。
