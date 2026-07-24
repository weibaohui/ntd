# 执行器安装参考手册

> 本文件记录 ntd 支持的 13 个执行器的官方安装方式、官网地址与安装脚本。
> 用于 AI 安装提示词的来源验证，以及开发者在执行器版本更新时快速复查。

## 执行器安装一览表

| 执行器 | 官方名称 | 官网 / 文档 | 验证命令 | 官方安装脚本 / 命令 |
|--------|---------|------------|---------|-------------------|
| claudecode | Claude Code | [code.claude.com](https://code.claude.com/docs/en/quickstart) | `claude --version` | `curl -fsSL https://claude.ai/install.sh \| bash`<br>`irm https://claude.ai/install.ps1 \| iex` |
| codebuddy | CodeBuddy Code | [codebuddy.ai/docs/cli/installation](https://www.codebuddy.ai/docs/cli/installation) | `codebuddy --version` | `curl -fsSL https://copilot.tencent.com/cli/install.sh \| bash`<br>`irm https://copilot.tencent.com/cli/install.ps1 \| iex` |
| opencode | Opencode | [opencode.ai](https://opencode.ai) | `opencode --version` | `curl -fsSL https://opencode.ai/install \| bash` |
| atomcode | AtomCode | [atomcode.atomgit.com](https://atomcode.atomgit.com/) | `atomcode --version` | `curl -fsSL https://atomcode.atomgit.com/install.sh \| sh`<br>`irm https://atomcode.atomgit.com/install.ps1 \| iex` |
| hermes | Hermes Agent | [hermes-agent.nousresearch.com](https://hermes-agent.nousresearch.com) | `hermes --version` | `curl -fsSL https://hermes-agent.nousresearch.com/install.sh \| bash` |
| kimi | Kimi Code CLI | [code.kimi.com](https://code.kimi.com) | `kimi --version` | `curl -fsSL https://code.kimi.com/kimi-code/install.sh \| bash`<br>`irm https://code.kimi.com/kimi-code/install.ps1 \| iex` |
| codex | Codex CLI | [github.com/openai/codex](https://github.com/openai/codex) | `codex --version` | `npm install -g @openai/codex` |
| codewhale | CodeWhale | [github.com/Hmbown/CodeWhale](https://github.com/Hmbown/CodeWhale) | `codewhale --version` | `npm install -g codewhale` |
| pi | Pi | [pi.dev](https://pi.dev) | `pi --version` | `curl -fsSL https://pi.dev/install.sh \| sh` |
| mimo | MiMo Code | [github.com/XiaomiMiMo/MiMo-Code](https://github.com/XiaomiMiMo/MiMo-Code) | `mimo --version` | `curl -fsSL https://mimo.xiaomi.com/install \| bash`<br>`powershell -ep Bypass -c "irm https://mimo.xiaomi.com/install.ps1 \| iex"` |
| kilo | Kilo Code CLI | [kilo.ai/cli](https://kilo.ai/cli)<br>[github.com/Kilo-Org/kilocode](https://github.com/Kilo-Org/kilocode) | `kilo --version` | `npm install -g @kilocode/cli`<br>`curl -fsSL https://kilo.ai/cli/install \| bash` |
| zhanlu | 湛卢 | 未找到公开 CLI 文档 ⚠️ | `zl --version` | 按官方文档执行 |
| mobilecoder | MobileCoder | 未找到公开 CLI 文档 ⚠️ | `mobile --version` | 按官方文档执行 |

## 各平台通用安装命令

### macOS

| 执行器 | 推荐方式 | 命令 |
|--------|---------|------|
| Claude Code | 官方脚本（原生，自动更新） | `curl -fsSL https://claude.ai/install.sh \| bash` |
| Claude Code | npm（需 Node.js 18+） | `npm install -g @anthropic-ai/claude-code` |
| CodeBuddy | 官方脚本（无需预装 Node.js） | `curl -fsSL https://copilot.tencent.com/cli/install.sh \| bash` |
| CodeBuddy | npm（需 Node.js 18.20+） | `npm install -g @tencent-ai/codebuddy-code` |
| Opencode | 官方脚本 | `curl -fsSL https://opencode.ai/install \| bash` |
| Opencode | npm（旧版） | `npm install -g opencode-ai` |
| AtomCode | 官方脚本（单文件二进制） | `curl -fsSL https://atomcode.atomgit.com/install.sh \| sh` |
| AtomCode | Cargo（需 Rust 1.80+） | `cargo install atomcode` |
| Hermes | 官方脚本 | `curl -fsSL https://hermes-agent.nousresearch.com/install.sh \| bash` |
| Hermes | npm | `npm install -g hermes-git` |
| Kimi | 官方脚本 | `curl -fsSL https://code.kimi.com/kimi-code/install.sh \| bash` |
| Kimi | npm | `npm install -g @moonshot-ai/kimi-code` |
| Codex | npm | `npm install -g @openai/codex` |
| Codex | Homebrew cask | `brew install --cask codex` |
| CodeWhale | npm（下载预编译二进制） | `npm install -g codewhale` |
| CodeWhale | Homebrew | `brew tap Hmbown/deepseek-tui && brew install deepseek-tui` |
| Pi | 官方脚本 | `curl -fsSL https://pi.dev/install.sh \| sh` |
| Pi | npm | `npm install -g @mariozechner/pi-coding-agent` |
| MiMo | 官方脚本 | `curl -fsSL https://mimo.xiaomi.com/install \| bash` |
| MiMo | npm | `npm install -g @mimo-ai/cli` |
| Kilo | npm | `npm install -g @kilocode/cli` |
| Kilo | 官方脚本 | `curl -fsSL https://kilo.ai/cli/install \| bash` |

### Linux

Linux 安装命令与 macOS 基本一致（缺少 Homebrew），优先使用官方 curl 安装脚本或 npm。

### Windows

| 执行器 | 推荐方式 | 命令 |
|--------|---------|------|
| Claude Code | PowerShell 官方脚本（原生，自动更新） | `irm https://claude.ai/install.ps1 \| iex` |
| Claude Code | npm | `npm install -g @anthropic-ai/claude-code` |
| CodeBuddy | PowerShell 官方脚本 | `irm https://copilot.tencent.com/cli/install.ps1 \| iex` |
| CodeBuddy | npm | `npm install -g @tencent-ai/codebuddy-code` |
| Opencode | scoop | `scoop install opencode` |
| Opencode | npm（旧版） | `npm install -g opencode-ai` |
| AtomCode | PowerShell 官方脚本 | `irm https://atomcode.atomgit.com/install.ps1 \| iex` |
| Kimi | PowerShell 官方脚本 | `irm https://code.kimi.com/kimi-code/install.ps1 \| iex` |
| Kimi | npm | `npm install -g @moonshot-ai/kimi-code` |
| Codex | npm | `npm install -g @openai/codex` |
| CodeWhale | npm（下载预编译二进制） | `npm install -g codewhale` |
| Pi | npm | `npm install -g @mariozechner/pi-coding-agent` |
| MiMo | PowerShell 官方脚本 | `powershell -ep Bypass -c "irm https://mimo.xiaomi.com/install.ps1 \| iex"` |
| MiMo | npm | `npm install -g @mimo-ai/cli` |
| Kilo | npm | `npm install -g @kilocode/cli` |

## 安装后验证命令

所有执行器安装完成后，运行以下命令验证可用性：

```bash
<binary> --version
```

各执行器的 binary 名称：

| 执行器 | binary 名 |
|--------|-----------|
| claudecode | `claude` |
| codebuddy | `codebuddy` |
| opencode | `opencode` |
| atomcode | `atomcode` |
| hermes | `hermes` |
| kimi | `kimi` |
| codex | `codex` |
| codewhale | `codewhale` |
| pi | `pi` |
| mimo | `mimo` |
| kilo | `kilo` |
| zhanlu | `zl` |
| mobilecoder | `mobile` |

## 更新记录

| 日期 | 更新内容 |
|------|---------|
| 2026-07-24 | 初始版本，基于各执行器官网 / GitHub 仓库的安装文档整理 |
