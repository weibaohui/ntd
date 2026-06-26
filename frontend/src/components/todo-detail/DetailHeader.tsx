import { Button, Tag, Badge, Popconfirm, App } from 'antd';
import { PlayCircleOutlined, ThunderboltOutlined, EditOutlined, DeleteOutlined, CopyOutlined } from '@ant-design/icons';
import { StatusPicker } from '@/components/StatusPicker';
import { ExecutorBadge } from '@/components/ExecutorBadge';
import { PromptDisplay } from './PromptDisplay';
import { InlineTokenStats } from './InlineTokenStats';
import { ProgressWidget } from './ProgressWidget';
import { formatLocalDateTime } from '@/utils/datetime';
import { copyToClipboard } from '@/utils/clipboard';
import type { ExecutionSummary, ExecutionRecord } from '@/types';
import type { Todo } from '@/types';

export function DetailHeader({
  selectedTodo, executor, isExecuting, summary, currentTodoProgress,
  records, onDelete, onTodoDrawerOpen, onOpenExecuteWithArgs, onExecute, onStatusChange,
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
}) {
  const { message } = App.useApp();
  const webhookUrl = `${window.location.origin}/webhook/trigger/todo/${selectedTodo.id}`;

  return (
    <>
      <div className="detail-card header-card">
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6 }}>
          <StatusPicker value={selectedTodo.status} onChange={onStatusChange} disabled={isExecuting} />
          <h2 className="card-title" style={{ margin: 0, flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{selectedTodo.title}</h2>
          <div style={{ display: 'flex', gap: 4, flexShrink: 0 }}>
            <Button type="text" icon={<EditOutlined />} onClick={onTodoDrawerOpen} className="icon-btn" aria-label="编辑任务" />
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
                  onClick={async () => {
                    const ok = await copyToClipboard(webhookUrl);
                    if (ok) message.success('已复制 Webhook 地址');
                    else message.error('复制失败');
                  }}
                >
                  {webhookUrl}
                </Tag>
                <Button
                  type="text"
                  size="small"
                  icon={<CopyOutlined />}
                  className="icon-btn"
                  aria-label="复制 Webhook 地址"
                  onClick={async () => {
                    const ok = await copyToClipboard(webhookUrl);
                    if (ok) message.success('已复制 Webhook 地址');
                    else message.error('复制失败');
                  }}
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
        {(selectedTodo.acceptance_criteria || selectedTodo.workspace) && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 4, marginTop: 2, marginBottom: 8, fontSize: 12, color: 'var(--color-text-secondary)' }}>
            {selectedTodo.acceptance_criteria && (
              <div>
                <span style={{ fontWeight: 600 }}>验收标准：</span>
                <span>{selectedTodo.acceptance_criteria}</span>
              </div>
            )}
            {selectedTodo.workspace && (
              <div>
                <span style={{ fontWeight: 600 }}>工作区目录：</span>
                <span>{selectedTodo.workspace}</span>
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
