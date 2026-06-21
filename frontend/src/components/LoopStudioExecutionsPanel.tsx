// Loop 执行历史面板。
//
// 展示 loop 的运行历史, 每行是一次 execution (一次完整 loop 调用)：
// - id / 触发类型 / 开始时间 / 耗时 / 状态 / 阶段完成进度
// - 点击行展开看 step_executions 明细
//
// 分页: page + limit, 简单表格不引入分页器, 改成"加载更多"按钮避免侵入式 UI

import { useState, useEffect, useCallback, useMemo } from 'react';
import { App as AntApp, Button, Empty, Skeleton, Tag, Tooltip, Drawer, Descriptions, Pagination } from 'antd';
import {
  CheckCircleOutlined,
  CloseCircleOutlined,
  LoadingOutlined,
  MinusCircleOutlined,
  StarOutlined,
  ArrowRightOutlined,
  ReadOutlined,
} from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import type { LoopExecutionDto, LoopExecutionDetail, LoopExecutionTokenSummary } from '@/types/loop';
import { formatRelativeTime } from '@/utils/datetime';
import { useExecutionEvents } from '@/hooks/useExecutionEvents';

interface Props {
  loopId: number;
  loopName: string;
  onTotalChange?: (total: number) => void;
  onExecutionTrace?: (tracedStepIds: number[], sequenceMap: Record<number, number>) => void;
}

const DEFAULT_PAGE_LIMIT = 5;

// 状态 → 颜色 + 图标, 与 LoopListPanel.executionIcon 保持一致
// 兼容旧记录：旧版只支持步数限制，status = "capped" 视为步数超限。
function execStatusView(status: string): { color: string; icon: React.ReactNode; label: string } {
  switch (status) {
    case 'success': return { color: 'green', icon: <CheckCircleOutlined />, label: '成功' };
    case 'failed': return { color: 'red', icon: <CloseCircleOutlined />, label: '失败' };
    case 'partial': return { color: 'orange', icon: <CloseCircleOutlined />, label: '部分' };
    case 'running': return { color: 'blue', icon: <LoadingOutlined />, label: '运行中' };
    case 'cancelled': return { color: 'default', icon: <MinusCircleOutlined />, label: '已取消' };
    case 'capped':
    case 'capped_step': return { color: 'gold', icon: <MinusCircleOutlined />, label: '步数超限' };
    case 'capped_token': return { color: 'purple', icon: <MinusCircleOutlined />, label: 'Token 超限' };
    default: return { color: 'default', icon: <MinusCircleOutlined />, label: status };
  }
}

