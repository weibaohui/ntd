import { Button, Tag, Badge, Popconfirm } from 'antd';
import { PlayCircleOutlined, ThunderboltOutlined, EditOutlined, DeleteOutlined, ArrowLeftOutlined, ExperimentOutlined } from '@ant-design/icons';
import { StatusPicker } from '@/components/StatusPicker';
import { ExecutorBadge } from '@/components/ExecutorBadge';
import { PromptDisplay } from './PromptDisplay';
import { InlineTokenStats } from './InlineTokenStats';
import { ProgressWidget } from './ProgressWidget';
import { formatLocalDateTime } from '@/utils/datetime';
import type { ExecutionSummary, ExecutionRecord } from '@/types';
import type { Todo } from '@/types';

export function DetailHeader({
  selectedTodo, executor, isExecuting, isMobile, summary, currentTodoProgress,
  records, onMobileBack, onDelete, onTodoDrawerOpen, onOpenExecuteWithArgs, onExecute, onStatusChange,
  onPromoteToStep, onDemoteToItem,
}: {
  selectedTodo: Todo;
  executor: string;
  isExecuting: boolean;
  isMobile: boolean;
  summary: ExecutionSummary | null;
  currentTodoProgress: any;
  records: ExecutionRecord[];
  onMobileBack: () => void;
  onDelete: () => Promise<void>;
  onTodoDrawerOpen: () => void;
  onOpenExecuteWithArgs: () => void;
  onExecute: () => Promise<void>;
  onStatusChange: (status: string) => Promise<void>;
  onPromoteToStep?: () => void;
  onDemoteToItem?: () => void;
}) {
  return (
    <>
      {isMobile && (
        <Button
          type="text"
          icon={<ArrowLeftOutlined />}
          onClick={onMobileBack}
          style={{ marginBottom: 8, marginLeft: -4 }}
        >
          返回
        </Button>
      )}
      <div className="detail-card header-card">
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6 }}>
          <StatusPicker value={selectedTodo.status} onChange={onStatusChange} disabled={isExecuting} />
          <h2 className="card-title" style={{ margin: 0, flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{selectedTodo.title}</h2>
          <div style={{ display: 'flex', gap: 4, flexShrink: 0 }}>
            <Button type="text" icon={<EditOutlined />} onClick={onTodoDrawerOpen} className="icon-btn" aria-label="编辑任务" />
            {/* 升级/降级按钮: 事项→升级为环节, 环节→降级为事项。互斥显示 */}
            {(selectedTodo.kind ?? 'item') === 'step' ? (
              <Popconfirm title="降级为事项" description="确定将此环节降级为事项？降级后不会被任何 loop 引用" onConfirm={onDemoteToItem}>
                <Button type="text" icon={<ExperimentOutlined />} className="icon-btn" aria-label="降级为事项" />
              </Popconfirm>
            ) : (
              <Popconfirm title="升级为环节" description="确定将此事提升级为环节？可在环路编排中复用" onConfirm={onPromoteToStep}>
                <Button type="text" icon={<ExperimentOutlined />} className="icon-btn" aria-label="升级为环节" />
              </Popconfirm>
            )}
            <Popconfirm title="删除任务" description="确定要删除吗？" onConfirm={onDelete}>
              <Button type="text" danger icon={<DeleteOutlined />} className="icon-btn" aria-label="删除任务" />
            </Popconfirm>
          </div>
        </div>
        <div style={{ display: 'flex', alignItems: 'flex-start', gap: 10, flexWrap: 'wrap' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
            <ExecutorBadge executor={executor} />
            {selectedTodo.scheduler_enabled ? (
              <Tag color="var(--color-primary)" style={{ fontWeight: 600, fontSize: 11 }}>
                调度: {selectedTodo.scheduler_config}
              </Tag>
            ) : (
              <Tag style={{ fontWeight: 600, fontSize: 11, color: 'var(--color-text-tertiary)', borderColor: 'var(--color-border)' }}>
                调度: 关闭
              </Tag>
            )}
            {records.length > 0 && (
              <span style={{ fontSize: 11, color: 'var(--color-text-tertiary)' }}>
                上次: {formatLocalDateTime(records[0].started_at)}
              </span>
            )}
            {selectedTodo.scheduler_next_run_at && (
              <span style={{ fontSize: 11, color: 'var(--color-success)' }}>
                下次: {formatLocalDateTime(selectedTodo.scheduler_next_run_at)}
              </span>
            )}
            {isExecuting && (
              <>
                <span style={{ color: 'var(--color-border)' }}>|</span>
                <Badge status="processing" />
                <span style={{ fontSize: 12, color: 'var(--color-primary)', fontWeight: 500 }}>执行中...</span>
              </>
            )}
          </div>
          {summary && summary.total_executions > 0 && (() => {
            const input = summary.total_input_tokens;
            const output = summary.total_output_tokens;
            const cacheRead = (summary as any).total_cache_read_tokens ?? 0;
            const cacheCreate = (summary as any).total_cache_creation_tokens ?? 0;
            const totalTokens = input + output + cacheRead + cacheCreate;
            return (
              <InlineTokenStats input={input} output={output} cacheRead={cacheRead} cacheCreate={cacheCreate} totalTokens={totalTokens} summary={summary} />
            );
          })()}
          {currentTodoProgress && (
            <div style={{ marginLeft: 'auto', flexShrink: 0 }}>
              <ProgressWidget items={currentTodoProgress} />
            </div>
          )}
        </div>
        {selectedTodo.prompt && <PromptDisplay content={selectedTodo.prompt} />}
        {(selectedTodo.acceptance_criteria) && (
          <div style={{ display: 'flex', gap: 16, flexWrap: 'wrap', marginTop: 2, marginBottom: 8, fontSize: 12, color: 'var(--color-text-secondary)' }}>
            {selectedTodo.acceptance_criteria && (
              <div style={{ flex: 1, minWidth: 200 }}>
                <span style={{ fontWeight: 600 }}>验收标准：</span>
                <span>{selectedTodo.acceptance_criteria}</span>
              </div>
            )}
          </div>
        )}
        <div style={{ display: 'flex', gap: 8 }}>
          <Button
            type="primary"
            icon={<PlayCircleOutlined />}
            onClick={onExecute}
            block
            className="btn-execute btn-execute-compact"
          >
            直接执行
          </Button>
          <Button
            type="primary"
            icon={<ThunderboltOutlined style={{ color: '#ffffff' }} />}
            onClick={onOpenExecuteWithArgs}
            block
            className="btn-execute btn-execute-compact"
          >
            带参执行
          </Button>
        </div>
      </div>
    </>
  );
}
