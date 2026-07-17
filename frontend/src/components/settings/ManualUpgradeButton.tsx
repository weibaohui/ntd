import { Button, Space, Typography, Modal } from 'antd';
import { CloudDownloadOutlined, ReloadOutlined, ExclamationCircleFilled } from '@ant-design/icons';
import { ActionButton } from '@/components/ActionButton';
import { upgradeVersion } from '@/utils/database';
import {
  MANUAL_UPGRADE_ACTION_TYPE,
  MANUAL_UPGRADE_ACTION_KEY,
  MANUAL_UPGRADE_PROMPT,
} from './upgradeButtonPrompt';

const { Paragraph, Text } = Typography;

// 升级后触发 daemon 重启的延迟：与 AboutPanel 中"一键更新"行为保持一致，
// 5 秒足够 npm install + daemon install/start 落定。
const UPGRADE_RELOAD_DELAY_MS = 5000;

interface ManualUpgradeButtonProps {
  /** 升级按钮类型（默认 default，与「一键更新」的 primary 形成层级） */
  buttonType?: 'primary' | 'default' | 'link' | 'text';
  /** 按钮尺寸 */
  buttonSize?: 'small' | 'middle' | 'large';
  /** 是否显示按钮文字；移动端可传 false 只留图标 */
  showLabel?: boolean;
  /** 外部禁用条件（如检查更新尚未完成） */
  disabled?: boolean;
}

/**
 * 弹出确认框 → 用户确认后调 upgradeVersion() → 关闭 Drawer → 5s 后刷新页面。
 *
 * 与 AboutPanel 中"一键更新"按钮走同一条路径（/api/version/upgrade）：
 * 后端 fork 子进程 sleep 3s 后 install --force + start，主进程 exit(0)。
 */
function confirmAndRestart(closeDrawer: () => void): void {
  Modal.confirm({
    title: '立即重启 NTD 服务？',
    icon: <ExclamationCircleFilled />,
    content: (
      <Space direction="vertical" size={4}>
        <Paragraph style={{ margin: 0 }}>
          服务重启期间页面会自动刷新，正在运行的 Todo 会被强制结束。
        </Paragraph>
        <Text type="secondary">重启预计 5–10 秒完成。</Text>
      </Space>
    ),
    okText: '立即重启',
    cancelText: '稍后再说',
    onOk: async () => {
      closeDrawer();
      // 后端 fork 子进程执行 install --force + start，主进程收到响应后延迟
      // 500ms exit(0)。预留 5s 缓冲覆盖整个 daemon 重启流程。
      window.setTimeout(() => window.location.reload(), UPGRADE_RELOAD_DELAY_MS);
      try {
        await upgradeVersion();
      } catch (_e) {
        // 极端情况：后端 exit(0) 早于响应落盘导致 TCP RST，吞掉错误，
        // 由 setTimeout 兜底刷新页面，避免给用户看无意义的失败提示。
      }
    },
  });
}

/** 「npm 包升级完成」后的 UI 视图：摘要 + 结果原文 + 「立即重启 / 稍后重启」按钮。 */
function UpgradeCompletedView({
  result,
  close,
}: {
  result: string;
  close: () => void;
}) {
  return (
    <Space direction="vertical" size="middle" style={{ width: '100%' }}>
      <Paragraph style={{ margin: 0 }}>
        <Text type="success">npm 包升级已完成。</Text>
        下面点「立即重启服务」让 ntd 切到新版本；如暂时不想重启，也可关闭此面板稍后处理。
      </Paragraph>
      <ResultBlock result={result} />
      <Space>
        <Button onClick={close}>稍后重启</Button>
        <Button
          type="primary"
          icon={<ReloadOutlined />}
          onClick={() => confirmAndRestart(close)}
        >
          立即重启服务
        </Button>
      </Space>
    </Space>
  );
}

/** AI 返回的结果原文块：等宽字体、可滚动、带复制按钮。 */
function ResultBlock({ result }: { result: string }) {
  return (
    <div
      style={{
        padding: 12,
        background: 'var(--color-bg-elevated)',
        border: '1px solid var(--color-border-light)',
        borderRadius: 6,
        maxHeight: 320,
        overflow: 'auto',
      }}
    >
      <Paragraph
        copyable={{ text: result }}
        style={{ whiteSpace: 'pre-wrap', margin: 0, fontFamily: 'monospace', fontSize: 12 }}
      >
        {result}
      </Paragraph>
    </div>
  );
}

/**
 * 「手动升级（AI 一键执行）」触发按钮。
 *
 * 复用通用 ActionButton 承载完整交互：点击 → 弹 Drawer 看 prompt / 选执行器 / 选
 * workspace → 执行 → 实时看执行器日志（npm install 过程）→ 完成后由 completedView
 * 插槽渲染「立即重启服务」按钮，点击后调后端 \`/api/version/upgrade\` 触发 daemon
 * 重启，5 秒后自动刷新页面访问新版本。
 *
 * 为什么 Prompt 不让 AI 直接执行 daemon restart：
 * daemon restart 会让主进程 exit(0)，AI 子进程随之被 SIGKILL——WebSocket 断连、
 * ActionButton 的 executing 状态会卡住。把"npm 升级"和"服务重启"拆成两步：前者
 * 走 ActionButton（AI 跑），后者走 ActionButton 完成态的"立即重启"按钮（前端直接
 * 调后端 API），UI 状态机完整、用户感知清晰。
 *
 * 不指定 executor：让用户在 Drawer 里挑一个本机已装的执行器（Claude Code / Pi 等）
 * 来跑升级命令；能打开本应用就必然已配置至少一个执行器。
 * 不指定 workspaceId：升级是环境级操作，与项目无关；后端 find_or_create_todo 会取
 * 第一个可用 workspace 兜底。
 */
export function ManualUpgradeButton({
  buttonType = 'default',
  buttonSize = 'middle',
  showLabel = true,
  disabled,
}: ManualUpgradeButtonProps) {
  return (
    <ActionButton
      actionType={MANUAL_UPGRADE_ACTION_TYPE}
      actionKey={MANUAL_UPGRADE_ACTION_KEY}
      prompt={MANUAL_UPGRADE_PROMPT}
      // 不需要前端参数：升级目标由执行器在目标机器上探测（见 upgradeButtonPrompt 注释）
      params={{}}
      buttonType={buttonType}
      icon={<CloudDownloadOutlined />}
      buttonSize={buttonSize}
      showLabel={showLabel}
      disabled={disabled}
      panelTitle="手动升级（AI 一键执行）"
      panelDescription="AI 将在你的本机执行 npm 升级；完成后点「立即重启服务」让 ntd 切到新版本（页面会自动刷新）"
      // 完成态插槽：替换默认的「结果原文 + 应用/拒绝」为「升级结果摘要 + 立即重启按钮」
      completedView={({ result, close }) => <UpgradeCompletedView result={result} close={close} />}
    >
      手动升级
    </ActionButton>
  );
}
