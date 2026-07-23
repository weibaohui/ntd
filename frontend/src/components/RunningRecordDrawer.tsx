import { useState, useEffect, useCallback } from 'react';
import { Drawer, Tag, Button, Popconfirm, Space, Popover, InputNumber, Collapse, Spin, message } from 'antd';
import {
  StarOutlined,
  StarFilled,
  StopOutlined,
  LinkOutlined,
  RightOutlined,
} from '@ant-design/icons';
import { CopyButton } from '@/components/CopyButton';
import XMarkdown from '@ant-design/x-markdown';
import { ExecutorBadge } from '@/components/ExecutorBadge';
import { useApp } from '@/hooks/useApp';
import { useViewState } from '@/hooks/useViewState';
import { formatLocalDateTime } from '@/utils/datetime';
import { formatTokens, formatDuration, elapsedSeconds } from '@/utils/format';
import { LOG_TYPE_COLORS, STATUS_COLORS, REVIEW_RESULT_COLORS } from '@/constants';
import * as db from '@/utils/database';
import type { ExecutionRecord, LogEntry } from '@/types';

/* ─── Helpers ─── */

const LOG_TYPE_LABELS: Record<string, string> = {
  info: 'INFO', stdout: 'OUT', stderr: 'ERR', error: 'ERROR',
  tool_use: 'TOOL', tool_call: 'CALL', tool_result: 'RESULT',
  assistant: 'AI', user: 'USER', system: 'SYS', thinking: 'THINK',
  result: 'RESULT', step_start: 'STEP', step_finish: 'STEP', tokens: 'TOKENS',
};

/* ─── Rating Control ─── */

function RatingControl({ record, onRate }: { record: ExecutionRecord; onRate: (id: number, r: number | null) => Promise<void> }) {
  const [open, setOpen] = useState(false);
  const [value, setValue] = useState<number | null>(record.rating ?? null);
  useEffect(() => { setValue(record.rating ?? null); }, [record.rating, record.id]);

  const handleSubmit = async (next: number | null) => {
    try { await onRate(record.id, next); setOpen(false); } catch {}
  };

  if (record.rating != null) {
    return (
      <Popover content={
        <div style={{ width: 200 }}>
          <div style={{ marginBottom: 8, fontSize: 12 }}>评分：{record.rating}</div>
          <Space.Compact style={{ width: '100%' }}>
            <InputNumber min={0} max={100} value={value ?? 0} onChange={v => setValue(typeof v === 'number' ? v : null)} style={{ width: '100%' }} />
            <Button onClick={() => handleSubmit(value)}>改</Button>
          </Space.Compact>
          <Button type="link" danger size="small" style={{ padding: '4px 0 0' }} onClick={() => handleSubmit(null)}>清除</Button>
        </div>
      } open={open} onOpenChange={setOpen} placement="bottomRight">
        <Button size="small" icon={<StarFilled style={{ color: '#fadb14' }} />} onClick={() => setOpen(o => !o)}>
          {record.rating}
        </Button>
      </Popover>
    );
  }

  return (
    <Popover content={
      <div style={{ width: 200 }}>
        <div style={{ marginBottom: 8, fontSize: 12 }}>评分（0-100）</div>
        <Space.Compact style={{ width: '100%' }}>
          <InputNumber min={0} max={100} value={value} onChange={v => setValue(typeof v === 'number' ? v : null)} placeholder="0-100" style={{ width: '100%' }} />
          <Button type="primary" onClick={() => handleSubmit(value)}>评分</Button>
        </Space.Compact>
      </div>
    } open={open} onOpenChange={setOpen} placement="bottomRight">
      <Button size="small" icon={<StarOutlined />} onClick={() => setOpen(o => !o)}>评分</Button>
    </Popover>
  );
}

/* ─── Review Status Badge ─── */

function ReviewStatusBadge({ status }: { status?: string | null }) {
  if (!status) return null;
  const map: Record<string, { color: string; text: string }> = {
    pending: { color: REVIEW_RESULT_COLORS.pending, text: '评审中' },
    success: { color: REVIEW_RESULT_COLORS.success, text: '评审通过' },
    failed: { color: REVIEW_RESULT_COLORS.failed, text: '评审失败' },
    interrupted: { color: REVIEW_RESULT_COLORS.interrupted, text: '评审中断' },
    skipped: { color: '#6b7280', text: '评审跳过' },
  };
  const info = map[status];
  if (!info) return null;
  return <Tag color={info.color}>{info.text}</Tag>;
}