// 计算耗时
function durationLabel(start: string, end: string | null): string {
  if (!end) return '进行中';
  const ms = new Date(end).getTime() - new Date(start).getTime();
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.floor(ms / 60_000)}m ${Math.floor((ms % 60_000) / 1000)}s`;
}

// Token 格式化：千位分隔
function formatToken(n: number): string {
  return n.toLocaleString();
}

// Cost 格式化：美元小数
function formatCost(cost: number): string {
  if (cost < 0.01) return '$<0.01';
  return `$${cost.toFixed(4)}`;
}

// Token 汇总条组件：展示本次 loop execution 的 token 总消耗
function TokenSummaryBar({ summary }: { summary: LoopExecutionTokenSummary }) {
  // 只在有实际消耗时才显示
  const hasTokens = summary.total_input_tokens > 0 || summary.total_output_tokens > 0;
  if (!hasTokens && summary.total_cost_usd <= 0) return null;
  return (
    <div style={{
      display: 'flex',
      alignItems: 'center',
      gap: 8, flexWrap: 'wrap',
      padding: '8px 12px',
      marginBottom: 8,
      background: 'var(--color-bg-hover)',
      borderRadius: 8,
      border: '1px solid var(--color-border)',
      fontSize: 12,
    }}>
      <span style={{ fontWeight: 600, color: 'var(--color-text)' }}>Token 消耗汇总</span>
      <TokenBadge label="输入" value={`${formatToken(summary.total_input_tokens)}`} color="#1677ff" />
      <TokenBadge label="输出" value={`${formatToken(summary.total_output_tokens)}`} color="#52c41a" />
      {summary.total_cache_read_input_tokens > 0 && (
        <TokenBadge label="缓存读取" value={`${formatToken(summary.total_cache_read_input_tokens)}`} color="#722ed1" />
      )}
      {summary.total_cache_creation_input_tokens > 0 && (
        <TokenBadge label="缓存创建" value={`${formatToken(summary.total_cache_creation_input_tokens)}`} color="#eb2f96" />
      )}
      {summary.total_cost_usd > 0 && (
        <span style={{
          padding: '2px 6px', borderRadius: 4,
          background: 'var(--color-warning-bg)', color: 'var(--color-warning)',
          fontWeight: 600, fontSize: 12,
        }}>
          费用 {formatCost(summary.total_cost_usd)}
        </span>
      )}
    </div>
  );
}

// Token 徽章组件
function TokenBadge({ label, value, color }: { label: string; value: string; color: string }) {
  return (
    <span style={{
      display: 'inline-flex', alignItems: 'center', gap: 3,
      padding: '1px 6px', borderRadius: 4,
      background: `${color}0f`,
      fontSize: 11, fontWeight: 500, color,
    }}>
      {label}: {value}
    </span>
  );
}

export function LoopExecutionsPanel({ loopId, loopName: _loopName, onTotalChange, onExecutionTrace }: Props) {
  const { message } = AntApp.useApp();
  const [items, setItems] = useState<LoopExecutionDto[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(false);
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [expandedDetail, setExpandedDetail] = useState<LoopExecutionDetail | null>(null);
  const [expandedLoading, setExpandedLoading] = useState(false);
  // 黑板抽屉：用于展示该次执行中所有环节的结论摘要。
  // blackboardExecs 存储当前展开行的 step_executions 数组，
  // 按 sequence_index 排序后供 BlackboardDrawer 渲染。
  const [blackboardOpen, setBlackboardOpen] = useState(false);
  const [blackboardExecs, setBlackboardExecs] = useState<Record<string, any>[]>([]);

  // 打开黑板抽屉：传入当前展开行的 stepExecs，由用户点击「黑板」按钮触发。
  const handleOpenBlackboard = useCallback((stepExecs: Record<string, any>[]) => {
    setBlackboardExecs(stepExecs);
    setBlackboardOpen(true);
  }, []);

  // 打开黑板抽屉：点击行内的「黑板」按钮时，直接加载该次执行的详情并打开黑板抽屉。
  // 通过 stopPropagation 阻止冒泡到行的展开/收起。
  const handleOpenBlackboardForExec = useCallback(async (ev: React.MouseEvent, execId: number) => {
    ev.stopPropagation();
    try {
      const detail = await dbLoops.getExecution(loopId, execId);
      handleOpenBlackboard(detail.step_executions);
    } catch {
      message.error('加载黑板数据失败');
    }
  }, [loopId, handleOpenBlackboard, message]);

  // 加载一页执行记录
  const loadPage = useCallback((p: number) => {
    setLoading(true);
    dbLoops.listExecutions(loopId, { page: p, limit: DEFAULT_PAGE_LIMIT })
      .then((res) => {
        setItems(res.items);
        setTotal(res.total);
        onTotalChange?.(res.total);
        setPage(p);
      })
      .catch(() => {
        setItems([]);
      })
      .finally(() => setLoading(false));
  }, [loopId]);

  useEffect(() => { loadPage(1); }, [loadPage]);

  // 实际的刷新逻辑：拉取列表 + 展开详情
  const doRefresh = useCallback(() => {
    dbLoops.listExecutions(loopId, { page, limit: DEFAULT_PAGE_LIMIT })
      .then((res) => {
        setItems(res.items);
        setTotal(res.total);
        onTotalChange?.(res.total);
        if (expandedId !== null) {
          return dbLoops.getExecution(loopId, expandedId);
        }
        return null;
      })
      .then((detail) => {
        if (detail) {
          setExpandedDetail(detail);
        }
      })
      .catch(() => {});
  }, [page, loopId, expandedId]);

  // WebSocket 事件触发刷新（后端写入完成后才发事件，无需延迟）
  useExecutionEvents(useCallback(() => {
    doRefresh();
  }, [doRefresh]));

  // 展开行: 拉取该 execution 的 step 详情
  const handleExpand = useCallback(async (execId: number) => {
    if (expandedId === execId) {
      setExpandedId(null);
      setExpandedDetail(null);
      onExecutionTrace?.([], {});
      return;
    }
    setExpandedId(execId);
    setExpandedLoading(true);
    try {
      const detail = await dbLoops.getExecution(loopId, execId);
      setExpandedDetail(detail);
      // 提取轨迹：按 sequence_index 排序的 step_id 列表
      const sorted = [...detail.step_executions].sort(
        (a: any, b: any) => (a.sequence_index || 0) - (b.sequence_index || 0)
      );
      const tracedIds = sorted.map((s: any) => s.step_id);
      const seqMap: Record<number, number> = {};
      sorted.forEach((s: any) => {
        if (s.sequence_index != null) seqMap[s.step_id] = s.sequence_index;
      });
      onExecutionTrace?.(tracedIds, seqMap);
    } catch {
      message.error('加载执行详情失败');
      setExpandedId(null);
      onExecutionTrace?.([], {});
    } finally {
      setExpandedLoading(false);
    }
  }, [expandedId, loopId, message, onExecutionTrace]);


  return (
    <div className="loop-executions-panel">
      <div style={{ marginBottom: 12 }}>
        <span style={{ fontSize: 12, color: 'var(--color-text-tertiary, #94a3b8)' }}>
          共 {total} 条
        </span>
      </div>

      {loading && items.length === 0 ? (
        <Skeleton active />
      ) : items.length === 0 ? (
        <Empty description="暂无执行记录; 启用 loop 后触发一次即可看到" />
      ) : (
        <>
          {items.map(e => {
            const view = execStatusView(e.status);
            const expanded = expandedId === e.id;
            return (
              <div key={e.id} className="loop-exec-row" style={{ marginBottom: expanded ? 10 : 6 }}>
                <div
                  className="loop-exec-row-head"
                  onClick={() => handleExpand(e.id)}
                  style={{
                    cursor: 'pointer', padding: '10px 12px',
                    background: 'var(--color-bg-elevated, #ffffff)',
                    border: '1px solid var(--color-border, #e2e8f0)',
                    borderRadius: expanded ? '8px 8px 0 0' : 8,
                    borderBottom: expanded ? 'none' : '1px solid var(--color-border, #e2e8f0)',
                  }}
                >
                  {/* 第一行：图标 + 编号 + 状态 + 触发类型 */}
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6 }}>
                    {view.icon}
                    <span style={{ fontWeight: 600, fontSize: 14 }}>#{e.id}</span>
                    <Tag color={view.color}>{view.label}</Tag>
                    <Tag>{e.trigger_type}</Tag>
                    {e.failed_steps > 0 && (
                      <Tag color="red">{e.failed_steps} 失败</Tag>
                    )}
                    <Button
                      size="small"
                      type="text"
                      icon={<ReadOutlined />}
                      onClick={(ev) => { handleOpenBlackboardForExec(ev, e.id); }}
                      style={{ marginLeft: 'auto', fontSize: 12 }}
                    >
                      黑板
                    </Button>
                    <span style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)' }}>
                      {expanded ? '收起 ▲' : '展开 ▼'}
                    </span>
                  </div>
                  {/* 第二行：时间 + 进度 + 耗时 */}
                  <div style={{ display: 'flex', alignItems: 'center', gap: 12, fontSize: 12, color: 'var(--color-text-secondary, #475569)' }}>
                    <Tooltip title={`开始: ${e.started_at}`}>
                      <span>{formatRelativeTime(e.started_at)}</span>
                    </Tooltip>
                    <span>{e.completed_steps}/{e.total_steps} 环节</span>
                    <span style={{ fontFamily: 'monospace', color: 'var(--color-text-tertiary, #94a3b8)' }}>
                      耗时 {durationLabel(e.started_at, e.finished_at)}
                    </span>
                  </div>
                </div>
                {expanded && (
                  <div className="loop-exec-row-detail" style={{
                    background: 'var(--color-bg-elevated, #ffffff)',
                    border: '1px solid var(--color-border, #e2e8f0)',
                    borderTop: 'none',
                    borderRadius: '0 0 8px 8px',
                    padding: '8px 12px 12px',
                  }}>
                    {expandedLoading ? (
                      <Skeleton active />
                    ) : expandedDetail && expandedDetail.id === e.id ? (
                      <>
                        {/* Token 汇总条：展示本次 loop 执行的所有 token 消耗 */}
                        {expandedDetail.token_summary && (
                          <TokenSummaryBar summary={expandedDetail.token_summary} />
                        )}
                        <StepExecList stepExecs={expandedDetail.step_executions} />
                      </>
                    ) : null}
                  </div>
                )}
              </div>
            );
          })}
          {total > DEFAULT_PAGE_LIMIT && (
            <div style={{ textAlign: 'center', marginTop: 12 }}>
              <Pagination
                size="small"
                current={page}
                total={total}
                pageSize={DEFAULT_PAGE_LIMIT}
                onChange={(p) => loadPage(p)}
                showSizeChanger={false}
              />
            </div>
          )}
        </>
      )}

      {/* 黑板抽屉：按顺序展示该次执行中所有环节的结论，
          让用户一次性纵览整条执行链路的输出摘要，
          无需逐个展开每个环节的卡片查看 conclusion。 */}
      <BlackboardDrawer
        open={blackboardOpen}
        stepExecs={blackboardExecs}
        onClose={() => setBlackboardOpen(false)}
      />
    </div>
  );
}

// 黑板抽屉组件：以列表形式展示该次执行中所有环节的结论。
// 按 sequence_index 排序，每个环节以卡片形式展示：
// - 标题行：序号 + 环节名称
// - 结论内容（markdown 区域）
// 没有结论的环节展示「-无结论-」占位。
function BlackboardDrawer({ open, stepExecs, onClose }: {
  open: boolean;
  stepExecs: Record<string, any>[];
  onClose: () => void;
}) {
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
      destroyOnClose
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
            {/* 标题行：序号 + 环节名称 + 状态标记 */}
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
                {s.step_name || `环节 #${s.step_id}`}
              </span>
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
  );
}

