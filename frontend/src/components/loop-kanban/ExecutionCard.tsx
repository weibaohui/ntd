// LoopKanban 执行卡片组件及其子组件。

import { Button, Tag, Tooltip } from 'antd';
import { ReadOutlined, ExclamationCircleOutlined } from '@ant-design/icons';
import type { LoopExecutionWithLoopName } from './useLoopExecutions';
import { execStatusView, durationLabel, formatToken } from './helpers';

// 卡片头部：环路名称 + 状态图标 + 状态标签。
function CardHeader({ exec, view }: { exec: LoopExecutionWithLoopName; view: ReturnType<typeof execStatusView> }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 6 }}>
      {view.icon}
      <span style={{ fontWeight: 600, fontSize: 13, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
        {exec.loop_name}
      </span>
      <Tag color={view.color} style={{ margin: 0, fontSize: 10 }}>{view.label}</Tag>
    </div>
  );
}

// 触发类型行。
function CardTrigger({ exec }: { exec: LoopExecutionWithLoopName }) {
  return (
    <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 4 }}>
      触发: {exec.trigger_type}
    </div>
  );
}

// 进度与时间信息行。
function CardProgress({ exec }: { exec: LoopExecutionWithLoopName }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 11, color: 'var(--color-text-secondary)' }}>
      <Tooltip title={`开始: ${exec.started_at}`}>
        <span>{exec.started_at ? new Date(exec.started_at).toLocaleDateString() : '-'}</span>
      </Tooltip>
      <span>{exec.completed_steps}/{exec.total_steps} 环节</span>
      <span style={{ fontFamily: 'monospace', color: 'var(--color-text-tertiary)' }}>
        {durationLabel(exec.started_at, exec.finished_at)}
      </span>
    </div>
  );
}

// Token 消耗汇总行。
function CardTokenSummary({ exec }: { exec: LoopExecutionWithLoopName }) {
  const ts = exec.token_summary;
  if (!ts) return null;
  const hasTokens = ts.total_input_tokens > 0 || ts.total_output_tokens > 0 || (ts.total_cache_read_input_tokens ?? 0) > 0;
  if (!hasTokens) return null;
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 4, flexWrap: 'wrap', fontSize: 10, marginTop: 4, color: 'var(--color-text-tertiary)' }}>
      <span style={{ color: '#1677ff', fontWeight: 600 }}>输入 {formatToken(ts.total_input_tokens)}</span>
      <span style={{ color: 'var(--color-text-tertiary)' }}>/</span>
      <span style={{ color: '#52c41a', fontWeight: 600 }}>输出 {formatToken(ts.total_output_tokens)}</span>
      {ts.total_cache_read_input_tokens > 0 && (
        <>
          <span style={{ color: 'var(--color-text-tertiary)' }}>/</span>
          <span style={{ color: '#722ed1', fontWeight: 600 }}>缓存 {formatToken(ts.total_cache_read_input_tokens)}</span>
        </>
      )}
    </div>
  );
}

// 待审批徽章。
function CardApprovalBadge({ count }: { count: number }) {
  return (
    <div style={{ marginTop: 4 }}>
      <Tag color="red" style={{ fontSize: 10, fontWeight: 600 }}>
        <ExclamationCircleOutlined /> {count} 待审批
      </Tag>
    </div>
  );
}

// 执行卡片组件
interface ExecutionCardProps {
  exec: LoopExecutionWithLoopName;
  view: ReturnType<typeof execStatusView>;
  onClick?: (exec: LoopExecutionWithLoopName) => void;
  onBlackboard?: (exec: LoopExecutionWithLoopName) => void;
}

export function ExecutionCard({ exec, view, onClick, onBlackboard }: ExecutionCardProps) {
  return (
    <div
      className="loop-kanban-card"
      style={{
        borderTop: `3px solid ${view.color}`,
        background: 'var(--color-bg-elevated, #ffffff)',
        border: '1px solid var(--color-border, #e2e8f0)',
        borderRadius: 8,
        padding: '10px 12px',
        marginBottom: 8,
        cursor: 'pointer',
        transition: 'box-shadow 200ms',
      }}
      onClick={() => onClick?.(exec)}
    >
      <CardHeader exec={exec} view={view} />
      <CardTrigger exec={exec} />
      <CardProgress exec={exec} />
      <CardTokenSummary exec={exec} />
      {exec.pending_approval_count > 0 && <CardApprovalBadge count={exec.pending_approval_count} />}
      <div style={{ marginTop: 6, display: 'flex', justifyContent: 'flex-end' }}>
        <Button
          type="link"
          size="small"
          icon={<ReadOutlined />}
          onClick={(e) => {
            e.stopPropagation();
            onBlackboard?.(exec);
          }}
          style={{ fontSize: 11, padding: 0, height: 'auto', lineHeight: '20px' }}
        >
          黑板
        </Button>
      </div>
    </div>
  );
}
