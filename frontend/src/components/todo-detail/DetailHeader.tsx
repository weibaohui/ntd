import { Button, Tag, Badge, App } from 'antd';
import { PlayCircleOutlined, ThunderboltOutlined } from '@ant-design/icons';
import { StatusPicker } from '@/components/StatusPicker';
import { ExecutorBadge } from '@/components/ExecutorBadge';
// 引入 ExpertBadge：在执行器徽章后展示关联的专家/团队信息。
import { ExpertBadge } from '@/components/ExpertBadge';
import { PromptDisplay } from './PromptDisplay';
import { InlineTokenStats } from './InlineTokenStats';
import { ProgressWidget } from './ProgressWidget';
import { formatLocalDateTime } from '@/utils/datetime';
import { CopyButton } from '@/components/CopyButton';
// 按钮组（优化标题/编辑/删除）抽到 TodoDetailActions，与 TodoDetailPage 的 titleSuffix 共用。
import { TodoDetailActions } from './TodoDetailActions';
import type { ExecutionSummary, ExecutionRecord } from '@/types';
import type { Todo } from '@/types';

export function DetailHeader({
  selectedTodo, executor, isExecuting, summary, currentTodoProgress,
  records, onDelete, onTodoDrawerOpen, onOpenExecuteWithArgs, onExecute, onStatusChange,
  onTitleUpdate,
  hideTitleRow = false,
}: {
  selectedTodo: Todo;
  executor: string;
  isExecuting: boolean;
  summary: ExecutionSummary | null;
  currentTodoProgress: any;
  records: ExecutionRecord[];
  onDelete: () => Promise<void>;
  onTodoDrawerOpen: () => void;
  onOpenExecuteWithArgs: () => void;
  onExecute: () => Promise<void>;
  onStatusChange: (status: string) => Promise<void>;
  onTitleUpdate?: (newTitle: string) => Promise<void>;
  hideTitleRow?: boolean;
}) {
  const { message } = App.useApp();
  const webhookUrl = `${window.location.origin}/webhook/trigger/todo/${selectedTodo.id}`;

  return (
    <>
      <div className="detail-card header-card">
        {!hideTitleRow && (
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6 }}>
            <StatusPicker value={selectedTodo.status} onChange={onStatusChange} disabled={isExecuting} />
            <h2 className="card-title" style={{ margin: 0, flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{selectedTodo.title}</h2>
            <TodoDetailActions
              todo={selectedTodo}
              onDelete={onDelete}
              onEdit={onTodoDrawerOpen}
              onTitleUpdate={onTitleUpdate}
            />
          </div>
        )}
        <div style={{ display: 'flex', alignItems: 'flex-start', gap: 10, flexWrap: 'wrap' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
            <ExecutorBadge executor={executor} />
            {/* 当 Todo 关联了专家时，在执行器徽章后展示专家徽章；未关联则不渲染。 */}
            {selectedTodo.expert_name && (
              <ExpertBadge expertName={selectedTodo.expert_name} />
            )}
            {selectedTodo.scheduler_enabled ? (
              <Tag color="var(--color-primary)" style={{ fontWeight: 600, fontSize: 11 }}>
                调度: {selectedTodo.scheduler_config}
              </Tag>
            ) : (
              <Tag style={{ fontWeight: 600, fontSize: 11, color: 'var(--color-text-tertiary)', borderColor: 'var(--color-border)' }}>
                调度: 关闭
              </Tag>
            )}
            {selectedTodo.webhook_enabled && (
              <>
                <Tag color="geekblue" style={{ fontWeight: 600, fontSize: 11 }}>
                  Webhook: 已启用
                </Tag>
                <Tag
                  style={{
                    fontWeight: 500,
                    fontSize: 11,
                    maxWidth: 420,
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                    cursor: 'pointer',
                  }}
                  onClick={() => message.success('已复制 Webhook 地址')}
                >
                  {webhookUrl}
                </Tag>
                <CopyButton
                  type="text"
                  size="small"
                  text={webhookUrl}
                  onCopy={() => message.success('已复制 Webhook 地址')}
                  className="icon-btn"
                  aria-label="复制 Webhook 地址"
                />
              </>
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
        {(selectedTodo.acceptance_criteria || selectedTodo.workspace_path) && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 4, marginTop: 2, marginBottom: 8, fontSize: 12, color: 'var(--color-text-secondary)' }}>
            {selectedTodo.acceptance_criteria && (
              <div>
                <span style={{ fontWeight: 600 }}>验收标准：</span>
                <span>{selectedTodo.acceptance_criteria}</span>
              </div>
            )}
            {selectedTodo.workspace_path && (
              <div>
                <span style={{ fontWeight: 600 }}>工作区目录：</span>
                <span>{selectedTodo.workspace_path}</span>
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
