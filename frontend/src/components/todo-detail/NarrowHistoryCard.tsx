import { useState, useEffect } from 'react';
import { Button, Popconfirm, Tag, Tooltip, Popover, InputNumber, Space } from 'antd';
import { MessageOutlined, FileTextOutlined, StopOutlined, CopyOutlined, LinkOutlined, StarOutlined, StarFilled } from '@ant-design/icons';
import { ExecutorBadge } from '@/components/ExecutorBadge';
import { XMarkdown } from '@ant-design/x-markdown';
import { supportsResume } from '@/types';
import { formatLocalDateTime, formatDurationSec } from '@/utils/datetime';
import * as db from '@/utils/database';
import { getElapsedSeconds, hasLogsStatic } from './helpers';
import { NarrowLogView } from './NarrowLogView';
import { getHookTriggerLabel } from '@/utils/database/hooks';
import { ReviewStatusBadge } from './RecordDetailView';
import type { ExecutionRecord, ExecutionStats, LogEntry } from '@/types';

/** Narrow mode: single history card */
export function NarrowHistoryCard({ record, viewMode, onOpenResume, onExport, onStop, onRefresh, onRate, getRunningTask, resolveStats, parseLogs, messageApi, onViewModeChange }: {
  record: ExecutionRecord;
  viewMode: 'log' | 'chat';
  onOpenResume: (r: ExecutionRecord) => void;
  onExport: (r: ExecutionRecord) => void;
  onStop: (id: number) => Promise<void>;
  onRefresh: (id: number) => Promise<void>;
  onRate: (recordId: number, rating: number | null) => Promise<void>;
  getRunningTask: (r: ExecutionRecord) => any;
  resolveStats: (r: ExecutionRecord, running: boolean) => ExecutionStats | null | undefined;
  parseLogs: (r: ExecutionRecord) => LogEntry[];
  messageApi: any;
  onViewModeChange: (mode: 'log' | 'chat') => void;
}) {
  const isRunning = record.status === 'running';
  const runningTask = isRunning ? getRunningTask(record) : null;
  const liveLogs = runningTask ? runningTask.logs : null;
  const restLogs = parseLogs(record);

  // 懒加载日志
  const [loadedLogs, setLoadedLogs] = useState<LogEntry[] | null>(null);
  useEffect(() => {
    if (restLogs.length > 0 || loadedLogs !== null) return;
    db.getExecutionLogs(record.id, 1, 200)
      .then(r => setLoadedLogs(r.logs))
      .catch(() => setLoadedLogs([]));
  }, [record.id, restLogs.length, loadedLogs]);

  const displayLogs = liveLogs && liveLogs.length > 0 ? liveLogs :
    restLogs.length > 0 ? restLogs :
    loadedLogs || [];

  return (
    <div className={`history-card history-card-${record.status}`}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8, flexWrap: 'wrap', gap: 8 }}>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
          <span style={{ fontSize: 12, color: 'var(--color-text-tertiary)' }}>
            {formatLocalDateTime(record.started_at)}
          </span>
          {record.executor && <ExecutorBadge executor={record.executor} />}
          {record.model && <Tag color="#3b82f6">{record.model}</Tag>}
          <Tag color={record.trigger_type === 'cron' ? '#8b5cf6' : record.trigger_type.startsWith('hook:') ? '#a855f7' : '#6b7280'} style={record.trigger_type.startsWith('hook:') ? { fontSize: 10, border: '1px solid #a855f7' } : { fontSize: 10 }}>
            {record.trigger_type === 'cron' ? 'Cron' : record.trigger_type.startsWith('hook:') ? 'Hook' : '手动'}
          </Tag>
          {(() => {
            const label = getHookTriggerLabel(record.trigger_type);
            if (!label || record.source_todo_id == null) return null;
            return (
              <Tag color="purple" icon={<LinkOutlined />} style={{ fontSize: 10 }}>
                被 #{record.source_todo_id} {record.source_todo_title ?? '?'} 的「{label}」hook 触发
              </Tag>
            );
          })()}
          {record.status !== 'running' && record.usage?.duration_ms && (
            <span style={{ fontSize: 11, color: 'var(--color-success)', fontWeight: 600 }}>
              {formatDurationSec(record.usage.duration_ms / 1000)}
            </span>
          )}
          {record.status === 'running' && (
            <span style={{ fontSize: 11, color: 'var(--color-info)', fontWeight: 600 }}>
              {formatDurationSec(getElapsedSeconds(record.started_at))}
            </span>
          )}
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
          <span style={{
            fontSize: 11, padding: '3px 12px', borderRadius: 12,
            backgroundColor: record.status === 'success' ? 'var(--color-success)' : record.status === 'failed' ? 'var(--color-error)' : 'var(--color-info)',
            color: '#fff', fontWeight: 600,
          }}>
            {record.status === 'success' ? '成功' : record.status === 'failed' ? '失败' : '进行中'}
          </span>
          {!isRunning && supportsResume(record) && (
            <Button type="primary" size="small" icon={<MessageOutlined />} onClick={() => onOpenResume(record)}>继续对话</Button>
          )}
          {!isRunning && (
            <NarrowRatingControl record={record} onRate={onRate} />
          )}
          {hasLogsStatic(record) && (
            <Button size="small" icon={<FileTextOutlined />} onClick={() => onExport(record)}>导出YAML</Button>
          )}
          {isRunning && (
            <Popconfirm title="确定强制停止该任务？" okText="停止" cancelText="取消" onConfirm={() => onStop(record.id)}>
              <Button type="primary" danger size="middle" icon={<StopOutlined />} style={{ fontSize: 14, fontWeight: 600, height: '32px', display: 'flex', alignItems: 'center', gap: '6px' }}>停止任务</Button>
            </Popconfirm>
          )}
        </div>
      </div>
      {/* 点击命令文本即可复制，不需要额外的复制按钮 */}
      {/* 复制逻辑三步走：①检查 clipboard API 可用性 → ②写入剪贴板 → ③反馈结果 */}
      {/* 使用 navigator.clipboard?.writeText 可选链：HTTP 环境或旧浏览器中该 API 为 undefined，直接调用会报 TypeError */}
      {record.command && (
        <Tooltip title="点击复制命令">
          <div
            onClick={async () => {
              try {
                if (!navigator.clipboard?.writeText) {
                  messageApi.error('当前环境不支持复制');
                  return;
                }
                await navigator.clipboard.writeText(record.command || '');
                messageApi.success('已复制');
              } catch {
                messageApi.error('复制失败');
              }
            }}
            style={{ fontSize: 11, color: 'var(--color-text-quaternary)', marginBottom: 8, fontFamily: 'var(--font-mono)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', cursor: 'pointer' }}
          >
            {record.command}
          </div>
        </Tooltip>
      )}
      {record.result !== null && record.result !== '' && (
        <div className={`history-result ${record.status === 'success' ? 'history-result-success' : 'history-result-failed'}`}>
          <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 4 }}>
            {/* 复制结论文本：先检查 clipboard API 可用性，防止在不支持的浏览器中崩溃 */}
            <Button type="text" size="small" icon={<CopyOutlined />} onClick={async () => {
              try {
                if (!navigator.clipboard?.writeText) {
                  messageApi.error('当前环境不支持复制');
                  return;
                }
                await navigator.clipboard.writeText(record.result || '');
                messageApi.success('已复制到剪贴板');
              } catch {
                messageApi.error('复制失败');
              }
            }} />
          </div>
          <XMarkdown content={record.result} />
        </div>
      )}
      {record.usage && (
        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginTop: 8, display: 'flex', gap: 12, flexWrap: 'wrap' }}>
          <span>Input: {record.usage.input_tokens.toLocaleString()}</span>
          <span>Output: {record.usage.output_tokens.toLocaleString()}</span>
          {record.usage.total_cost_usd !== null && (
            <span style={{ color: 'var(--color-warning)', fontWeight: 600 }}>${record.usage.total_cost_usd.toFixed(6)}</span>
          )}
        </div>
      )}
      {(() => {
        const stats = resolveStats(record, isRunning);
        if (!stats) return null;
        return (
          <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginTop: 8, display: 'flex', gap: 12, flexWrap: 'wrap' }}>
            <span>工具调用: <b style={{ color: 'var(--color-primary)' }}>{stats.tool_calls}</b></span>
            <span>对话轮次: <b style={{ color: 'var(--color-primary)' }}>{stats.conversation_turns}</b></span>
            {stats.thinking_count > 0 && (
              <span>思考次数: <b style={{ color: 'var(--color-primary)' }}>{stats.thinking_count}</b></span>
            )}
          </div>
        );
      })()}
      {/* 评分与评审状态 */}
      <div style={{ marginTop: 8, display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
        <NarrowRatingControl record={record} onRate={onRate} />
        <ReviewStatusBadge status={record.last_review_status} />
      </div>
      <NarrowLogView
        record={record}
        isRunning={isRunning}
        displayLogs={displayLogs}
        liveLogs={liveLogs}
        viewMode={viewMode}
        onRefresh={onRefresh}
        onViewModeChange={onViewModeChange}
      />
    </div>
  );
}

/** 手机版评分控件（简化版） */
function NarrowRatingControl({
  record,
  onRate,
}: {
  record: ExecutionRecord;
  onRate: (recordId: number, rating: number | null) => Promise<void>;
}) {
  const [open, setOpen] = useState(false);
  const [value, setValue] = useState<number | null>(record.rating ?? null);

  useEffect(() => {
    setValue(record.rating ?? null);
  }, [record.rating, record.id]);

  const handleSubmit = async (next: number | null) => {
    try {
      await onRate(record.id, next);
      setOpen(false);
    } catch {
      // 错误由上层拦截器统一提示
    }
  };

  if (record.rating != null) {
    return (
      <Popover content={
        <div style={{ width: 200 }}>
          <div style={{ marginBottom: 8, fontSize: 12, color: 'var(--color-text-secondary)' }}>评分：{record.rating}</div>
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
        <div style={{ marginBottom: 8, fontSize: 12, color: 'var(--color-text-secondary)' }}>为本次执行结果评分（0-100）</div>
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
