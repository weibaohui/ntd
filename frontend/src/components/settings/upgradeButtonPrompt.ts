/**
 * 「手动升级（AI 一键执行）」的 Prompt 模板常量。
 *
 * 设计取舍：
 * - 通过 ActionButton 走 AI 执行器来跑升级命令，比"复制命令到 AI Coding 工具"
 *   的体验更顺滑：用户点一下按钮，在 Drawer 里看实时执行日志，完事。
 * - **关键约束**：Prompt 只让 AI 升级 npm 包，不在这里执行 `ntd daemon restart`。
 *   原因：daemon restart 会让主进程 exit(0)，AI 子进程随之被 SIGKILL——
 *   ActionButton 的 WebSocket 会断连、UI 状态会卡在 executing，给用户带来
 *   "AI 没跑完"的错觉。拆成两步：①AI 升级 npm 包；②完成态点"立即重启"按钮
 *   触发后端 \`/api/version/upgrade\`，此时后端按既定流程 fork 子进程重启服务。
 * - 让执行器自己判断环境（macOS / Linux / Windows）选择正确的 daemon 守护方式：
 *   macOS 用 launchd（plist 在 ~/Library/LaunchAgents/）、Linux 用 systemd
 *   （unit 文件在 ~/.config/systemd/user/）、Windows 用 SCM。
 *   前端不参与这些平台分支，避免维护多套命令。
 * - 不在前端 prompt 里写死具体路径：npm 全局 prefix 由 `npm config get prefix`
 *   动态探测，传给 `npm install -g --prefix=<prefix>` 避免无写权限失败。
 */

export const MANUAL_UPGRADE_ACTION_TYPE = 'system_upgrade';
export const MANUAL_UPGRADE_ACTION_KEY = 'manual';

/**
 * 升级用的 Prompt 模板（仅升级 npm 包，不重启服务）。
 *
 * 关键步骤：
 * 1. `npm install -g` 升级 @weibaohui/ntd 包（用探测到的 prefix 路径）
 * 2. 完成后**只**汇报探测到的 prefix、npm 升级成功与否、当前已安装版本号
 *
 * 不在这里执行 `ntd daemon restart`：完成态的「立即重启」按钮会统一触发后端
 * 的 restart 流程，避免 AI 子进程被主进程重启中断。
 */
export const MANUAL_UPGRADE_PROMPT = `你的任务：升级本机安装的 NTD (Now Task, Done) 命令行工具到最新版本。

请按顺序执行以下步骤：

1. **探测 npm 全局目录**
   - 运行 \`npm config get prefix\` 拿到 npm 全局安装路径（记为 \`PREFIX\`）。
   - 该路径需要可写；如果无写权限，提示用户用 root / sudo 重新运行 ntd 服务，
     或参考 \`npm config set prefix '~/.npm-global'\` 改为用户级目录。

2. **升级 npm 包**
   - 运行 \`npm install -g --prefix="$PREFIX" @weibaohui/ntd@latest\`。
   - 如果报权限错误，立即停下来把错误原文回显给用户，不要继续。

3. **确认新版本已就位**
   - 运行 \`ntd --version\`（或 \`ntd -V\`）拿到当前 binary 版本号，确认与 npm 升级后的版本一致。

4. **汇报结果（不要执行 daemon restart）**
   - 简要汇报：探测到的 prefix、npm 升级是否成功、当前 ntd 版本号。
   - **不要** 在这里执行 \`ntd daemon install\` / \`ntd daemon restart\` /
     \`ntd daemon start\`。服务重启由前端在「立即重启」按钮中统一触发，
     这样可以确保主进程优雅退出、用户看到新版本页面。
   - 整个过程预计 10–30 秒；npm 升级可能稍久，请耐心等待。
   - 涉及 sudo / 提权时直接在终端向用户索要密码，不要静默失败。
   - 如果中途任一步失败，把 \`npm install\` 的原始 stdout / stderr 完整回显给用户。`;
