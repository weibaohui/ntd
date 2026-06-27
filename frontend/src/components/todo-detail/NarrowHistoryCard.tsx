import { useState, useEffect } from 'react';
import { Button, Popconfirm, Tag, Tooltip, Popover, InputNumber, Space } from 'antd';
import { MessageOutlined, FileTextOutlined, StopOutlined, StarOutlined, StarFilled } from '@ant-design/icons';
import { ExecutorBadge } from '@/components/ExecutorBadge';
import { supportsResume } from '@/types';
import { formatLocalDateTime, formatDurationSec } from '@/utils/datetime';
import * as db from '@/utils/database';
import { getElapsedSeconds, hasLogsStatic } from './helpers';
import { NarrowLogView } from './NarrowLogView';
// todo hook 已整块移除（plan `purring-forging-petal`），不再需要 getHookTriggerLabel。
import { ReviewStatusBadge } from './RecordDetailView';
import { CollapsibleConclusion } from './CollapsibleConclusion';
import type { ExecutionRecord, ExecutionStats, LogEntry } from '@/types';
import { copyToClipboard } from '@/utils/clipboard';

/** Narrow mode: single history card */
export function NarrowHistoryCard({ record, viewMode, onOpenResume, onExport, onStop, onRefresh, onRate, getRunningTask, resolveStats, parseLogs, messageApi, onViewModeChange }: {
  record: ExecutionRecord;
  viewMode: 'log' | 'chat' | 'command';
  onOpenResume: (r: ExecutionRecord) => void;
  onExport: (r: ExecutionRecord) => void;
  onStop: (id: number) => Promise<void>;
  onRefresh: (id: number) => Promise<void>;
  onRate: (recordId: number, rating: number | null) => Promise<void>;
  getRunningTask: (r: ExecutionRecord) => any;
  resolveStats: (r: ExecutionRecord, running: boolean) => ExecutionStats | null | undefined;
  parseLogs: (r: ExecutionRecord) => LogEntry[];
  messageApi: any;
  onViewModeChange: (mode: 'log' | 'chat' | 'command') => void;
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
          <Tag color={record.trigger_type === 'cron' ? '#8b5cf6' : '#6b7280'} style={{ fontSize: 10 }}>
            {record.trigger_type === 'cron' ? 'Cron' : '手动'}
          </Tag>
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
            <Button type="text" size="small" icon={<FileTextOutlined />} onClick={() => onExport(record)}>导出YAML</Button>
          )}
          {isRunning && (
            <Popconfirm title="确定强制停止该任务？" okText="停止" cancelText="取消" onConfirm={() => onStop(record.id)}>
              <Button type="text" size="small" icon={<StopOutlined />} style={{ fontSize: 14, fontWeight: 600, height: '32px', display: 'flex', alignItems: 'center', gap: '6px' }}>停止任务</Button>
            </Popconfirm>
          )}
        </div>
      </div>
      {/* 点击命令文本即可复制，不需要额外的复制按钮 */}
      {/* 使用 copyToClipboard 统一处理，兼容 HTTP 环境（通过 fallback 到 execCommand） */}
      {record.command && (
        <Tooltip title="点击复制命令">
          <div
            onClick={async () => {
              try {
                // 调用统一的复制工具（内置 fallback，兼容 HTTP 环境）
                const ok = await copyToClipboard(record.command || '');
                // 根据返回结果提示用户
                if (ok) {
                  messageApi.success('已复制');
                } else {
                  messageApi.error('复制失败');
                }
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
        // 窄屏/手机版的结论区与桌面端共用同一 CollapsibleConclusion，
        // 折叠状态按 recordId 持久化，长结论在窄屏下尤其友好。
        <CollapsibleConclusion
          result={record.result}
          status={record.status}
          messageApi={messageApi}
          recordId={record.id}
        />
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
