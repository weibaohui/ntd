import { useEffect, useState } from 'react';
import { Button, message } from 'antd';
import { BulbOutlined } from '@ant-design/icons';
import { useActionExecution } from '@/components/ActionButton/useActionExecution';
import { PROPOSAL_ACTION_TYPE, PROPOSAL_ACTION_KEY, PROPOSAL_PROMPT, PROPOSAL_EXECUTOR } from './proposalPrompt';
import { parseProposals, type ParseResult } from './parseProposals';
import { ProposalDrawer } from './ProposalDrawer';

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
 * 点击后复用通用 action 执行管道（useActionExecution → POST /api/actions/execute），
 * 让 AI 读取当前 topic 文件并输出 YAML 建议列表；执行完成后解析结果并弹出 ProposalDrawer。
 */
export function ProposalButton({ workspaceId, slug, disabled, buttonSize = 'middle', showLabel = true }: ProposalButtonProps) {
  const filePath = buildTopicFilePath(workspaceId, slug);
  // params 随 slug 变化重建；useActionExecution 内部的 execute 闭包会捕获最新 params
  const { status, result, error, execute, reset } = useActionExecution(
    PROPOSAL_ACTION_TYPE,
    PROPOSAL_ACTION_KEY,
    PROPOSAL_PROMPT,
    { topic_file_path: filePath },
    workspaceId,
  );
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [parseResult, setParseResult] = useState<ParseResult>({ proposals: [], raw: '' });

  // 执行完成 → 解析 AI 输出 → 打开建议 Drawer；失败时 toast 即时反馈（Drawer 不会自动开，否则用户点完无响应）
  useEffect(() => {
    if (status === 'completed' && result) {
      setParseResult(parseProposals(result));
      setDrawerOpen(true);
    } else if (status === 'failed') {
      message.error(error || '生成建议失败，请重试');
    }
  }, [status, result, error]);

  // 切换 topic 时清掉上一次的结果与 Drawer，避免不同主题的建议串台
  useEffect(() => {
    reset();
    setDrawerOpen(false);
  }, [slug, reset]);

  const handleClick = () => {
    reset();
    // 显式用 pi 执行器生成建议：本 workspace 日常用 pi，且默认 claudecode 在此环境未配置
    execute(PROPOSAL_PROMPT, PROPOSAL_EXECUTOR);
  };

  const buttonDisabled = disabled || !slug;

  return (
    <>
      <Button
        icon={<BulbOutlined />}
        onClick={handleClick}
        loading={status === 'executing'}
        disabled={buttonDisabled}
        size={buttonSize}
        title={buttonDisabled ? '请先选择一个主题页面' : 'AI 分析当前主题，生成可执行 Todo 建议'}
      >
        {showLabel && '生成建议'}
      </Button>
      <ProposalDrawer
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        parseResult={parseResult}
        workspaceId={workspaceId}
        errorMessage={status === 'failed' ? (error ?? undefined) : undefined}
      />
    </>
  );
}
