// Worktree 路径展示组件：
// - 未启用 worktree 时（`worktree_path` 为 null 或空串）整段不渲染。
// - 路径过长时只展示尾部，tooltip 显示完整路径。
// - 点击整行复制完整路径。

import { Tooltip, message as antdMessage } from 'antd';
import { BranchesOutlined } from '@ant-design/icons';
import { CopyButton } from '@/components/CopyButton';

interface WorktreePathDisplayProps {
  worktreePath: string | null;
}

/**
 * 与 RecordDetailView 中的 WorktreePathDisplay 保持一致：
 * - 未启用 worktree 时（`worktree_path` 为 null 或空串）整段不渲染。
 * - 路径过长时只展示尾部，tooltip 显示完整路径。
 * - 点击整行复制完整路径，HTTP 环境自动 fallback 到 execCommand。
 *
 * 与 desktop 详情页的展示语义完全一致，便于用户在任何入口都能定位 worktree 目录。
 */
export function WorktreePathDisplay({ worktreePath }: WorktreePathDisplayProps) {
  if (!worktreePath) return null;

  // 路径过长时只展示尾部，鼠标悬停 tooltip 显示完整路径
  const displayPath = worktreePath.length > 60
    ? `…${worktreePath.slice(-59)}`
    : worktreePath;

  return (
    <Tooltip title={worktreePath}>
      <CopyButton
        type="text"
        text={worktreePath}
        onCopy={() => antdMessage.success('已复制 worktree 路径')}
        style={{
          fontSize: 11,
          color: 'var(--color-text-quaternary)',
          marginBottom: 12,
          fontFamily: 'var(--font-mono)',
          padding: 0,
          height: 'auto',
          lineHeight: 1.6,
          display: 'inline-flex',
          alignItems: 'center',
          justifyContent: 'flex-start',
          gap: 6,
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          whiteSpace: 'nowrap',
          width: '100%',
          textAlign: 'left',
        }}
        icon={<BranchesOutlined style={{ fontSize: 11, color: 'var(--color-primary)' }} />}
      >
        <span>Worktree: {displayPath}</span>
      </CopyButton>
    </Tooltip>
  );
}