// 环节执行卡片：卡片式布局 + 箭头连接展示执行顺序，每张卡展示执行详情
function StepExecList({ stepExecs }: { stepExecs: Record<string, any>[] }) {
  const [drawerRecord, setDrawerRecord] = useState<any | null>(null);
  const [drawerLoading, setDrawerLoading] = useState(false);

  const handleCardClick = useCallback(async (s: any) => {
    if (!s.execution_record_id) return;
    setDrawerLoading(true);
    try {
      const { getExecutionRecord } = await import('@/utils/database/executions');
      const rec = await getExecutionRecord(s.execution_record_id);
      setDrawerRecord(rec);
    } catch {
      // ignore
    } finally {
      setDrawerLoading(false);
    }
  }, []);

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
                {s.step_name || `环节 #${s.step_id}`}
              </div>

              {/* 状态 + 耗时 */}
              <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 6 }}>
                {view.icon}
                <Tag color={view.color} style={{ margin: 0, fontSize: 11 }}>{s.status}</Tag>
                <span style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)', fontFamily: 'monospace' }}>{duration}</span>
              </div>

              {/* 评分 / 阈值 */}
              <div style={{ display: 'flex', alignItems: 'center', gap: 4, flexWrap: 'wrap', marginBottom: 4 }}>
                {s.rating != null ? (
                  <span style={{
                    display: 'inline-flex', alignItems: 'center', gap: 2,
                    padding: '1px 6px', borderRadius: 4, fontSize: 12,
                    background: ratingPassed ? 'var(--color-success-bg, #f0fdf4)' : 'var(--color-error-bg, #fef2f2)',
                    color: ratingPassed ? 'var(--color-success, #22c55e)' : 'var(--color-error, #ef4444)',
                    fontWeight: 600,
                  }}>
                    <StarOutlined style={{ fontSize: 10 }} /> {s.rating}
                  </span>
                ) : (
                  <span style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)' }}>未评审</span>
                )}
                {s.min_rating != null && (
                  <span style={{ fontSize: 11, color: s.rating != null && s.rating >= s.min_rating ? 'var(--color-success, #22c55e)' : 'var(--color-error, #ef4444)' }}>
                    {s.rating != null && (s.rating >= s.min_rating ? '✅' : '❌')} {s.min_rating}
                  </span>
                )}
              </div>

              {/* Token 消耗：从 execution_record.usage 解析 */}
              {(s.input_tokens != null || s.output_tokens != null) && (
                <div style={{
                  display: 'flex', alignItems: 'center', gap: 4, flexWrap: 'wrap',
                  marginTop: 4, marginBottom: 4,
                }}>
                  {s.input_tokens != null && (
                    <span style={{
                      padding: '1px 5px', borderRadius: 3,
                      background: 'var(--color-info-bg)', fontSize: 10, color: 'var(--color-info)',
                      fontWeight: 500, fontFamily: 'monospace',
                    }}>
                      i{formatToken(s.input_tokens)}
                    </span>
                  )}
                  {s.output_tokens != null && (
                    <span style={{
                      padding: '1px 5px', borderRadius: 3,
                      background: 'var(--color-success-bg)', fontSize: 10, color: 'var(--color-success)',
                      fontWeight: 500, fontFamily: 'monospace',
                    }}>
                      o{formatToken(s.output_tokens)}
                    </span>
                  )}
                  {s.cache_read_input_tokens != null && s.cache_read_input_tokens > 0 && (
                    <span style={{
                      padding: '1px 5px', borderRadius: 3,
                      background: 'var(--color-info-bg)', fontSize: 10, color: 'var(--color-primary)',
                      fontWeight: 500, fontFamily: 'monospace',
                    }}>
                      cr{formatToken(s.cache_read_input_tokens)}
                    </span>
                  )}
                  {s.total_cost_usd != null && s.total_cost_usd > 0 && (
                    <span style={{
                      padding: '1px 5px', borderRadius: 3,
                      background: 'var(--color-warning-bg)', fontSize: 10, color: 'var(--color-warning)',
                      fontWeight: 500, fontFamily: 'monospace',
                    }}>
                      {formatCost(s.total_cost_usd)}
                    </span>
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

              {/* 错误 */}
              {s.error_message && (
                <div style={{ marginTop: 4, fontSize: 11, color: 'var(--color-error, #ef4444)', lineHeight: 1.4 }}>
                  {s.error_message}
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
      width={480}
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
        </div>
      )}
    </Drawer>
    </>
  );
}
