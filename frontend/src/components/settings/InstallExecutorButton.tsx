import { DownloadOutlined } from '@ant-design/icons';
import { ActionButton } from '@/components/ActionButton';
import { INSTALL_EXECUTOR_ACTION_TYPE } from './executorInstallPrompts';

interface InstallExecutorButtonProps {
  /** 执行器内部名，用于 actionKey 与回调识别 */
  executorName: string;
  /** UI 展示名，用于面板标题与描述 */
  displayName: string;
  /** 安装 prompt 模板 */
  prompt: string;
  /** 按钮类型 */
  buttonType?: 'primary' | 'default' | 'link' | 'text';
  /** 按钮尺寸 */
  buttonSize?: 'small' | 'middle' | 'large';
  /** 是否显示文字 */
  showLabel?: boolean;
  /** 是否禁用 */
  disabled?: boolean;
  /** 安装动作完成后回调（通常用来重新探测执行器并刷新状态） */
  onInstalled?: () => void | Promise<void>;
}

/**
 * 「一键安装执行器」触发按钮。
 *
 * 复用通用 ActionButton 承载完整交互：点击 → 弹 Drawer 看 prompt / 选执行器 / 选 workspace →
 * 执行 → 实时看执行器日志 → 完成后用户点「应用」触发 onInstalled 重探。
 *
 * 不传默认 executor：让用户在 Drawer 里挑一个本机已装的执行器来跑安装命令；
 * 能打开本应用就必然已配置至少一个执行器，因此不存在「没有执行器可装」的死结。
 */
export function InstallExecutorButton({
  executorName,
  displayName,
  prompt,
  onInstalled,
  buttonType = 'default',
  buttonSize = 'small',
  showLabel = true,
  disabled,
}: InstallExecutorButtonProps) {
  return (
    // data-testid 供 Playwright 测试定位，格式 executor-install-button-{name}
    <span data-testid={`executor-install-button-${executorName}`}>
      <ActionButton
        actionType={INSTALL_EXECUTOR_ACTION_TYPE}
        actionKey={executorName}
        prompt={prompt}
        // 不需要前端参数：操作系统由执行器在目标机器上自行检测
        params={{}}
        // 不传 executor：让用户在 Drawer 里选本机已装的执行器
        buttonType={buttonType}
        icon={<DownloadOutlined />}
        buttonSize={buttonSize}
        showLabel={showLabel}
        disabled={disabled}
        panelTitle={`安装 ${displayName}`}
        panelDescription={`AI 将检测你的操作系统并自动安装 ${displayName}，全程可在面板看到执行日志`}
        // 完成态默认渲染「结果原文 + 应用/拒绝」即够用；用户点「应用」触发重探，状态由红转绿
        onApply={async () => {
          await onInstalled?.();
        }}
      >
        安装
      </ActionButton>
    </span>
  );
}