/* ─── Log View ─── */

function LogView({ record, workspaceId }: { record: ExecutionRecord; workspaceId: number }) {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (record.status === 'running') return;
    setLoading(true);
    db.getExecutionLogs(workspaceId, record.id, 1, 500)
      .then(r => setLogs(r.logs))
      .catch(() => setLogs([]))
      .finally(() => setLoading(false));
  }, [record.id, record.status]);

  if (loading) return <Spin size="small" style={{ display: 'block', margin: '16px auto' }} />;
  if (logs.length === 0) return <div style={{ color: 'var(--color-text-tertiary)', fontSize: 12, padding: 16 }}>暂无日志</div>;

  return (
    <div style={{ maxHeight: 400, overflow: 'auto', fontSize: 12, fontFamily: 'var(--font-mono)' }}>
      {logs.map((log, i) => (
        <div key={i} style={{ display: 'flex', gap: 8, padding: '2px 0', borderBottom: '1px solid var(--color-border-light)' }}>
          <span style={{ color: LOG_TYPE_COLORS[log.type] || '#6b7280', minWidth: 52, textAlign: 'right', flexShrink: 0 }}>
            {LOG_TYPE_LABELS[log.type] || log.type}
          </span>
          <span style={{ color: 'var(--color-text-secondary)', whiteSpace: 'pre-wrap', wordBreak: 'break-all', flex: 1 }}>
            {log.content?.slice(0, 2000)}
          </span>
        </div>
      ))}
    </div>
  );
}

/* ─── Main Drawer ─── */

export interface RunningRecordDrawerProps {
  record: ExecutionRecord | null;
  open: boolean;
  onClose: () => void;
  onRefresh?: () => void;
}

