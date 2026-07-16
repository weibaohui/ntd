/**
 * 「一键安装 Git」的 Prompt 模板常量。
 *
 * 设计取舍：
 * - 不在前端猜操作系统塞进 params，而是让执行器（如 Claude Code）自己在目标机器上检测系统后
 *   选择对应的安装命令。理由：前端 navigator.userAgent 可被改写、也不区分 Linux 发行版，
 *   而执行器跑在真实 shell 里，`uname`/包管理器探测比浏览器猜的准得多。
 * - 安装是「环境级」操作，与具体 workspace 无关（brew/apt/winget 都是全局安装），
 *   因此不依赖 workspace_id；用户仍可在 ActionButton Drawer 里选 workspace 作为执行 cwd。
 * - 末尾强制 `git --version` 验证：装完未必进当前 shell 的 PATH（尤其 macOS/Linux 新装），
 *   让执行器显式验证并把结果汇报回来，前端 onApply 再触发一次 is_git_available 重探。
 * - 权限提示单独列出来：apt/dnf 要 sudo、Windows 可能弹 UAC，提前告知执行器「需要密码时找用户要」，
 *   避免它在提权交互上卡死或静默失败。
 */
export const INSTALL_GIT_ACTION_TYPE = 'install_git';
export const INSTALL_GIT_ACTION_KEY = 'default';

export const INSTALL_GIT_PROMPT = `你的任务：在本机安装 Git 命令行工具。

请先检测当前操作系统，再选择对应的安装方式：

- macOS：优先用 Homebrew 执行 \`brew install git\`；若检测到尚未安装 Homebrew，先按官方方式安装 Homebrew（https://brew.sh），再装 git。
- Linux（Debian/Ubuntu）：\`sudo apt-get update && sudo apt-get install -y git\`。
- Linux（Fedora/RHEL/CentOS）：\`sudo dnf install -y git\`，旧系统无 dnf 则用 \`sudo yum install -y git\`。
- Windows：优先 \`winget install --id Git.Git -e --source winget\`；若无 winget，引导用户从 https://git-scm.com/download/win 下载安装包并执行。

注意事项：
- 涉及 sudo 或 UAC 提权时，如需密码请在终端提示用户输入，不要卡死或反复重试。
- 安装完成后，运行 \`git --version\` 验证已正确安装并能看到版本号。
- 若新装的 git 在当前 shell 还不在 PATH 里（常见于刚装完未重开终端），请明确告知用户「需要新开一个终端」。

完成后简要汇报三点：检测到的操作系统、实际执行的安装命令、最终的 \`git --version\` 输出。`;
