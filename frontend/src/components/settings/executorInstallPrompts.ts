/**
 * 各 AI 执行器的一键安装 Prompt 模板。
 *
 * 设计取舍：
 * - 所有执行器共用 actionType = 'install_executor'，用 actionKey = 执行器名区分 todo，
 *   避免为每个执行器新增 action_type 污染 todos 表。
 * - 操作系统由执行器在目标机器自行检测（uname / 包管理器），不依赖前端 UA。
 * - 每个 prompt 提供官方/常见安装方式，并允许 AI 在命令失败时查看官方最新文档。
 * - 安装完成后必须执行验证命令，确认 CLI 可用。
 * - 涉及 sudo / UAC 提权时要求 AI 提示用户输入密码，不得卡死或反复重试。
 * - 部分执行器安装后仍需登录/授权/配置 API Key，prompt 会明确告知用户后续手动步骤。
 */

export const INSTALL_EXECUTOR_ACTION_TYPE = 'install_executor';

/**
 * 已知执行器安装信息。
 * packageName：官方包名或安装命令关键字，用于 Homebrew / npm / winget 等场景。
 * binaryName：验证时使用的 CLI 命令名（可能与执行器名不同，如 zhanlu → zl）。
 * verifyArgs：验证参数，默认 --version。
 * notes：特殊说明（如需要登录、仅脚本安装等）。
 */
interface ExecutorInstallHints {
  /** 执行器展示名 */
  displayName: string;
  /** 验证用的 CLI 命令名 */
  binaryName: string;
  /** 验证参数 */
  verifyArgs: string;
  /** macOS 安装命令/说明 */
  macos: string;
  /** Linux 安装命令/说明 */
  linux: string;
  /** Windows 安装命令/说明 */
  windows: string;
  /** 额外注意事项，如需要登录 */
  notes?: string;
}

/**
 * 根据安装信息生成标准化安装 prompt。
 * 函数体保持精简：拼接固定格式字符串，把 OS 检测、安装方式、验证、注意事项串起来。
 */
function buildInstallPrompt(hints: ExecutorInstallHints): string {
  const verifyCommand = `${hints.binaryName} ${hints.verifyArgs}`;
  return `你的任务：在本机安装 ${hints.displayName} 命令行工具（可执行文件应能被 shell 找到，命令名为 \`${hints.binaryName}\`）。

请先检测当前操作系统，再选择对应的安装方式：

- macOS：${hints.macos}
- Linux（Debian/Ubuntu）：${hints.linux}
- Linux（Fedora/RHEL/CentOS）：先用 \`sudo dnf install -y <包名>\`；旧系统无 dnf 则用 \`sudo yum install -y <包名>\`。若官方未提供 dnf/yum 源，按官方文档推荐方式安装。
- Windows：${hints.windows}

安装原则：
1. 优先使用 npm / pip / 官方安装脚本进行安装；其次考虑包管理器（Homebrew / apt / dnf / yum / winget）。
2. 如果以上方式安装失败或官方未提供包，前往该执行器官网查看最新安装脚本/二进制下载地址，下载并放到 PATH 中的目录（如 \`~/.local/bin\`、\`/usr/local/bin\`、Windows 的 PATH 目录），必要时执行 \`chmod +x\`。
3. 涉及 sudo 或 UAC 提权时，如需密码请在终端提示用户输入，不要卡死或反复重试。
4. 安装完成后，运行 \`${verifyCommand}\` 验证已正确安装并能看到版本号。
5. 如果新装的命令在当前 shell 还不在 PATH 里（常见于刚装完未重开终端），请明确告知用户「需要新开一个终端」。
${hints.notes ? '\n' + hints.notes + '\n' : ''}
完成后简要汇报三点：检测到的操作系统、实际执行的安装命令、最终的 \`${verifyCommand}\` 输出。`;
}

// ─── 各执行器安装提示词 ──────────────────────────────────────

