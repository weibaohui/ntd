import { BulbOutlined } from '@ant-design/icons';
import { ActionButton } from '@/components/ActionButton';
import { PROPOSAL_ACTION_TYPE, PROPOSAL_ACTION_KEY, PROPOSAL_PROMPT, PROPOSAL_EXECUTOR } from './proposalPrompt';
import { ProposalCompleted } from './ProposalCompleted';

interface ProposalButtonProps {
  workspaceId: number;
  /** 当前选中的 wiki 页面 slug（topic 文件名，不含 .md 扩展名） */
  slug: string;
  /** 外部额外禁用条件（例如当前页面不是 topic 类型时由父组件禁用） */
  disabled?: boolean;
  /** 按钮尺寸，移动端工具栏传 'small' 与其他图标按钮对齐 */
  buttonSize?: 'small' | 'middle' | 'large';
  /** 是否显示「生成建议」文字；移动端空间紧张可传 false 只留图标 */
  showLabel?: boolean;
}

/** 拼 wiki topic 文件的「家目录相对路径」，交给 AI 的 cat 命令读取（~ 由 shell 展开）。 */
function buildTopicFilePath(workspaceId: number, slug: string): string {
  return `~/.ntd/workspace/${workspaceId}/wiki/topics/${slug}.md`;
}

/**
 * 黑板「生成 Todo 建议」触发按钮。
 *
 * 复用通用 ActionButton 承载完整交互：点击 → 弹 Drawer 看 prompt / 选执行器 / 看参数预览 →
 * 执行 → 完成后由 completedView 插槽渲染 ProposalCompleted（解析 YAML 建议列表 + 批量创建）。
 * 与 ActionButton 的唯一差异就是完成态：不是「结果原文 + 应用/拒绝」，而是「建议列表 + 批量创建」。
 *
 * key=slug：切换主题时整体重建，确保 ActionButton 内部 prompt/执行器/结果状态不串台。
 */
export function ProposalButton({ workspaceId, slug, disabled, buttonSize = 'middle', showLabel = true }: ProposalButtonProps) {
  const filePath = buildTopicFilePath(workspaceId, slug);
  const buttonDisabled = disabled || !slug;

  return (
    <ActionButton
      key={slug}
      actionType={PROPOSAL_ACTION_TYPE}
      actionKey={PROPOSAL_ACTION_KEY}
      prompt={PROPOSAL_PROMPT}
      params={{ topic_file_path: filePath }}
      workspaceId={workspaceId}
      // 默认执行器 pi：本 workspace 日常用 pi，默认 claudecode 在此环境未配置；
      // 用户仍可在 Drawer 里临时改执行器
      executor={PROPOSAL_EXECUTOR}
      buttonType="default"
      icon={<BulbOutlined />}
      buttonSize={buttonSize}
      showLabel={showLabel}
      disabled={buttonDisabled}
      panelTitle="生成 Todo 建议"
      panelDescription="AI 将读取当前主题，识别待解决问题/风险/下一步建议，拆成可执行 Todo 供勾选批量创建"
      // 完成态插槽：把 AI 输出交给 ProposalCompleted 解析为建议列表
      completedView={({ result, close }) => (
        <ProposalCompleted result={result} workspaceId={workspaceId} onClose={close} />
      )}
    >
      生成建议
    </ActionButton>
  );
}
