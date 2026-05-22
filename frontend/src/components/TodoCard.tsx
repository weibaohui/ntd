import { memo } from 'react';
import { Tag, Badge, message } from 'antd';
import {
  CheckCircleOutlined,
  CloseCircleOutlined,
  ClockCircleOutlined,
  RobotOutlined,
  CopyOutlined,
  EditOutlined,
  FileProtectOutlined,
} from '@ant-design/icons';
import XMarkdown from '@ant-design/x-markdown';
import { ExecutorBadge } from './ExecutorBadge';

/* ─── Format helpers ─── */

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function formatDuration(ms: number): string {
  if (ms < 1_000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1_000).toFixed(0)}s`;
  return `${(ms / 60_000).toFixed(1)}m`;
}

/* ─── Types ─── */

export interface TodoCardProps {
  /** Core content */
  id: number;
  title: string;
  prompt: string | null;
  resultText: string | null;

  /** Status display */
  isSuccess: boolean;
  showResultSection: boolean;

  /** Meta */
  executor?: string | null;
  time: string; // already formatted relative time
  model?: string | null;

  /** Tags (already resolved) */
  tags: Array<{ id: number; name: string; color: string }>;

  /** Usage stats from execution record */
  usage?: {
    duration_ms: number | null;
    input_tokens: number;
    output_tokens: number;
    total_cost_usd: number | null;
  } | null;
  triggerType?: string;

  /** Expand state */
  promptExpanded: boolean;
  resultExpanded: boolean;

  /** Handlers */
  onTogglePrompt: () => void;
  onToggleResult: () => void;

  /** Optional: select todo handler (title click) */
  onSelectTodo?: (e: React.MouseEvent) => void;

  /** Optional: loading state for async result (KanbanBoard) */
  isLoadingResult?: boolean;

  /** Run history switching */
  runCount?: number;
  selectedRun?: number; // 0-based, default 0 (most recent)
  onSelectRun?: (index: number) => void;
  isLoadingRun?: boolean;
}

/* ─── Component ─── */

export const TodoCard = memo(function TodoCard({
  id,
  title,
  prompt,
  resultText,
  isSuccess,
  showResultSection,
  executor,
  time,
  model,
  tags,
  usage,
  triggerType,
  promptExpanded,
  resultExpanded,
  onTogglePrompt,
  onToggleResult,
  onSelectTodo,
  isLoadingResult,
  runCount,
  selectedRun = 0,
  onSelectRun,
  isLoadingRun,
}: TodoCardProps) {
  return (
    <>
      {/* ── Card Header ── */}
      <div className="kanban-card-header">
        <div className="kanban-card-top">
          <span
            className="kanban-card-title"
            onClick={onSelectTodo}
            title={title}
          >
            <span style={{ color: '#999', marginRight: 4 }}>#{id}</span>{title}
          </span>
          <span>
            {isSuccess ? (
              <CheckCircleOutlined className="kanban-status-icon kanban-status-success" />
            ) : (
              <CloseCircleOutlined className="kanban-status-icon kanban-status-failed" />
            )}
          </span>
        </div>

        {/* Meta Row */}
        <div className="kanban-card-meta-row">
          {executor && <ExecutorBadge executor={executor} />}
          <span className="kanban-card-meta-time">
            <ClockCircleOutlined /> {time}
          </span>
          {model && (
            <span className="kanban-card-meta-model">
              <RobotOutlined /> {model}
            </span>
          )}
        </div>

        {/* Tags */}
        {tags.length > 0 && (
          <div className="kanban-card-tags">
            {tags.map(tag => (
              <Tag key={tag.id} color={tag.color} className="kanban-tag-badge">
                {tag.name}
              </Tag>
            ))}
          </div>
        )}
      </div>

      {/* ── Card Body (Expandable Sections) ── */}
      <div className="kanban-card-body">
        {/* Prompt Section */}
        {prompt && (
          <div className="kanban-card-section">
            <div
              className="kanban-card-section-header kanban-section-prompt"
              onClick={e => { e.stopPropagation(); onTogglePrompt(); }}
              role="button"
              tabIndex={0}
              onKeyDown={e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); e.stopPropagation(); onTogglePrompt(); } }}
            >
              <span className="kanban-card-section-label"><EditOutlined /> Prompt</span>
              {prompt && (
                <button
                  className="kanban-copy-btn"
                  onClick={e => {
                    e.stopPropagation();
                    navigator.clipboard.writeText(prompt).then(() => message.success('已复制'));
                  }}
                  title="复制 Prompt"
                >
                  <CopyOutlined />
                </button>
              )}
              <span className="kanban-card-section-toggle">
                {promptExpanded ? '收起' : '展开'}
              </span>
            </div>
            {promptExpanded && (
              <div className="kanban-card-section-content">
                <XMarkdown content={prompt} />
              </div>
            )}
          </div>
        )}

        {/* Result Section */}
        {showResultSection && (
          <div className="kanban-card-section">
            <div
              className={`kanban-card-section-header kanban-section-result ${isSuccess ? 'result-success' : 'result-failed'}`}
              onClick={e => { e.stopPropagation(); onToggleResult(); }}
              role="button"
              tabIndex={0}
              onKeyDown={e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); e.stopPropagation(); onToggleResult(); } }}
            >
              <span className="kanban-card-section-label"><FileProtectOutlined /> 结论</span>
              {runCount != null && runCount > 1 && (
                <span className="kanban-run-tags" onClick={e => e.stopPropagation()}>
                  {Array.from({ length: Math.min(runCount, 5) }, (_, i) => (
                    <Tag
                      key={i}
                      className={`kanban-run-tag ${i === selectedRun ? 'kanban-run-tag-active' : ''}`}
                      onClick={() => onSelectRun?.(i)}
                    >
                      {i + 1}
                    </Tag>
                  ))}
                </span>
              )}
              {resultText && (
                <button
                  className="kanban-copy-btn"
                  onClick={e => {
                    e.stopPropagation();
                    navigator.clipboard.writeText(resultText).then(() => message.success('已复制'));
                  }}
                  title="复制结论"
                >
                  <CopyOutlined />
                </button>
              )}
              <span className="kanban-card-section-toggle">
                {(isLoadingResult || isLoadingRun) ? '加载中…' : (resultExpanded ? '收起' : '展开')}
              </span>
            </div>
            {resultExpanded && (
              <div className="kanban-card-section-content">
                {(isLoadingResult || isLoadingRun) ? (
                  <span className="kanban-loading-text">加载中…</span>
                ) : resultText ? (
                  <XMarkdown content={resultText} />
                ) : (
                  <span className="kanban-no-result">暂无结论</span>
                )}
              </div>
            )}
          </div>
        )}
      </div>

      {/* ── Footer — Usage stats ── */}
      {usage && (
        <div className="kanban-card-footer">
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', alignItems: 'center', fontSize: 11, color: 'var(--color-text-tertiary)' }}>
            {usage.duration_ms != null && (
              <span>{formatDuration(usage.duration_ms)}</span>
            )}
            <span>
              {formatTokens(usage.input_tokens)} + {formatTokens(usage.output_tokens)} tokens
            </span>
            {usage.total_cost_usd != null && usage.total_cost_usd > 0 && (
              <span>${usage.total_cost_usd.toFixed(4)}</span>
            )}
            {triggerType && triggerType !== 'manual' && (
              <Badge
                count={triggerType === 'scheduler' ? '定时' : triggerType}
                style={{ fontSize: 10, height: 16, lineHeight: '16px' }}
              />
            )}
          </div>
        </div>
      )}
    </>
  );
});