export const INSTALL_CLAUCODE_ACTION_KEY = 'claudecode';
export const INSTALL_CLAUCODE_PROMPT = buildInstallPrompt({
  displayName: 'Claude Code',
  binaryName: 'claude',
  verifyArgs: '--version',
  // 官方推荐：https://code.claude.com/docs/en/quickstart
  // macOS/Linux: curl -fsSL https://claude.ai/install.sh | bash
  // Windows: irm https://claude.ai/install.ps1 | iex
  // 备选: npm install -g @anthropic-ai/claude-code
  macos: '优先用官方原生安装脚本：\`curl -fsSL https://claude.ai/install.sh | bash\`（自动更新，推荐）；或 npm 全局安装：\`npm install -g @anthropic-ai/claude-code\`（需 Node.js 18+）。',
  linux: '优先用官方安装脚本：\`curl -fsSL https://claude.ai/install.sh | bash\`（自动更新，推荐）；或 npm 全局安装：\`npm install -g @anthropic-ai/claude-code\`。',
  windows: '优先用官方 PowerShell 安装：\`irm https://claude.ai/install.ps1 | iex\`（自动更新，推荐）；或 npm 全局安装：\`npm install -g @anthropic-ai/claude-code\`。',
  notes: 'Claude Code 安装后首次运行需要登录 Anthropic 账号，请明确告知用户这一步需要手动完成。',
});

export const INSTALL_CODEBUDDY_ACTION_KEY = 'codebuddy';
export const INSTALL_CODEBUDDY_PROMPT = buildInstallPrompt({
  displayName: 'CodeBuddy',
  binaryName: 'codebuddy',
  verifyArgs: '--version',
  // 官方文档：https://www.codebuddy.ai/docs/cli/installation
  // npm: @tencent-ai/codebuddy-code
  // 脚本: https://copilot.tencent.com/cli/install.sh
  macos: '优先用官方安装脚本：\`curl -fsSL https://copilot.tencent.com/cli/install.sh | bash\`（不必预装 Node.js）；或 npm 全局安装：\`npm install -g @tencent-ai/codebuddy-code\`（需 Node.js 18.20+）。',
  linux: '优先用官方安装脚本：\`curl -fsSL https://copilot.tencent.com/cli/install.sh | bash\`；或 npm 全局安装：\`npm install -g @tencent-ai/codebuddy-code\`。',
  windows: '优先用官方 PowerShell 安装：\`irm https://copilot.tencent.com/cli/install.ps1 | iex\`；或 npm 全局安装：\`npm install -g @tencent-ai/codebuddy-code\`。',
});

export const INSTALL_OPENCODE_ACTION_KEY = 'opencode';
export const INSTALL_OPENCODE_PROMPT = buildInstallPrompt({
  displayName: 'Opencode',
  binaryName: 'opencode',
  verifyArgs: '--version',
  // 官方安装：https://opencode.ai/install
  // npm: opencode-ai（旧版），新版为 Go 二进制
  // brew: brew install opencode
  // scoop: scoop install opencode
  macos: '优先用官方安装脚本：\`curl -fsSL https://opencode.ai/install | bash\`（推荐）；或用 npm 全局安装：\`npm install -g opencode-ai\`（旧版 npm 包）；也可用 Homebrew：\`brew install opencode\`。',
  linux: '优先用官方安装脚本：\`curl -fsSL https://opencode.ai/install | bash\`（推荐）；或用 npm 全局安装：\`npm install -g opencode-ai\`。',
  windows: '优先用官方安装脚本：\`curl -fsSL https://opencode.ai/install | bash\`（WSL 下可用）；或 scoop：\`scoop install opencode\`；也可用 npm：\`npm install -g opencode-ai\`。',
  notes: 'Opencode 有新旧两个版本，新版为 Go 自包含二进制，旧版为 npm 包（opencode-ai），命令参数不同，安装后请确认 \`opencode --version\` 能正常输出。',
});

