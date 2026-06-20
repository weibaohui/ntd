// Loop 执行历史面板。
//
// 展示 loop 的运行历史, 每行是一次 execution (一次完整 loop 调用)：
// - id / 触发类型 / 开始时间 / 耗时 / 状态 / 阶段完成进度
// - 点击行展开看 step_executions 明细
//
// 分页: page + limit, 简单表格不引入分页器, 改成"加载更多"按钮避免侵入式 UI

import { useState, useEffect, useCallback } from 'react';
import { App as AntApp, Button, Empty, Skeleton, Tag, Space, Tooltip } from 'antd';
import {
  CheckCircleOutlined,
  CloseCircleOutlined,
  LoadingOutlined,
  MinusCircleOutlined,
  ReloadOutlined,
  HistoryOutlined,
  StarOutlined,
  ArrowRightOutlined,
} from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import type { LoopExecutionDto, LoopExecutionDetail } from '@/types/loop';
import { formatRelativeTime } from '@/utils/datetime';

interface Props {
  loopId: number;
  loopName: string;
}

const DEFAULT_PAGE_LIMIT = 20;

// 状态 → 颜色 + 图标, 与 LoopListPanel.executionIcon 保持一致
function execStatusView(status: string): { color: string; icon: React.ReactNode; label: string } {
  switch (status) {
    case 'success': return { color: 'green', icon: <CheckCircleOutlined />, label: '成功' };
    case 'failed': return { color: 'red', icon: <CloseCircleOutlined />, label: '失败' };
    case 'partial': return { color: 'orange', icon: <CloseCircleOutlined />, label: '部分' };
    case 'running': return { color: 'blue', icon: <LoadingOutlined />, label: '运行中' };
    case 'cancelled': return { color: 'default', icon: <MinusCircleOutlined />, label: '已取消' };
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

export function LoopExecutionsPanel({ loopId, loopName }: Props) {
  const { message } = AntApp.useApp();
  const [items, setItems] = useState<LoopExecutionDto[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(false);
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [expandedDetail, setExpandedDetail] = useState<LoopExecutionDetail | null>(null);
  const [expandedLoading, setExpandedLoading] = useState(false);

  // 加载一页执行记录
  const loadPage = useCallback((p: number) => {
    setLoading(true);
    dbLoops.listExecutions(loopId, { page: p, limit: DEFAULT_PAGE_LIMIT })
      .then((res) => {
        if (p === 1) {
          setItems(res.items);
        } else {
          setItems(prev => [...prev, ...res.items]);
        }
        setTotal(res.total);
        setPage(p);
      })
      .catch(() => {
        if (p === 1) setItems([]);
      })
      .finally(() => setLoading(false));
  }, [loopId]);

  useEffect(() => { loadPage(1); }, [loadPage]);

  // 展开行: 拉取该 execution 的 step 详情
  const handleExpand = useCallback(async (execId: number) => {
    if (expandedId === execId) {
      setExpandedId(null);
      setExpandedDetail(null);
      return;
    }
    setExpandedId(execId);
    setExpandedLoading(true);
    try {
      const detail = await dbLoops.getExecution(loopId, execId);
      setExpandedDetail(detail);
    } catch {
      message.error('加载执行详情失败');
      setExpandedId(null);
    } finally {
      setExpandedLoading(false);
    }
  }, [expandedId, loopId, message]);

  // 加载更多
  const handleLoadMore = useCallback(() => {
    if (items.length < total && !loading) {
      loadPage(page + 1);
    }
  }, [items.length, total, loading, page, loadPage]);

  return (
    <div className="loop-executions-panel">
      <div style={{ marginBottom: 12, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <Space>
          <HistoryOutlined />
          <span style={{ color: 'var(--color-text-secondary, #475569)' }}>{loopName} 的执行历史 (共 {total} 条)</span>
        </Space>
        <Button size="small" icon={<ReloadOutlined />} onClick={() => loadPage(1)}>
          刷新
        </Button>
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
              <div key={e.id} className="loop-exec-row">
                <div
                  className="loop-exec-row-head"
                  onClick={() => handleExpand(e.id)}
                  style={{ cursor: 'pointer', padding: '10px 12px', borderRadius: 8, border: '1px solid var(--color-border, #e2e8f0)', marginBottom: 6, background: 'var(--color-bg-elevated, #ffffff)' }}
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
                    {/* 展开/收起提示 */}
                    <span style={{ marginLeft: 'auto', fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)' }}>
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
                  <div className="loop-exec-row-detail">
                    {expandedLoading ? (
                      <Skeleton active />
                    ) : expandedDetail && expandedDetail.id === e.id ? (
                      <StepExecList stepExecs={expandedDetail.step_executions} />
                    ) : null}
                  </div>
                )}
              </div>
            );
          })}
          {items.length < total && (
            <div style={{ textAlign: 'center', marginTop: 12 }}>
              <Button onClick={handleLoadMore} loading={loading}>
                加载更多 ({items.length}/{total})
              </Button>
            </div>
          )}
        </>
      )}
    </div>
  );
}

