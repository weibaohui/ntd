// 黑板抽屉组件：以列表形式展示该次执行中所有环节的结论。
// 按 sequence_index 排序，每个环节以卡片形式展示：
// - 标题行：序号 + 环节名称 + 执行记录 ID（可点击查看详情） + 状态标记
// - 结论内容（markdown 区域）
// 没有结论的环节展示「-无结论-」占位。
//
// 点击执行记录 ID（#1234）时弹出执行记录详情 Drawer，
// 复用与 StepExecList 相同的详情展示（Descriptions + 执行结果），
// 不跳转页面，不丢失黑板上下文。

import { useState, useCallback, useMemo } from 'react';
import { Drawer, Descriptions, Tag, Button } from 'antd';
import { ReadOutlined } from '@ant-design/icons';
import * as dbExecutions from '@/utils/database/executions';
import type { LogEntry } from '@/types';
import { LogDrawer } from '@/components/TodoPostPage';
import { execStatusView } from './helpers';

interface BlackboardDrawerProps {
  open: boolean;
  stepExecs: Record<string, any>[];
  onClose: () => void;
}

export function BlackboardDrawer({ open, stepExecs, onClose }: BlackboardDrawerProps) {
  // ── 执行记录详情 Drawer 状态 ────────────────────────────
  const [detailRecord, setDetailRecord] = useState<any | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);

  // 点击 #ID 时加载记录并弹出详情 Drawer，与 StepExecList 的交互一致
  const handleOpenDetail = useCallback(async (executionRecordId: number) => {
    setDetailLoading(true);
    try {
      const record = await dbExecutions.getExecutionRecord(executionRecordId);
      setDetailRecord(record);
    } catch {
      // 加载失败不弹窗
    } finally {
      setDetailLoading(false);
    }
  }, []);

  // ── 执行日志 Drawer 状态（第三层下探） ─────────────────
  // 在详情 Drawer 中点击"查看日志"时弹出一个带 Segmented 三视图（日志/对话/命令）的 Drawer
  const [logDrawerRecord, setLogDrawerRecord] = useState<any | null>(null);
  const [logDrawerLogs, setLogDrawerLogs] = useState<LogEntry[]>([]);

  // 加载日志并打开日志 Drawer
  const handleOpenLogView = useCallback(async (record: any) => {
    setLogDrawerRecord(record);
    try {
      const result = await dbExecutions.getExecutionLogs(record.id, 1, 500);
      setLogDrawerLogs(result.logs || []);
    } catch {
      setLogDrawerLogs([]);
    }
  }, []);

  // 按 sequence_index 排序，确保展示顺序与执行链一致
  const sorted = useMemo(
    () => [...stepExecs].sort((a, b) => (a.sequence_index || 0) - (b.sequence_index || 0)),
    [stepExecs],
  );

  // 统计有结论的环节数，在标题栏展示
  const conclusionCount = useMemo(
    () => sorted.filter(s => s.conclusion).length,
    [sorted],
  );

  return (
    <>
      <Drawer
        title={
          <span>
            <ReadOutlined style={{ marginRight: 8 }} />
            黑板 · {sorted.length} 个环节
            <span style={{ marginLeft: 8, fontSize: 12, color: 'var(--color-text-tertiary, #94a3b8)', fontWeight: 400 }}>
              {conclusionCount} 个有结论
            </span>
          </span>
        }
        placement="right"
        width={520}
        open={open}
        onClose={onClose}
      >
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          {sorted.map((s, idx) => (
            <div
              key={s.id || idx}
              style={{
                background: 'var(--color-bg-elevated, #ffffff)',
                border: '1px solid var(--color-border, #e2e8f0)',
                borderRadius: 8,
                padding: '12px 14px',
              }}
            >
              {/* 标题行：序号 + 环节名称 + 执行记录 ID + 状态标记 */}
              <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 8 }}>
                <span style={{
                  width: 22, height: 22, borderRadius: 11,
                  background: 'var(--color-primary, #0891b2)',
                  color: '#fff', fontSize: 11, fontWeight: 700,
                  display: 'flex', alignItems: 'center', justifyContent: 'center',
                  fontFamily: 'monospace',
                }}>
                  {idx + 1}
                </span>
                <span style={{ fontWeight: 600, fontSize: 13, color: 'var(--color-text, #0f172a)', flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {s.step_name || `环节 #${s.loop_step_id}`}
                </span>
                {/* 执行记录 ID：可点击查看详情，弹出与 StepExecList 相同的记录详情 Drawer */}
                {s.execution_record_id != null && (
                  <span
                    onClick={(e) => {
                      e.stopPropagation();
                      handleOpenDetail(s.execution_record_id);
                    }}
                    title={`查看执行记录 #${s.execution_record_id} 详情`}
                    style={{
                      fontSize: 10, color: 'var(--color-primary, #0891b2)',
                      fontFamily: 'monospace', whiteSpace: 'nowrap',
                      border: '1px solid var(--color-primary, #0891b2)',
                      borderRadius: 4, padding: '0 5px', lineHeight: '18px',
                      cursor: 'pointer', userSelect: 'none',
                    }}
                  >
                    #{s.execution_record_id}
                  </span>
                )}
                {s.status && (
                  <Tag color={execStatusView(s.status).color} style={{ margin: 0, fontSize: 10, lineHeight: '16px' }}>
                    {execStatusView(s.status).label}
                  </Tag>
                )}
              </div>
              {/* 结论内容 */}
              {s.conclusion ? (
                <div style={{
                  fontSize: 13, color: 'var(--color-text-secondary, #475569)',
                  background: 'var(--color-bg-hover, #f1f5f9)',
                  padding: 10, borderRadius: 6, whiteSpace: 'pre-wrap',
                  lineHeight: 1.6, maxHeight: 200, overflow: 'auto',
                }}>
                  {s.conclusion}
                </div>
              ) : (
                <div style={{ fontSize: 12, color: 'var(--color-text-tertiary, #94a3b8)', fontStyle: 'italic' }}>
                  - 无结论 -
                </div>
              )}
              {/* 如果该环节有错误，在结论下方展示错误信息 */}
              {s.error_message && (
                <div style={{
                  marginTop: 6, fontSize: 12, color: 'var(--color-error, #ef4444)',
                  lineHeight: 1.5, maxHeight: 60, overflow: 'auto',
                }}>
                  {s.error_message}
                </div>
              )}
            </div>
          ))}
          {sorted.length === 0 && (
            <div style={{ padding: 40, textAlign: 'center', color: 'var(--color-text-tertiary, #94a3b8)' }}>
              暂无环节数据
            </div>
          )}
        </div>
      </Drawer>

      {/* 执行记录详情 Drawer：点击 #ID 时弹出，复用与 StepExecList 相同的详情展示。
           与黑板 Drawer 是同级兄弟节点，不会嵌套。 */}
      <Drawer
        title={detailRecord ? `执行记录 #${detailRecord.id}` : '执行记录'}
        placement="right"
        width={520}
        open={!!detailRecord}
        onClose={() => setDetailRecord(null)}
        loading={detailLoading}
      >
        {detailRecord && (
          <>
            <Descriptions column={1} size="small" bordered={false} style={{ marginBottom: 16 }}>
              <Descriptions.Item label="状态">
                <Tag color={detailRecord.status === 'success' ? 'green' : detailRecord.status === 'failed' ? 'red' : 'blue'}>
                  {detailRecord.status}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label="执行器">{detailRecord.executor || '-'}</Descriptions.Item>
              {detailRecord.rating != null && (
                <Descriptions.Item label="评分">{detailRecord.rating} / 100</Descriptions.Item>
              )}
              <Descriptions.Item label="开始时间">{detailRecord.started_at || '-'}</Descriptions.Item>
              <Descriptions.Item label="结束时间">{detailRecord.finished_at || '-'}</Descriptions.Item>
              {detailRecord.command && (
                <Descriptions.Item label="命令"><code style={{ wordBreak: 'break-all' }}>{detailRecord.command}</code></Descriptions.Item>
              )}
            </Descriptions>
            {detailRecord.result && (
              <div>
                <div style={{ fontWeight: 600, marginBottom: 8, fontSize: 14 }}>执行结果</div>
                <div style={{
                  background: 'var(--color-bg-hover)',
                  padding: 12, borderRadius: 6, fontSize: 13,
                  whiteSpace: 'pre-wrap', lineHeight: 1.6, maxHeight: 400, overflow: 'auto',
                }}>
                  {detailRecord.result}
                </div>
                <Button
                  type="link"
                  size="small"
                  icon={<ReadOutlined />}
                  onClick={() => handleOpenLogView(detailRecord)}
                  style={{ marginTop: 6, padding: 0, height: 'auto' }}
                >
                  查看日志
                </Button>
              </div>
            )}
          </>
        )}
      </Drawer>

      {/* 执行详情 Drawer：第三层下探，在详情 Drawer 中点击"查看日志"按钮时弹出。
           复用 TodoPostPage 中的 LogDrawer 组件，和执行历史页面的"详情"按钮弹出内容完全一致。 */}
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
