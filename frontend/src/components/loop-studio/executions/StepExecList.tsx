// 环节执行卡片：卡片式布局 + 箭头连接展示执行顺序，每张卡展示执行详情。

import { useState, useCallback } from 'react';
import { Drawer, Descriptions, Tag, Button, Skeleton, Empty, App as AntApp } from 'antd';
import { ArrowRightOutlined, ReadOutlined } from '@ant-design/icons';
import bundledApi from '@/api/bundled';
import * as dbExecutions from '@/utils/database/executions';
import type { LogEntry } from '@/types';
import { LogDrawer } from '@/components/todo-post/LogDrawer';
import { execStatusView, durationLabel, formatToken } from './helpers';

interface StepExecListProps {
  stepExecs: Record<string, any>[];
  loopId: number;
  /** 当前工作空间 ID（v1 路由 workspace-scoped） */
  workspaceId: number;
  executionId: number;
  onApproved: () => void;
}

export function StepExecList({ stepExecs, loopId, workspaceId, executionId, onApproved }: StepExecListProps) {
  const { message } = AntApp.useApp();
  const [drawerRecord, setDrawerRecord] = useState<any | null>(null);
  const [drawerLoading, setDrawerLoading] = useState(false);
  // ── 执行日志 Drawer 状态（下探查看完整日志） ─────────────
  const [logDrawerRecord, setLogDrawerRecord] = useState<any | null>(null);
  const [logDrawerLogs, setLogDrawerLogs] = useState<LogEntry[]>([]);

  // 加载日志并打开日志 Drawer
  const handleOpenLogView = useCallback(async (record: any) => {
    setLogDrawerRecord(record);
    try {
      const result = await dbExecutions.getExecutionLogs(workspaceId, record.id, 1, 500);
      setLogDrawerLogs(result.logs || []);
    } catch {
      setLogDrawerLogs([]);
    }
  }, []);

  // 人工审批状态：记录正在提交审批的环节 id，避免重复点击；
  // 044 起改门禁制（通过/拒绝二选一），不再需要评分滑块；
  // 也不带「审批意见」——与 ProcessExecutionBoard 的审批操作对齐为纯通过/拒绝，两处功能一致。
  const [approvingId, setApprovingId] = useState<number | null>(null);

  const handleCardClick = useCallback(async (s: any) => {
    if (!s.execution_record_id) return;
    setDrawerLoading(true);
    try {
      const { getExecutionRecord } = dbExecutions;
      const rec = await getExecutionRecord(workspaceId, s.execution_record_id);
      setDrawerRecord(rec);
    } catch {
      // ignore
    } finally {
      setDrawerLoading(false);
    }
  }, []);

  // 门禁制人工审批：通过/拒绝二选一，调门禁审批接口。
  // gateId 由后端在 step execution 的 pending_gate_id 字段注入（仅 pending_approval 时有值）。
  // 不带审批意见，与 ProcessExecutionBoard 的通过/拒绝保持功能一致（同一后端接口 approveGate）。
  const handleApprove = useCallback(async (stepExecutionId: number, gateId: number | null, approved: boolean) => {
    if (gateId == null) {
      message.error('未找到待审批门禁');
      return;
    }
    setApprovingId(stepExecutionId);
    try {
      await bundledApi.approveGate(workspaceId, loopId, executionId, stepExecutionId, gateId, approved);
      message.success(approved ? '已通过' : '已拒绝');
      onApproved();
    } catch (e: any) {
      message.error(e?.message || '审批失败');
    } finally {
      setApprovingId(null);
    }
  }, [workspaceId, loopId, executionId, message, onApproved]);

  if (stepExecs.length === 0) {
    return <Empty description="无环节执行记录" />;
  }
  return (
    <>
      <div style={{ display: 'flex', gap: 0, overflowX: 'auto', paddingBottom: 8, alignItems: 'stretch' }}>
      {stepExecs.map((s, idx) => {
        const view = execStatusView(s.status);
        const ratingPassed = s.rating != null && s.min_rating != null && s.rating >= s.min_rating;
        const duration = s.started_at ? durationLabel(s.started_at, s.finished_at) : '-';
        return (
          <div key={s.id} style={{ display: 'flex', alignItems: 'center', gap: 0 }}>
            {/* 箭头连接 + 跳转标注 */}
            {idx > 0 && (
              <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', padding: '0 2px' }}>
                <ArrowRightOutlined style={{ color: 'var(--color-text-tertiary, #94a3b8)', fontSize: 16 }} />
                {s.sequence_index != null && (
                  <span style={{ fontSize: 9, color: 'var(--color-text-tertiary)', fontFamily: 'monospace', lineHeight: 1 }}>
                    #{s.sequence_index}
                  </span>
                )}
              </div>
            )}

            {/* 执行卡片 */}
            <div
              onClick={() => handleCardClick(s)}
              style={{
                position: 'relative',
                width: 240, minWidth: 240,
                background: 'var(--color-bg-elevated, #ffffff)',
                border: `1px solid ${s.status === 'success' ? 'var(--color-success, #22c55e)' : s.status === 'failed' ? 'var(--color-error, #ef4444)' : 'var(--color-border, #e2e8f0)'}`,
                borderRadius: 10,
                padding: '14px 16px',
                cursor: s.execution_record_id ? 'pointer' : 'default',
                transition: 'box-shadow 200ms',
                userSelect: 'none',
              }}
            >
              {/* 黑板序号 + 执行序号 */}
              <div style={{ position: 'absolute', top: 6, left: 10, fontSize: 11, fontWeight: 600, color: 'var(--color-text-tertiary, #94a3b8)', display: 'flex', gap: 6 }}>
                {s.sequence_index != null && <span style={{ fontFamily: 'monospace' }}>#{s.sequence_index}</span>}
                <span style={{ fontFamily: 'monospace', color: 'var(--color-text-tertiary)' }}>/#{idx + 1}</span>
              </div>

              {/* 状态指示圆点 */}
              <div style={{ position: 'absolute', top: 8, right: 10, width: 8, height: 8, borderRadius: 4, background: view.color }} />

              {/* 环节名称 */}
              <div style={{ fontSize: 14, fontWeight: 600, color: 'var(--color-text, #0f172a)', marginBottom: 2, marginTop: 4, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {s.step_name || `环节 #${s.loop_step_id}`}
              </div>

              {/* 状态 + 耗时 */}
              <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 6 }}>
                {view.icon}
                <Tag color={view.color} style={{ margin: 0, fontSize: 11 }}>{s.status}</Tag>
                <span style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)', fontFamily: 'monospace' }}>{duration}</span>
              </div>

              {/* 评分 / 阈值：仅旧评分制历史记录展示（min_rating 非空）。
                  门禁制（044）执行 min_rating 恒空，状态以 status 标签为准；
                  approve_gate 会给通过/拒绝写入合成的 rating 100/0（仅供 resume 判定，非真实评分），
                  若仍按 rating>=min_rating 显示「通过/不通过」会误判——通过写入 100 却因 min_rating 空而显示「不通过」。 */}
              {s.min_rating != null && (
                <div style={{ display: 'flex', alignItems: 'center', gap: 6, flexWrap: 'wrap', marginBottom: 4 }}>
                  <span style={{ fontSize: 11, color: 'var(--color-text-secondary)' }}>
                    阈值 {s.min_rating}
                  </span>
                  {s.rating != null ? (
                    <>
                      <span style={{ fontSize: 11, color: 'var(--color-text-secondary)' }}>
                        评分 {s.rating}
                      </span>
                      <span style={{
                        padding: '1px 6px', borderRadius: 4, fontSize: 11, fontWeight: 600,
                        background: ratingPassed ? 'var(--color-success-bg, #f0fdf4)' : 'var(--color-error-bg, #fef2f2)',
                        color: ratingPassed ? 'var(--color-success, #22c55e)' : 'var(--color-error, #ef4444)',
                      }}>
                        {ratingPassed ? '通过' : '不通过'}
                      </span>
                    </>
                  ) : (
                    <span style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)' }}>未评审</span>
                  )}
                </div>
              )}

              {/* 结论（黑板） */}
              {s.conclusion && (
                <div style={{
                  marginTop: 6, padding: '6px 8px',
                  background: 'var(--color-bg-hover)', borderRadius: 6,
                  fontSize: 12, color: 'var(--color-text-secondary)',
                  lineHeight: 1.5, maxHeight: 60, overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  borderLeft: '3px solid var(--color-primary)',
                }}>
                  <div style={{ fontSize: 10, fontWeight: 600, color: 'var(--color-primary)', marginBottom: 2 }}>结论</div>
                  {s.conclusion.length > 120 ? s.conclusion.slice(0, 120) + '…' : s.conclusion}
                </div>
              )}

              {/* 时间 */}
              <div style={{ fontSize: 10, color: 'var(--color-text-tertiary, #94a3b8)', lineHeight: 1.5, marginTop: 4 }}>
                开始 {s.started_at ? new Date(s.started_at).toLocaleTimeString() : '-'}
                · 结束 {s.finished_at ? new Date(s.finished_at).toLocaleTimeString() : '-'}
              </div>

              {/* Token 消耗 */}
              {(s.input_tokens != null || s.output_tokens != null) && (
                <div style={{
                  display: 'flex', flexWrap: 'wrap', gap: 4,
                  marginTop: 4, fontSize: 10, color: 'var(--color-text-tertiary)',
                }}>
                  {s.input_tokens != null && (
                    <span>输入 {formatToken(s.input_tokens)}</span>
                  )}
                  {s.output_tokens != null && (
                    <span>输出 {formatToken(s.output_tokens)}</span>
                  )}
                  {s.cache_read_input_tokens != null && s.cache_read_input_tokens > 0 && (
                    <span>缓存读 {formatToken(s.cache_read_input_tokens)}</span>
                  )}
                </div>
              )}

              {/* 错误 */}
              {s.error_message && (
                <div style={{ marginTop: 4, fontSize: 11, color: 'var(--color-error, #ef4444)', lineHeight: 1.4 }}>
                  {s.error_message}
                </div>
              )}

              {/* 人工审批操作区域：pending_approval 状态时显示（044 门禁制：通过/拒绝二选一） */}
              {s.status === 'pending_approval' && (
                <div
                  onClick={(e) => e.stopPropagation()}
                  style={{
                    marginTop: 8,
                    padding: '8px 10px',
                    background: 'var(--color-warning-bg, #fffbeb)',
                    border: '1px solid var(--color-warning, #f59e0b)',
                    borderRadius: 8,
                  }}
                >
                  <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--color-warning, #f59e0b)', marginBottom: 6 }}>
                    ⏳ 等待人工审批
                  </div>
                  <div style={{ display: 'flex', gap: 6 }}>
                    <Button
                      size="small"
                      type="primary"
                      loading={approvingId === s.id}
                      onClick={() => handleApprove(s.id, s.pending_gate_id ?? null, true)}
                      style={{ flex: 1, fontSize: 11 }}
                    >
                      通过
                    </Button>
                    <Button
                      size="small"
                      danger
                      loading={approvingId === s.id}
                      onClick={() => handleApprove(s.id, s.pending_gate_id ?? null, false)}
                      style={{ flex: 1, fontSize: 11 }}
                    >
                      拒绝
                    </Button>
                  </div>
                </div>
              )}
            </div>
          </div>
        );
      })}
    </div>
    {drawerLoading && <Skeleton active style={{ padding: 24 }} />}
    <Drawer
      title={drawerRecord ? `执行记录 #${drawerRecord.id}` : '执行记录'}
      placement="right"
      width={520}
      open={!!drawerRecord}
      onClose={() => setDrawerRecord(null)}
      loading={drawerLoading}
    >
      {drawerRecord && (
        <Descriptions column={1} size="small" bordered={false} style={{ marginBottom: 16 }}>
          <Descriptions.Item label="状态">
            <Tag color={drawerRecord.status === 'success' ? 'green' : drawerRecord.status === 'failed' ? 'red' : 'blue'}>
              {drawerRecord.status}
            </Tag>
          </Descriptions.Item>
          <Descriptions.Item label="执行器">{drawerRecord.executor || '-'}</Descriptions.Item>
          {drawerRecord.rating != null && (
            <Descriptions.Item label="评分">{drawerRecord.rating} / 100</Descriptions.Item>
          )}
          <Descriptions.Item label="开始时间">{drawerRecord.started_at || '-'}</Descriptions.Item>
          <Descriptions.Item label="结束时间">{drawerRecord.finished_at || '-'}</Descriptions.Item>
          {drawerRecord.command && (
            <Descriptions.Item label="命令"><code style={{ wordBreak: 'break-all' }}>{drawerRecord.command}</code></Descriptions.Item>
          )}
        </Descriptions>
      )}
      {drawerRecord?.result && (
        <div>
          <div style={{ fontWeight: 600, marginBottom: 8, fontSize: 14 }}>执行结果</div>
          <div style={{
            background: 'var(--color-bg-hover)',
            padding: 12, borderRadius: 6, fontSize: 13,
            whiteSpace: 'pre-wrap', lineHeight: 1.6, maxHeight: 400, overflow: 'auto',
          }}>
            {drawerRecord.result}
          </div>
          <Button
            type="link"
            size="small"
            icon={<ReadOutlined />}
            onClick={() => handleOpenLogView(drawerRecord)}
            style={{ marginTop: 6, padding: 0, height: 'auto' }}
          >
            查看日志
          </Button>
        </div>
      )}
    </Drawer>

    {/* 执行详情 Drawer：点击"查看日志"时弹出，与黑板抽屉弹出的内容一致 */}
    <LogDrawer
      open={!!logDrawerRecord}
      record={logDrawerRecord}
      onClose={() => { setLogDrawerRecord(null); setLogDrawerLogs([]); }}
      paginatedLogs={logDrawerLogs}
      logsPage={1}
      isLoadingLogs={false}
      onLoadLogs={async (id, page) => {
        try {
          const result = await dbExecutions.getExecutionLogs(id, page, 500);
          setLogDrawerLogs(result.logs || []);
        } catch {
          // 加载失败时保持现有日志
        }
      }}
      runningTasks={{}}
    />
    </>
  );
}