export function RunningRecordDrawer({ record, open, onClose, onRefresh }: RunningRecordDrawerProps) {
  const { state } = useApp();
  const { selectTodo } = useViewState();
  const [stopping, setStopping] = useState(false);

  const todo = record ? state.todos.find(t => t.id === record.todo_id) : null;
  const isRunning = record?.status === 'running';

  const handleNavigateTodo = useCallback(() => {
    if (record) {
      selectTodo(record.todo_id);
      onClose();
    }
  }, [record, selectTodo, onClose]);

  const wsId = state.selectedWorkspace ?? 0;

  const handleStop = useCallback(async () => {
    if (!record) return;
    setStopping(true);
    try {
      await db.stopExecution(wsId, record.id);
      onRefresh?.();
    } catch {} finally {
      setStopping(false);
    }
  }, [record, wsId, onRefresh]);

  const handleRate = useCallback(async (id: number, rating: number | null) => {
    await db.rateExecutionRecord(wsId, id, rating);
    onRefresh?.();
  }, [wsId, onRefresh]);

  if (!record) return null;

  // 统一使用毫秒作为 duration 单位，与 formatDuration(ms) 签名一致。
  // 优先使用后端记录的 duration_ms，运行时中的记录则通过 started_at 实时计算。
  // elapsedSeconds 返回秒，需 * 1000 转换为毫秒。
  // 使用 ?? 而非 ||：duration_ms === 0 时是合法值（任务瞬间完成），不应回退到实时计算。
  const duration = record.usage?.duration_ms ?? (isRunning ? elapsedSeconds(record.started_at) * 1000 : null);

  return (
    <Drawer
      title={
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <span style={{
            display: 'inline-block', width: 8, height: 8, borderRadius: '50%',
            backgroundColor: isRunning ? STATUS_COLORS.running : record.status === 'success' ? STATUS_COLORS.success : STATUS_COLORS.failed,
          }} />
          <span>执行记录 #{record.id}</span>
          <Tag color={isRunning ? 'orange' : record.status === 'success' ? 'green' : 'red'}>
            {isRunning ? '运行中' : record.status === 'success' ? '成功' : '失败'}
          </Tag>
          <ReviewStatusBadge status={record.last_review_status} />
        </div>
      }
      open={open}
      onClose={onClose}
      width={560}
      extra={
        <div style={{ display: 'flex', gap: 8 }}>
          {todo && (
            <Button size="small" icon={<RightOutlined />} onClick={handleNavigateTodo}>
              查看任务
            </Button>
          )}
          {isRunning && (
            <Popconfirm title="确定停止该任务？" onConfirm={handleStop} okText="停止" cancelText="取消">
              <Button size="small" danger icon={<StopOutlined />} loading={stopping}>停止</Button>
            </Popconfirm>
          )}
        </div>
      }
    >
      {/* Todo Info */}
      {todo && (
        <div
          onClick={handleNavigateTodo}
          style={{
            padding: '10px 12px', marginBottom: 16, borderRadius: 8,
            background: 'var(--color-fill-quaternary)', cursor: 'pointer',
            border: '1px solid var(--color-border-light)',
          }}
        >
          <div style={{ fontSize: 14, fontWeight: 600, marginBottom: 4 }}>{todo.title}</div>
          {todo.prompt && (
            <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              {todo.prompt}
            </div>
          )}
        </div>
      )}

      {/* Meta */}
      <div style={{ display: 'flex', gap: 12, flexWrap: 'wrap', marginBottom: 16 }}>
        {record.executor && <ExecutorBadge executor={record.executor} />}
        {record.model && <Tag color="#3b82f6">{record.model}</Tag>}
        <Tag color={record.trigger_type === 'cron' ? STATUS_COLORS.scheduled : record.trigger_type.startsWith('hook:') ? STATUS_COLORS.hook : '#6b7280'}>
          {record.trigger_type === 'cron' ? 'Cron' : record.trigger_type.startsWith('hook:') ? 'Hook' : record.trigger_type === 'manual' ? '手动' : record.trigger_type}
        </Tag>
        {record.source_todo_id && (
          <Tag color="purple" icon={<LinkOutlined />}>
            来自 #{record.source_todo_id} {record.source_todo_title ?? ''}
          </Tag>
        )}
      </div>

      {/* Times */}
      <div style={{ display: 'flex', gap: 16, marginBottom: 16, fontSize: 13 }}>
        <div>
          <span style={{ color: 'var(--color-text-tertiary)' }}>开始：</span>
          {formatLocalDateTime(record.started_at)}
        </div>
        {record.finished_at && (
          <div>
            <span style={{ color: 'var(--color-text-tertiary)' }}>结束：</span>
            {formatLocalDateTime(record.finished_at)}
          </div>
        )}
        {duration != null && (
          <div>
            <span style={{ color: 'var(--color-text-tertiary)' }}>耗时：</span>
            <span style={{ fontWeight: 600 }}>{formatDuration(duration)}</span>
          </div>
        )}
      </div>

      {/* Usage */}
      {record.usage && (
        <div style={{ display: 'flex', gap: 16, marginBottom: 16, fontSize: 12, color: 'var(--color-text-secondary)' }}>
          <div>Input: <b>{formatTokens(record.usage.input_tokens)}</b></div>
          <div>Output: <b>{formatTokens(record.usage.output_tokens)}</b></div>
          {record.usage.total_cost_usd != null && (
            <div style={{ color: 'var(--color-warning)', fontWeight: 600 }}>${record.usage.total_cost_usd.toFixed(4)}</div>
          )}
        </div>
      )}

      {/* Execution Stats */}
      {record.execution_stats && (
        <div style={{ display: 'flex', gap: 16, marginBottom: 16, fontSize: 12, color: 'var(--color-text-tertiary)' }}>
          <div>工具调用: <b style={{ color: 'var(--color-primary)' }}>{record.execution_stats.tool_calls}</b></div>
          <div>对话轮次: <b style={{ color: 'var(--color-primary)' }}>{record.execution_stats.conversation_turns}</b></div>
          {record.execution_stats.thinking_count > 0 && (
            <div>思考: <b style={{ color: 'var(--color-primary)' }}>{record.execution_stats.thinking_count}</b></div>
          )}
        </div>
      )}

      {/* Rating + Review */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 16 }}>
        <RatingControl record={record} onRate={handleRate} />
        <ReviewStatusBadge status={record.last_review_status} />
      </div>

      {/* Result */}
      {record.result && (
        <Collapse
          defaultActiveKey={['result']}
          size="small"
          style={{ marginBottom: 16 }}
          items={[{
            key: 'result',
            label: (
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', width: '100%' }}>
                <span>执行结果</span>
                <CopyButton type="text" size="small" text={record?.result || ''} onCopy={() => message.success('已复制')} />
              </div>
            ),
            children: (
              <div style={{ maxHeight: 300, overflow: 'auto' }}>
                <XMarkdown content={record.result} />
              </div>
            ),
          }]}
        />
      )}

      {/* Logs */}
      <Collapse
        size="small"
        items={[{
          key: 'logs',
          label: '执行日志',
          children: <LogView record={record} workspaceId={wsId} />,
        }]}
      />
    </Drawer>
  );
}
