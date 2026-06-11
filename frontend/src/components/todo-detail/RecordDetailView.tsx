import { useState, useEffect } from 'react';
import { Button, Tag, Empty, Segmented, Popconfirm, Tooltip, Pagination, message, Popover, InputNumber, Space } from 'antd';
import { StarOutlined, StarFilled } from '@ant-design/icons';
import { MessageOutlined, FileTextOutlined, StopOutlined, CopyOutlined, UnorderedListOutlined, LinkOutlined, LoadingOutlined } from '@ant-design/icons';
import XMarkdown from '@ant-design/x-markdown';
import { ExecutorBadge } from '@/components/ExecutorBadge';
import { ChatView } from '@/components/ChatView';
import { RefreshBtn } from './LogViewHeader';
import { formatLocalDateTime, formatDuration } from '@/utils/datetime';
import { getElapsedSeconds, formatLogTime, logTypeColors, logTypeLabels } from './helpers';
import type { SessionGroup } from './helpers';
import { supportsResume } from '@/types';
import type { ExecutionRecord, LogEntry } from '@/types';
import { getHookTriggerLabel } from '@/utils/database/hooks';

export function RecordDetailView({
  isLoadingDetail, record, sessionGroups,
  onSelectRecord, viewMode, onViewModeChange,
  onOpenResume, onExportMarkdown, onStop, onRefreshSingle, onRate,
  paginatedLogs, logsTotal, logsPage, logsPerPage, onLoadLogs, isLoadingLogs,
  getRunningTaskForRecord, resolveExecutionStats,
}: {
  isLoadingDetail: boolean;
  record: ExecutionRecord | null;
  sessionGroups: SessionGroup[];
  onSelectRecord: (id: number) => void;
  viewMode: 'log' | 'chat';
  onViewModeChange: (mode: 'log' | 'chat') => void;
  onOpenResume: (record: ExecutionRecord) => void;
  onExportMarkdown: (record: ExecutionRecord) => Promise<void>;
  onStop: (recordId: number) => Promise<void>;
  onRefreshSingle: (recordId: number) => Promise<void>;
  /**
   * 评分回调。record 表示被评分的新值，recordId 是被清分的记录。
   * 传 null 表示清除评分。
   */
  onRate: (recordId: number, rating: number | null) => Promise<void>;
  paginatedLogs: LogEntry[];
  logsTotal: number;
  logsPage: number;
  logsPerPage: number;
  onLoadLogs: (recordId: number, page: number) => Promise<void>;
  isLoadingLogs: boolean;
  getRunningTaskForRecord: (record: ExecutionRecord) => any;
  resolveExecutionStats: (record: ExecutionRecord, isRunning: boolean) => any;
}) {
  if (isLoadingDetail) {
    return (
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%', gap: 8, color: 'var(--color-text-secondary)' }}>
        <LoadingOutlined style={{ fontSize: 20, color: 'var(--color-primary)' }} />
        <span>加载执行详情...</span>
      </div>
    );
  }

  if (!record) {
    return (
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%' }}>
        <Empty description="选择一条执行记录查看详情" image={Empty.PRESENTED_IMAGE_SIMPLE} />
      </div>
    );
  }

  const isRunning = record.status === 'running';
  const runningTask = isRunning ? getRunningTaskForRecord(record) : null;
  const liveLogs = runningTask ? runningTask.logs : null;
  const displayLogs = liveLogs && liveLogs.length > 0 ? liveLogs : paginatedLogs;

  return (
    <>
      {(() => {
        const group = sessionGroups.find(g => g.records.some(r => r.id === record!.id));
        if (!group || group.records.length <= 1 || !group.records[0].session_id) return null;
        const idx = group.records.findIndex(r => r.id === record!.id);
        if (idx <= 0) return null;
        return (
          <div style={{
            display: 'flex', alignItems: 'center', gap: 6,
            marginBottom: 10, padding: '4px 10px', borderRadius: 6,
            background: 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)',
            fontSize: 11, color: 'var(--color-text-tertiary)',
          }}>
            <LinkOutlined style={{ color: 'var(--color-primary)', fontSize: 11 }} />
            <span>继续自</span>
            <span
              onClick={() => onSelectRecord(group.records[0].id)}
              style={{ cursor: 'pointer', color: 'var(--color-primary)', fontWeight: 500 }}
            >
              {formatLocalDateTime(group.records[0].started_at)}
            </span>
            {record.resume_message && (
              <>
                <span style={{ color: 'var(--color-border)' }}>·</span>
                <span style={{ color: 'var(--color-text-secondary)', fontStyle: 'italic' }}>
                  "{String(record.resume_message).length > 40 ? String(record.resume_message).substring(0, 40) + '...' : record.resume_message}"
                </span>
              </>
            )}
            <span style={{ marginLeft: 'auto', color: 'var(--color-text-quaternary)' }}>
              第{idx + 1}轮 / 共{group.records.length}轮
            </span>
          </div>
        );
      })()}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12, flexWrap: 'wrap', gap: 8 }}>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
          {record.executor && <ExecutorBadge executor={record.executor} />}
          {record.model && <Tag color="#3b82f6">{record.model}</Tag>}
          <span style={{ fontSize: 13, color: 'var(--color-text-secondary)', fontWeight: 500 }}>
            {formatLocalDateTime(record.started_at)}
          </span>
          <span style={{
            fontSize: 11,
            padding: '3px 12px',
            borderRadius: 12,
            backgroundColor: record.status === 'success' ? 'var(--color-success)' : record.status === 'failed' ? 'var(--color-error)' : 'var(--color-info)',
            color: '#fff',
            fontWeight: 600,
          }}>
            {record.status === 'success' ? '成功' : record.status === 'failed' ? '失败' : '进行中'}
          </span>
          {(() => {
            const label = getHookTriggerLabel(record.trigger_type);
            if (!label || record.source_todo_id == null) return null;
            return (
              <Tag color="purple" icon={<LinkOutlined />} style={{ margin: 0 }}>
                被 #{record.source_todo_id} {record.source_todo_title ?? '?'} 的「{label}」hook 触发
              </Tag>
            );
          })()}
          {record.status !== 'running' && record.usage?.duration_ms && (
            <span style={{ fontSize: 12, color: 'var(--color-success)', fontWeight: 600 }}>
              {formatDuration(record.usage.duration_ms / 1000)}
            </span>
          )}
          {record.status === 'running' && (
            <span style={{ fontSize: 12, color: 'var(--color-info)', fontWeight: 600 }}>
              {formatDuration(getElapsedSeconds(record.started_at))}
            </span>
          )}
        </div>
        <div style={{ display: 'flex', gap: 8 }}>
          {record.status !== 'running' && supportsResume(record) && (
            <Button type="primary" size="small" icon={<MessageOutlined />} onClick={() => onOpenResume(record)}>继续对话</Button>
          )}
          {record.status !== 'running' && (
            <RecordRatingControl
              record={record}
              onRate={onRate}
            />
          )}
          {record.status !== 'running' && !!record.finished_at && (
            <Button size="small" icon={<FileTextOutlined />} onClick={() => onExportMarkdown(record)}>导出YAML</Button>
          )}
          {record.status === 'running' && (
            <Popconfirm
              title="确定强制停止该任务？"
              okText="停止"
              cancelText="取消"
              onConfirm={async () => { await onStop(record.id); }}
            >
              <Button type="primary" danger size="small" icon={<StopOutlined />}>停止</Button>
            </Popconfirm>
          )}
        </div>
      </div>
      {record.command && (
        <Tooltip title="点击复制命令">
          <div
            onClick={() => { navigator.clipboard.writeText(record.command || '').then(() => message.success('已复制')); }}
            style={{ fontSize: 11, color: 'var(--color-text-quaternary)', marginBottom: 12, fontFamily: 'var(--font-mono)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', cursor: 'pointer' }}
          >
            {record.command}
          </div>
        </Tooltip>
      )}
      {record.result !== null && record.result !== '' && (
        <div className={`history-result ${record.status === 'success' ? 'history-result-success' : 'history-result-failed'}`} style={{ marginBottom: 12 }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 4 }}>
            <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--color-text)' }}>结论</span>
            <Button
              type="text"
              size="small"
              icon={<CopyOutlined />}
              onClick={async () => {
                try {
                  await navigator.clipboard.writeText(record.result || '');
                  message.success('已复制到剪贴板');
                } catch {
                  message.error('复制失败');
                }
              }}
            />
          </div>
          <XMarkdown content={record.result} />
        </div>
      )}
      {record.usage && (
        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 12, display: 'flex', gap: 12, flexWrap: 'wrap' }}>
          <span>Input: {record.usage.input_tokens.toLocaleString()}</span>
          <span>Output: {record.usage.output_tokens.toLocaleString()}</span>
          {record.usage.total_cost_usd !== null && (
            <span style={{ color: 'var(--color-warning)', fontWeight: 600 }}>${record.usage.total_cost_usd.toFixed(6)}</span>
          )}
        </div>
      )}
      {(() => {
        const stats = resolveExecutionStats(record, isRunning);
        if (!stats) return null;
        return (
          <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 12, display: 'flex', gap: 12, flexWrap: 'wrap' }}>
            <span>工具调用: <b style={{ color: 'var(--color-primary)' }}>{stats.tool_calls}</b></span>
            <span>对话轮次: <b style={{ color: 'var(--color-primary)' }}>{stats.conversation_turns}</b></span>
            {stats.thinking_count > 0 && (
              <span>思考次数: <b style={{ color: 'var(--color-primary)' }}>{stats.thinking_count}</b></span>
            )}
          </div>
        );
      })()}
      {(() => {
        if (!isRunning && displayLogs.length === 0) return null;
        if (viewMode === 'chat') {
          return (
            <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8, flexShrink: 0 }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                  <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--color-primary)' }}>
                    对话视图 ({displayLogs.length} 条){isRunning && liveLogs && liveLogs.length > 0 ? ' · 实时' : ''}
                  </span>
                  <RefreshBtn onClick={() => onRefreshSingle(record.id)} />
                </div>
                <Segmented
                  size="small"
                  value={viewMode}
                  onChange={(value) => onViewModeChange(value as 'log' | 'chat')}
                  options={[
                    { value: 'log', icon: <UnorderedListOutlined />, label: '日志' },
                    { value: 'chat', icon: <MessageOutlined />, label: '对话' },
                  ]}
                />
              </div>
              <ChatView logs={displayLogs as LogEntry[]} isRunning={isRunning} />
            </div>
          );
        }
        return (
          <div>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--color-primary)' }}>
                  执行过程 ({isRunning ? displayLogs.length : logsTotal} 条{isRunning && liveLogs && liveLogs.length > 0 ? ' · 实时' : ''})
                </span>
                <RefreshBtn onClick={() => {
                  onRefreshSingle(record.id);
                  onLoadLogs(record.id, logsPage);
                }} />
              </div>
              <Segmented
                size="small"
                value={viewMode}
                onChange={(value) => onViewModeChange(value as 'log' | 'chat')}
                options={[
                  { value: 'log', icon: <UnorderedListOutlined />, label: '日志' },
                  { value: 'chat', icon: <MessageOutlined />, label: '对话' },
                ]}
              />
            </div>
            <div style={{
              background: 'var(--log-bg)',
              color: 'var(--log-text)',
              padding: 12,
              borderRadius: 8,
              fontFamily: 'var(--font-mono)',
              fontSize: 11,
              overflow: 'auto',
            }}>
              {displayLogs.length === 0 ? (
                <div style={{ color: 'var(--log-text-muted)' }}>{isRunning ? '等待输出...' : (isLoadingLogs ? '加载中...' : '暂无日志')}</div>
              ) : (
                displayLogs.map((log: LogEntry, idx: number) => (
                  <div key={idx} style={{ marginBottom: 4, display: 'flex', gap: 8 }}>
                    <span style={{ color: 'var(--log-text-muted)', flexShrink: 0 }}>{formatLogTime(log.timestamp || '')}</span>
                    <span style={{ color: logTypeColors[log.type || ''] || 'var(--log-text)' }}>
                      [{logTypeLabels[log.type || ''] || log.type}]
                    </span>
                    <span>{log.content}</span>
                  </div>
                ))
              )}
            </div>
            {!isRunning && logsTotal > logsPerPage && (
              <Pagination
                simple
                current={logsPage}
                pageSize={logsPerPage}
                total={logsTotal}
                onChange={(page) => onLoadLogs(record.id, page)}
                size="small"
                style={{ marginTop: 8, textAlign: 'center' }}
              />
            )}
          </div>
        );
      })()}
    </>
  );
}