// 环节执行卡片：卡片式布局 + 箭头连接展示执行顺序，每张卡展示执行详情
function StepExecList({ stepExecs }: { stepExecs: Record<string, any>[] }) {
  if (stepExecs.length === 0) {
    return <Empty description="无环节执行记录" />;
  }
  return (
    <div style={{ display: 'flex', gap: 0, overflowX: 'auto', paddingBottom: 8, alignItems: 'stretch' }}>
      {stepExecs.map((s, idx) => {
        const view = execStatusView(s.status);
        const ratingPassed = s.rating != null && s.min_rating != null && s.rating >= s.min_rating;
        const duration = s.started_at ? durationLabel(s.started_at, s.finished_at) : '-';
        return (
          <div key={s.id} style={{ display: 'flex', alignItems: 'center', gap: 0 }}>
            {/* 箭头连接（第一项前不显示） */}
            {idx > 0 && (
              <div style={{ display: 'flex', alignItems: 'center', padding: '0 4px' }}>
                <ArrowRightOutlined style={{ color: 'var(--color-text-tertiary, #94a3b8)', fontSize: 16 }} />
              </div>
            )}

            {/* 执行卡片 */}
            <div
              style={{
                position: 'relative',
                width: 220, minWidth: 220,
                background: 'var(--color-bg-elevated, #ffffff)',
                border: `1px solid ${s.status === 'success' ? 'var(--color-success, #22c55e)' : s.status === 'failed' ? 'var(--color-error, #ef4444)' : 'var(--color-border, #e2e8f0)'}`,
                borderRadius: 10,
                padding: '14px 16px',
                transition: 'box-shadow 200ms',
                userSelect: 'none',
              }}
            >
              {/* 执行序号 */}
              <div style={{ position: 'absolute', top: 6, left: 10, fontSize: 11, fontWeight: 600, color: 'var(--color-text-tertiary, #94a3b8)' }}>
                #{idx + 1}
              </div>

              {/* 状态指示圆点 */}
              <div style={{ position: 'absolute', top: 8, right: 10, width: 8, height: 8, borderRadius: 4, background: view.color }} />

              {/* 环节名称 */}
              <div style={{ fontSize: 14, fontWeight: 600, color: 'var(--color-text, #0f172a)', marginBottom: 2, marginTop: 4, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                环节 #{s.step_id}
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

              {/* 策略 */}
              {s.unrated_policy && (
                <div style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)', marginBottom: 2 }}>
                  策略: {s.unrated_policy === 'skip' ? '未达标跳过' : '未达标放行'}
                </div>
              )}

              {/* 时间 */}
              <div style={{ fontSize: 10, color: 'var(--color-text-tertiary, #94a3b8)', lineHeight: 1.5 }}>
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
  );
}