export const INSTALL_ATOMCODE_ACTION_KEY = 'atomcode';
export const INSTALL_ATOMCODE_PROMPT = buildInstallPrompt({
  displayName: 'AtomCode',
  binaryName: 'atomcode',
  verifyArgs: '--version',
  // 官网：https://atomcode.atomgit.com/
  // 安装脚本：https://atomcode.atomgit.com/install.sh
  // cargo: cargo install atomcode（需 Rust 1.80+）
  // Homebrew cask: brew install --cask atomcode
  macos: '优先用官方安装脚本：\`curl -fsSL https://atomcode.atomgit.com/install.sh | sh\`（推荐，单文件二进制，无需 Node.js）；或用 cargo 安装：\`cargo install atomcode\`（需 Rust 1.80+）。',
  linux: '优先用官方安装脚本：\`curl -fsSL https://atomcode.atomgit.com/install.sh | sh\`（推荐，安装到 /usr/local/bin）；或直接下载二进制：\`sudo curl -L -o /usr/local/bin/atomcode https://release.atomgit.com/atomcode/latest/linux-x86_64/atomcode && sudo chmod +x /usr/local/bin/atomcode\`。',
  windows: '优先用官方 PowerShell 安装：\`irm https://atomcode.atomgit.com/install.ps1 | iex\`；或从官网下载安装包。',
});

export const INSTALL_HERMES_ACTION_KEY = 'hermes';
export const INSTALL_HERMES_PROMPT = buildInstallPrompt({
  displayName: 'Hermes',
  binaryName: 'hermes',
  verifyArgs: '--version',
  // 官方: https://hermes-agent.nousresearch.com
  // npm 包名可能为 hermes-git 或其他；官方推荐 curl 安装
  macos: '优先用官方安装脚本：\`curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash\`；或用 npm 全局安装：\`npm install -g hermes-git\`（Hermes Git CLI，需 Node.js 18+）。',
  linux: '优先用官方安装脚本：\`curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash\`；或用 npm 全局安装：\`npm install -g hermes-git\`。',
  windows: '优先用官方 PowerShell 安装脚本；或用 npm 全局安装：\`npm install -g hermes-git\`。',
});

export const INSTALL_KIMI_ACTION_KEY = 'kimi';
export const INSTALL_KIMI_PROMPT = buildInstallPrompt({
  displayName: 'Kimi',
  binaryName: 'kimi',
  verifyArgs: '--version',
  // 官方: https://code.kimi.com
  // 安装脚本: curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash
  // npm: @moonshot-ai/kimi-code
  macos: '优先执行官方安装脚本：\`curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash\`（推荐）；或用 npm 全局安装：\`npm install -g @moonshot-ai/kimi-code\`。',
  linux: '优先执行官方安装脚本：\`curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash\`；或用 npm 全局安装：\`npm install -g @moonshot-ai/kimi-code\`。',
  windows: '优先执行 PowerShell 安装脚本：\`irm https://code.kimi.com/kimi-code/install.ps1 | iex\`；或用 npm 全局安装：\`npm install -g @moonshot-ai/kimi-code\`。',
  notes: 'Kimi CLI 安装后通常需要配置 API Key，请明确告知用户在安装完成后按官方文档配置。',
});

export const INSTALL_CODEX_ACTION_KEY = 'codex';
export const INSTALL_CODEX_PROMPT = buildInstallPrompt({
  displayName: 'Codex',
  binaryName: 'codex',
  verifyArgs: '--version',
  // 官方: https://github.com/openai/codex
  // npm: @openai/codex
  // brew cask: brew install --cask codex
  macos: '优先用 npm 全局安装：\`npm install -g @openai/codex\`（官方推荐，需 Node.js 18+）；也可用 Homebrew cask：\`brew install --cask codex\`。',
  linux: '优先用 npm 全局安装：\`npm install -g @openai/codex\`（需 Node.js 18+）。',
  windows: '优先用 npm 全局安装：\`npm install -g @openai/codex\`（需 Node.js 18+）。',
  notes: 'Codex CLI 安装后首次运行需要登录 OpenAI 账号（\`codex login\`），请明确告知用户这一步必须手动完成。',
});

