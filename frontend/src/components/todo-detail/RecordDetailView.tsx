import { useState, useEffect, type ReactNode } from 'react';
import { Button, Tag, Empty, Segmented, Popconfirm, Tooltip, Pagination, message, Popover, InputNumber, Space } from 'antd';
import { StarOutlined, StarFilled, SyncOutlined, CheckCircleOutlined, CloseCircleOutlined, PauseCircleOutlined, MinusCircleOutlined } from '@ant-design/icons';
import { MessageOutlined, FileTextOutlined, StopOutlined, UnorderedListOutlined, LinkOutlined, LoadingOutlined, BranchesOutlined, CodeOutlined } from '@ant-design/icons';
import { ExecutorBadge } from '@/components/ExecutorBadge';
import { ChatView } from '@/components/ChatView';
import { RefreshBtn } from './LogViewHeader';
import { formatLocalDateTime, formatDurationSec } from '@/utils/datetime';
import { getElapsedSeconds, formatLogTime } from './helpers';
import { LOG_TYPE_COLORS, LOG_TYPE_LABELS } from '@/constants';
import type { SessionGroup } from './helpers';
import { supportsResume } from '@/types';
import type { ExecutionRecord, LogEntry } from '@/types';
// todo hook 已整块移除（plan `purring-forging-petal`），不再需要 getHookTriggerLabel。
import { copyToClipboard } from '@/utils/clipboard';
import { CommandPanel } from '@/components/CommandPanel';
import { CollapsibleConclusion } from './CollapsibleConclusion';

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
  viewMode: 'log' | 'chat' | 'command';
  onViewModeChange: (mode: 'log' | 'chat' | 'command') => void;
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
          {record.status !== 'running' && record.usage?.duration_ms && (
            <span style={{ fontSize: 12, color: 'var(--color-success)', fontWeight: 600 }}>
              {formatDurationSec(record.usage.duration_ms / 1000)}
            </span>
          )}
          {record.status === 'running' && (
            <span style={{ fontSize: 12, color: 'var(--color-info)', fontWeight: 600 }}>
              {formatDurationSec(getElapsedSeconds(record.started_at))}
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
      {/* 点击命令文本即可复制，不需要额外的复制按钮 */}
      {/* 使用 copyToClipboard 工具函数统一处理剪贴板写入，支持 HTTP 环境（通过 fallback 到 execCommand） */}
      {record.command && (
        <Tooltip title="点击复制命令">
          <div
            onClick={async () => {
              try {
                // 调用统一的复制工具（内置 fallback，兼容 HTTP 环境）
                const ok = await copyToClipboard(record.command || '');
                // 根据返回结果提示用户
                if (ok) {
                  message.success('已复制');
                } else {
                  message.error('复制失败');
                }
              } catch {
                message.error('复制失败');
              }
            }}
            style={{ fontSize: 11, color: 'var(--color-text-quaternary)', marginBottom: 12, fontFamily: 'var(--font-mono)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', cursor: 'pointer' }}
          >
            {record.command}
          </div>
        </Tooltip>
      )}
      {/* issue #645: 展示本次执行使用的 git worktree 目录路径；目录可能已被清理，但仍保留在记录里便于排查 */}
      <WorktreePathDisplay worktreePath={record.worktree_path ?? null} />
      {record.result !== null && record.result !== '' && (
        // 折叠/展开控制由 CollapsibleConclusion 内部状态管理；
        // 传 recordId 让折叠状态按记录持久化，避免每次重渲染都重新展开长结论。
        <CollapsibleConclusion
          result={record.result}
          status={record.status}
          messageApi={message}
          showTitle
          recordId={record.id}
        />
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
        // issue #648: 把头部 Segmented 抽到三个分支共享，body 各自渲染
        const header = (
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8, flexShrink: 0 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--color-primary)' }}>
                {viewMode === 'command'
                  ? `命令视图 (${displayLogs.length} 条${isRunning && liveLogs && liveLogs.length > 0 ? ' · 实时' : ''})`
                  : viewMode === 'chat'
                    ? `对话视图 (${displayLogs.length} 条${isRunning && liveLogs && liveLogs.length > 0 ? ' · 实时' : ''})`
                    : `执行过程 (${isRunning ? displayLogs.length : logsTotal} 条${isRunning && liveLogs && liveLogs.length > 0 ? ' · 实时' : ''})`}
              </span>
              <RefreshBtn onClick={() => {
                onRefreshSingle(record.id);
                onLoadLogs(record.id, logsPage);
              }} />
            </div>
            <Segmented
              size="small"
              value={viewMode}
              onChange={(value) => onViewModeChange(value as 'log' | 'chat' | 'command')}
              options={[
                { value: 'log', icon: <UnorderedListOutlined />, label: '日志' },
                { value: 'chat', icon: <MessageOutlined />, label: '对话' },
                { value: 'command', icon: <CodeOutlined />, label: '命令' },
              ]}
            />
          </div>
        );
        if (viewMode === 'chat') {
          return (
            <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
              {header}
              <ChatView logs={displayLogs as LogEntry[]} isRunning={isRunning} />
            </div>
          );
        }
        // issue #648: 命令视图 — 走 CommandPanel
        if (viewMode === 'command') {
          return (
            <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
              {header}
              <div style={{ flex: 1, overflow: 'auto', padding: 4 }}>
                <CommandPanel logs={displayLogs} executor={record.executor} />
              </div>
            </div>
          );
        }
        return (
          <div>
            {header}
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
                    <span style={{ color: LOG_TYPE_COLORS[log.type || ''] || 'var(--log-text)' }}>
                      [{LOG_TYPE_LABELS[log.type || ''] || log.type}]
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
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
          <Button
            size="small"
            icon={<StarFilled style={{ color: '#fadb14' }} />}
            onClick={() => setOpen(o => !o)}
            aria-label="已评分，点击修改"
          >
            {record.rating}
          </Button>
          <ReviewStatusBadge status={record.last_review_status} />
        </span>
      </Popover>
    );
  }

  return (
    <Popover content={content} open={open} onOpenChange={setOpen} placement="bottomRight">
      <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
        <Button
          size="small"
          icon={<StarOutlined />}
          onClick={() => setOpen(o => !o)}
          aria-label="评分"
        >
          评分
        </Button>
        <ReviewStatusBadge status={record.last_review_status} />
      </span>
    </Popover>
  );
}

/** 评审状态徽章: pending(评审中) / success(评审成功) / failed(评审失败) / interrupted(被打断) / skipped(跳过) */
export function ReviewStatusBadge({ status }: { status?: 'pending' | 'success' | 'failed' | 'interrupted' | 'skipped' | null }) {
  if (!status) return null;
  const map: Record<string, { color: string; bg: string; border: string; text: string; icon: ReactNode }> = {
    pending:     { color: '#1677ff', bg: '#1677ff14', border: '#1677ff30', text: '⏳ 评审中',   icon: <SyncOutlined spin /> },
    success:     { color: '#52c41a', bg: '#52c41a14', border: '#52c41a30', text: '✅ 评审成功', icon: <CheckCircleOutlined /> },
    failed:      { color: '#ff4d4f', bg: '#ff4d4f14', border: '#ff4d4f30', text: '❌ 评审失败', icon: <CloseCircleOutlined /> },
    interrupted: { color: '#faad14', bg: '#faad1414', border: '#faad1430', text: '⏸ 中断',     icon: <PauseCircleOutlined /> },
    skipped:     { color: '#6b7280', bg: '#6b728014', border: '#6b728030', text: '⏭ 跳过',     icon: <MinusCircleOutlined /> },
  };
  const s = map[status];
  if (!s) return null;
  return (
    <span
      style={{
        fontSize: 11,
        padding: '1px 6px',
        borderRadius: 4,
        color: s.color,
        background: s.bg,
        border: `1px solid ${s.border}`,
        display: 'inline-flex',
        alignItems: 'center',
        gap: 3,
        whiteSpace: 'nowrap',
      }}
      title={`自动评审状态: ${status}`}
    >
      {s.icon}
      {s.text}
    </span>
  );
}

/**
 * issue #645: 展示执行记录关联的 git worktree 目录路径。
 *
 * - 未启用 worktree 时不渲染（`worktree_path` 为 `null` 或空串）。
 * - 点击整行即可复制完整路径；与命令复制复用同一 `copyToClipboard` 工具，兼容 HTTP 环境。
 * - 路径可能已被 `WorktreeService::cleanup_worktree` 清理，复制仍可成功，
 *   用于排查"当时用了哪个 worktree"非常方便。
 */
function WorktreePathDisplay({ worktreePath }: { worktreePath: string | null }) {
  // 空值时整段不渲染，避免出现 "Worktree: " 这种空标签
  if (!worktreePath) return null;

  // 路径过长时只展示尾部，鼠标悬停 tooltip 显示完整路径
  const displayPath = worktreePath.length > 60
    ? `…${worktreePath.slice(-59)}`
    : worktreePath;

  return (
    <Tooltip title={worktreePath}>
      <div
        onClick={async () => {
          // 复用统一的复制工具：HTTPS 走 navigator.clipboard，
          // HTTP 环境自动 fallback 到 execCommand，保持与命令复制一致
          const ok = await copyToClipboard(worktreePath);
          message[ok ? 'success' : 'error'](ok ? '已复制 worktree 路径' : '复制失败');
        }}
        // 与命令展示行保持一致的视觉权重：四级文本色、等宽字体、单行省略
        style={{
          fontSize: 11,
          color: 'var(--color-text-quaternary)',
          marginBottom: 12,
          fontFamily: 'var(--font-mono)',
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          whiteSpace: 'nowrap',
          cursor: 'pointer',
          display: 'flex',
          alignItems: 'center',
          gap: 6,
        }}
      >
        <BranchesOutlined style={{ fontSize: 11, color: 'var(--color-primary)' }} />
        <span>Worktree: {displayPath}</span>
      </div>
    </Tooltip>
  );
}
