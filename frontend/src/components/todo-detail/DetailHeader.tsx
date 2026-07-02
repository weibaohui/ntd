import { Button, Tag, Badge, Popconfirm, App, Tooltip } from 'antd';
import { PlayCircleOutlined, ThunderboltOutlined, EditOutlined, DeleteOutlined, RocketOutlined } from '@ant-design/icons';
import { StatusPicker } from '@/components/StatusPicker';
import { ExecutorBadge } from '@/components/ExecutorBadge';
import { PromptDisplay } from './PromptDisplay';
import { InlineTokenStats } from './InlineTokenStats';
import { ProgressWidget } from './ProgressWidget';
import { formatLocalDateTime } from '@/utils/datetime';
import { CopyButton } from '@/components/CopyButton';
import { ActionButton } from '@/components/ActionButton';
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
            <div style={{ display: 'flex', gap: 4, flexShrink: 0 }}>
              {onTitleUpdate && (
                <Tooltip title="自动优化标题">
                  <ActionButton
                    actionType="title_optimize"
                    actionKey="default"
                    prompt={`你是一个标题优化专家。请根据以下信息生成更优的标题。

当前标题：{{title}}
当前 Prompt：{{prompt}}

要求：
1. 保持原意
2. 更简洁有力
3. 适合 AI Todo 应用的场景

输出格式：用 RESULT 标记包裹最终标题，不要加任何其他内容。

RESULT
优化后的标题文本
RESULT`}
                    params={{
                      title: selectedTodo.title,
                      prompt: selectedTodo.prompt || '',
                    }}
                    workspaceId={selectedTodo.workspace_id || undefined}
                    onApply={onTitleUpdate}
                    buttonType="text"
                    icon={<RocketOutlined />}
                    panelTitle="自动优化标题"
                    panelDescription="AI 将根据当前标题和 Prompt 生成更优的版本"
                  />
                </Tooltip>
              )}
              <Button type="text" icon={<EditOutlined />} onClick={onTodoDrawerOpen} className="icon-btn" aria-label="编辑任务" />
              <Popconfirm title="删除任务" description="确定要删除吗？" onConfirm={onDelete}>
                <Button type="text" icon={<DeleteOutlined />} className="icon-btn" aria-label="删除任务" />
              </Popconfirm>
            </div>
          </div>
        )}
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
