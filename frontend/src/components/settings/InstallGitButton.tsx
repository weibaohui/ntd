import { DownloadOutlined } from '@ant-design/icons';
import { ActionButton } from '@/components/ActionButton';
import { INSTALL_GIT_ACTION_TYPE, INSTALL_GIT_ACTION_KEY, INSTALL_GIT_PROMPT } from './installGitPrompt';

interface InstallGitButtonProps {
  /** 安装动作完成后回调（通常用来重新探测 git 是否可用、刷新状态展示）。
   *  ActionButton 的 onApply 在用户点「应用」时触发；装 git 没有需要「应用回填」的产物，
   *  复用这个时机来重跑 is_git_available 最自然——用户看到版本号后点应用，状态即转绿。 */
  onInstalled?: () => void | Promise<void>;
  /** 按钮类型：横幅里用 primary 强引导，状态弹窗里用 default 次级 */
  buttonType?: 'primary' | 'default' | 'link' | 'text';
  /** 按钮尺寸，与周围按钮对齐 */
  buttonSize?: 'small' | 'middle' | 'large';
  /** 是否显示「安装 Git」文字，空间紧张可传 false 只留图标 */
  showLabel?: boolean;
  /** 外部额外禁用条件 */
  disabled?: boolean;
}

/**
 * 「一键安装 Git」触发按钮。
 *
 * 复用通用 ActionButton 承载完整交互：点击 → 弹 Drawer 看 prompt / 选执行器 / 选 workspace →
 * 执行 → 实时看执行器日志（brew/apt/winget 的输出过程）→ 完成后用户点「应用」触发 onInstalled 重探。
 *
 * 不指定 executor：让用户在 Drawer 里挑一个本机已装的执行器（Claude Code 等）来跑安装命令；
 * 能打开本应用就必然已配置至少一个执行器，因此不存在「没有执行器可装 git」的死结。
 * worktree 也不构成障碍：install_git 这条 action todo 一般不在已启用 worktree 的项目目录下执行；
 * 即便命中，create_worktree 失败也会优雅降级回原 workspace（见 executor_service/worktree.rs 的 fallback）。
 */
export function InstallGitButton({
  onInstalled,
  buttonType = 'default',
  buttonSize = 'middle',
  showLabel = true,
  disabled,
}: InstallGitButtonProps) {
  return (
    <ActionButton
      actionType={INSTALL_GIT_ACTION_TYPE}
      actionKey={INSTALL_GIT_ACTION_KEY}
      prompt={INSTALL_GIT_PROMPT}
      // 不需要前端参数：操作系统由执行器在目标机器上自行检测（见 installGitPrompt 注释）
      params={{}}
      // 不传 executor：让用户在 Drawer 里选本机已装的执行器；后端 find_or_create_todo
      // 会取第一个可用 workspace 兜底，用户也可在 Drawer 的 WorkspaceSwitcher 里改。
      buttonType={buttonType}
      icon={<DownloadOutlined />}
      buttonSize={buttonSize}
      showLabel={showLabel}
      disabled={disabled}
      panelTitle="安装 Git"
      panelDescription="AI 将检测你的操作系统并自动安装 Git（macOS 用 Homebrew / Linux 用 apt、dnf / Windows 用 winget），全程可在面板看到执行日志"
      // 完成态默认渲染「结果原文 + 应用/拒绝」即够用：执行器会汇报 git --version 输出；
      // 用户点「应用」触发重探，状态由红转绿。无需自定义 completedView。
      onApply={async () => {
        await onInstalled?.();
      }}
    >
      安装 Git
    </ActionButton>
  );
}