/**
 * 评分控件（仅针对已结束的执行记录）。
 * - 未评分时：点击“评分”按钮弹出 Popover，输入 0-100 提交。
 * - 已评分时：直接显示 `★ 85` 可点击重新编辑；提供“清除”按钮。
 * 设计：评分仅属于“执行结果”，不能给 running 记录评分（RecordDetailView
 * 会在 status === 'running' 时不渲染本控件）。
 */
function RecordRatingControl({
  record,
  onRate,
}: {
  record: ExecutionRecord;
  onRate: (recordId: number, rating: number | null) => Promise<void>;
}) {
  const [open, setOpen] = useState(false);
  const [value, setValue] = useState<number | null>(record.rating ?? null);
  const [submitting, setSubmitting] = useState(false);

  // 跟随 record.rating 变化同步本地值（避免外部刷新后表单与实际评分不一致）
  useEffect(() => {
    setValue(record.rating ?? null);
  }, [record.rating, record.id]);

  const handleSubmit = async (next: number | null) => {
    setSubmitting(true);
    try {
      await onRate(record.id, next);
      setOpen(false);
    } catch {
      // 错误由上层拦截器统一提示，本处仅保持弹窗开启以便用户重试
    } finally {
      setSubmitting(false);
    }
  };

  const content = (
    <div style={{ width: 220 }}>
      <div style={{ marginBottom: 8, fontSize: 12, color: 'var(--color-text-secondary)' }}>
        为本次执行结果评分（0-100）
      </div>
      <Space.Compact style={{ width: '100%' }}>
        <InputNumber
          min={0}
          max={100}
          value={value}
          onChange={v => setValue(typeof v === 'number' ? v : null)}
          placeholder="0-100"
          style={{ width: '100%' }}
          autoFocus
          onPressEnter={() => {
            if (value != null) handleSubmit(value);
          }}
        />
        <Button
          type="primary"
          loading={submitting}
          disabled={value == null}
          onClick={() => handleSubmit(value)}
        >
          保存
        </Button>
      </Space.Compact>
      {record.rating != null && (
        <Button
          type="link"
          size="small"
          danger
          style={{ padding: '4px 0 0', marginTop: 4 }}
          disabled={submitting}
          onClick={() => handleSubmit(null)}
        >
          清除评分
        </Button>
      )}
    </div>
  );

  if (record.rating != null) {
    // 已评分：以徽章形式呈现，点击重新编辑
    return (
      <Popover content={content} open={open} onOpenChange={setOpen} placement="bottomRight">
        <Button
          size="small"
          icon={<StarFilled style={{ color: '#fadb14' }} />}
          onClick={() => setOpen(o => !o)}
          aria-label="已评分，点击修改"
        >
          {record.rating}
        </Button>
      </Popover>
    );
  }

  return (
    <Popover content={content} open={open} onOpenChange={setOpen} placement="bottomRight">
      <Button
        size="small"
        icon={<StarOutlined />}
        onClick={() => setOpen(o => !o)}
        aria-label="评分"
      >
        评分
      </Button>
    </Popover>
  );
}
