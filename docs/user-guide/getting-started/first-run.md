# 首次运行

刚装好 ntd 后的 5 步上手流程。

## Step 1: 配置至少一个执行器

ntd 本身不执行代码，**必须**先配好至少一个执行器（Claude Code / Codex / Hermes / Kimi 等）才能跑 Todo。

设置 → 执行器管理 → 「**自动检测全部**」→ 让 ntd 自动找已安装的 CLI

详细看 [执行器管理](../settings/executors.md)

## Step 2: 创建第一个 Todo

主界面右上「**+ 新建**」或快捷键：

- 填**标题**：一句话说清楚要做什么
- 填**Prompt**：给执行器的完整指令
- 选**执行器**：默认用刚才配好的
- 选**工作目录**（可选）：在「项目目录」白名单里挑
- 选**标签**（可选）
- 「**保存**」

## Step 3: 执行

Todo 详情 → 右上「**执行**」按钮 → 实时日志流出来。

第一次跑可能会因为：
- CLI 第一次运行要授权（macOS 弹窗）
- 缺 API key
- workspace 路径不存在

详细排查看 [执行器管理 - 故障排查](../settings/executors.md#故障排查)

## Step 4: 看看历史和统计

- Todo 详情 → 历史链：所有执行记录
- Todo 详情 → Token 统计：每次消耗的 token
- 仪表盘：全局指标

## Step 5: 体验一下高级功能

- [云端同步](../settings/cloud-sync.md) — 多设备同步 Todo
- [Webhook](../settings/webhooks.md) — 让 CI 触发 Todo
- [飞书 Bot](../settings/messages-feishu.md) — 群里 @ 机器人
- [Skills 管理](../../frontend-features.md) — 给执行器装 Skills
- [备份与恢复](../settings/backup-and-restore.md) — 重要数据先备份

## 提示

- **建议先开自动备份**：设置 → 备份与恢复 → 数据库 → 自动备份，每天 04:00，保留 10 个文件
- **建议先开「执行前确认」**：系统设置 → `execution_timeout_secs` 调成 7200（2 小时），避免执行器卡死
- **项目目录**：在「项目目录」tab 添加你常用的代码目录，Todo 工作目录从白名单选更安全