export const INSTALL_MOBILECODER_ACTION_KEY = 'mobilecoder';
export const INSTALL_MOBILECODER_PROMPT = buildInstallPrompt({
  displayName: 'MobileCoder',
  binaryName: 'mobile',
  verifyArgs: '--version',
  macos: '优先按 MobileCoder 官网安装脚本执行；或尝试 \`npm install -g mobilecoder-cli\`（如官方提供 npm 包）；Homebrew 作为备选。',
  linux: '优先按 MobileCoder 官网安装脚本执行；或尝试 \`npm install -g mobilecoder-cli\`（如官方提供 npm 包）。',
  windows: '优先按 MobileCoder 官网安装脚本/PowerShell 执行；或从官网下载安装包。',
});

export const INSTALL_CODEWHALE_ACTION_KEY = 'codewhale';
export const INSTALL_CODEWHALE_PROMPT = buildInstallPrompt({
  displayName: 'CodeWhale',
  binaryName: 'codewhale',
  verifyArgs: '--version',
  macos: '先用 npm 全局安装（下载预编译 Rust 二进制）：\`npm install -g codewhale\`；Homebrew 作为备选：\`brew tap Hmbown/deepseek-tui && brew install deepseek-tui\`。',
  linux: '优先用 npm 全局安装（下载预编译 Rust 二进制）：\`npm install -g codewhale\`；或按 GitHub 官方脚本安装。',
  windows: '优先用 npm 全局安装（下载预编译 Rust 二进制）：\`npm install -g codewhale\`；或从官方 GitHub Releases 下载 exe。',
});

export const INSTALL_PI_ACTION_KEY = 'pi';
export const INSTALL_PI_PROMPT = buildInstallPrompt({
  displayName: 'Pi',
  binaryName: 'pi',
  verifyArgs: '--version',
  macos: 'macOS 优先执行官方安装脚本：\`curl -fsSL https://pi.dev/install.sh | sh\`，或直接用 npm 全局安装：\`npm install -g @mariozechner/pi-coding-agent\`；安装时建议加上 \`--ignore-scripts\` 跳过生命周期脚本。',
  linux: '优先执行官方安装脚本：\`curl -fsSL https://pi.dev/install.sh | sh\`，或直接用 npm 全局安装：\`npm install -g @mariozechner/pi-coding-agent\`。',
  windows: '优先用 npm 全局安装：\`npm install -g @mariozechner/pi-coding-agent\`；或执行官方 PowerShell 安装脚本。',
});

export const INSTALL_MIMO_ACTION_KEY = 'mimo';
export const INSTALL_MIMO_PROMPT = buildInstallPrompt({
  displayName: 'MiMo',
  binaryName: 'mimo',
  verifyArgs: '--version',
  macos: 'macOS/Linux 优先执行官方一键安装脚本：\`curl -fsSL https://mimo.xiaomi.com/install | bash\`；或用 npm 全局安装：\`npm install -g @mimo-ai/cli\`。',
  linux: '优先执行官方一键安装脚本：\`curl -fsSL https://mimo.xiaomi.com/install | bash\`；或用 npm 全局安装：\`npm install -g @mimo-ai/cli\`。',
  windows: '优先执行官方 PowerShell 安装脚本：\`powershell -ep Bypass -c "irm https://mimo.xiaomi.com/install.ps1 | iex"\`；或用 npm 全局安装：\`npm install -g @mimo-ai/cli\`。',
});

export const INSTALL_ZHANLU_ACTION_KEY = 'zhanlu';
export const INSTALL_ZHANLU_PROMPT = buildInstallPrompt({
  displayName: 'Zhanlu',
  binaryName: 'zl',
  verifyArgs: '--version',
  macos: '优先按 Zhanlu 官网安装脚本执行；或尝试 \`npm install -g zhanlu\`（如官方提供）。',
  linux: '优先按 Zhanlu 官网安装脚本执行；或尝试 \`npm install -g zhanlu\`（如官方提供）。',
  windows: '优先按 Zhanlu 官网安装脚本/PowerShell 执行；或尝试 \`npm install -g zhanlu\`。',
});

export const INSTALL_KILO_ACTION_KEY = 'kilo';
export const INSTALL_KILO_PROMPT = buildInstallPrompt({
  displayName: 'Kilo',
  binaryName: 'kilo',
  verifyArgs: '--version',
  // 官方: https://kilo.ai/cli
  // npm: @kilocode/cli
  // brew: Kilo-Org/tap/kilo
  // curl: https://kilo.ai/cli/install | bash
  macos: '优先用 npm 全局安装：\`npm install -g @kilocode/cli\`；或执行官方安装脚本：\`curl -fsSL https://kilo.ai/cli/install | bash\`。',
  linux: '优先用 npm 全局安装：\`npm install -g @kilocode/cli\`；或执行官方安装脚本：\`curl -fsSL https://kilo.ai/cli/install | bash\`；Arch Linux：\`paru -S kilo-bin\`。',
  windows: '优先用 npm 全局安装：\`npm install -g @kilocode/cli\`；或从 https://kilo.ai/cli 下载安装包。',
});

/**
 * 执行器名 → 安装提示词的查找表。
 * 使用 Record 而不是 switch，便于单元测试遍历 keys。
 */
const EXECUTOR_INSTALL_PROMPTS: Record<string, { actionKey: string; prompt: string }> = {
  [INSTALL_CLAUCODE_ACTION_KEY]: { actionKey: INSTALL_CLAUCODE_ACTION_KEY, prompt: INSTALL_CLAUCODE_PROMPT },
  [INSTALL_CODEBUDDY_ACTION_KEY]: { actionKey: INSTALL_CODEBUDDY_ACTION_KEY, prompt: INSTALL_CODEBUDDY_PROMPT },
  [INSTALL_OPENCODE_ACTION_KEY]: { actionKey: INSTALL_OPENCODE_ACTION_KEY, prompt: INSTALL_OPENCODE_PROMPT },
  [INSTALL_ATOMCODE_ACTION_KEY]: { actionKey: INSTALL_ATOMCODE_ACTION_KEY, prompt: INSTALL_ATOMCODE_PROMPT },
  [INSTALL_HERMES_ACTION_KEY]: { actionKey: INSTALL_HERMES_ACTION_KEY, prompt: INSTALL_HERMES_PROMPT },
  [INSTALL_KIMI_ACTION_KEY]: { actionKey: INSTALL_KIMI_ACTION_KEY, prompt: INSTALL_KIMI_PROMPT },
  [INSTALL_MOBILECODER_ACTION_KEY]: { actionKey: INSTALL_MOBILECODER_ACTION_KEY, prompt: INSTALL_MOBILECODER_PROMPT },
  [INSTALL_CODEX_ACTION_KEY]: { actionKey: INSTALL_CODEX_ACTION_KEY, prompt: INSTALL_CODEX_PROMPT },
  [INSTALL_CODEWHALE_ACTION_KEY]: { actionKey: INSTALL_CODEWHALE_ACTION_KEY, prompt: INSTALL_CODEWHALE_PROMPT },
  [INSTALL_PI_ACTION_KEY]: { actionKey: INSTALL_PI_ACTION_KEY, prompt: INSTALL_PI_PROMPT },
  [INSTALL_MIMO_ACTION_KEY]: { actionKey: INSTALL_MIMO_ACTION_KEY, prompt: INSTALL_MIMO_PROMPT },
  [INSTALL_ZHANLU_ACTION_KEY]: { actionKey: INSTALL_ZHANLU_ACTION_KEY, prompt: INSTALL_ZHANLU_PROMPT },
  [INSTALL_KILO_ACTION_KEY]: { actionKey: INSTALL_KILO_ACTION_KEY, prompt: INSTALL_KILO_PROMPT },
};

/**
 * 按执行器名获取安装 prompt 与 actionKey。
 * 找不到时返回 null，让调用方决定是否展示安装按钮。
 */
export function getExecutorInstallPrompt(executorName: string): { actionKey: string; prompt: string } | null {
  return EXECUTOR_INSTALL_PROMPTS[executorName] ?? null;
}

/**
 * 获取所有支持一键安装的执行器名列表。
 * 用于单元测试与类型校验，确保 EXECUTORS 数组中的执行器都有 prompt。
 */
export function getInstallableExecutorNames(): string[] {
  return Object.keys(EXECUTOR_INSTALL_PROMPTS);
}
