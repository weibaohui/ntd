// Loop 执行历史面板。
//
// 展示 loop 的运行历史, 每行是一次 execution (一次完整 loop 调用)：
// - id / 触发类型 / 开始时间 / 耗时 / 状态 / 阶段完成进度
// - 点击行展开看 step_executions 明细
//
// 分页: page + limit, 简单表格不引入分页器, 改成"加载更多"按钮避免侵入式 UI
//
// 子组件 TokenSummaryBar / BlackboardDrawer / StepExecList 仅在本目录内自用，
// 外部 caller 需要时直接 import 对应文件，不再 re-export。

import { useState, useEffect, useCallback, useRef } from 'react';
import { App as AntApp, Button, Empty, Skeleton, Tag, Tooltip, Pagination } from 'antd';
import {
  ReadOutlined,
  ExclamationCircleOutlined,
} from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import type { LoopExecutionDto, LoopExecutionDetail } from '@/types/loop';
import { formatRelativeTime } from '@/utils/datetime';
import { useExecutionEvents } from '@/hooks/useExecutionEvents';
import { execStatusView, durationLabel } from './helpers';
import { TokenSummaryBar } from './TokenSummaryBar';
import { BlackboardDrawer } from './BlackboardDrawer';
import { StepExecList } from './StepExecList';

interface Props {
  loopId: number;
  /** 当前工作空间 ID（v1 路由 workspace-scoped） */
  workspaceId: number | null;
  loopName: string;
  onTotalChange?: (total: number) => void;
  onExecutionTrace?: (tracedStepIds: number[], sequenceMap: Record<number, number>) => void;
  /**
   * 063：首屏加载后自动展开第一条 pending_approval_count>0 的执行行，
   * 供任务列表「待审批」标记跳转后一步到位看到审批按钮。
   * 仅初始触发一次；缺省 false，Loop 侧既有调用点行为不变。
   */
  autoExpandFirstPending?: boolean;
}

const DEFAULT_PAGE_LIMIT = 5;

export function LoopExecutionsPanel({ loopId, workspaceId, loopName: _loopName, onTotalChange, onExecutionTrace, autoExpandFirstPending }: Props) {
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
      const detail = await dbLoops.getExecution(workspaceId ?? 0, loopId, execId);
      handleOpenBlackboard(detail.step_executions);
    } catch {
      message.error('加载黑板数据失败');
    }
  }, [loopId, workspaceId, handleOpenBlackboard, message]);

  // 063 自动展开守卫：ref 而非 state——置位不需要触发重渲染，
  // 且能保证「仅首屏触发一次」，WebSocket 刷新（doRefresh）不再自动展开打扰用户。
  const autoExpandedRef = useRef(false);

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
      const detail = await dbLoops.getExecution(workspaceId ?? 0, loopId, execId);
      setExpandedDetail(detail);
      // 提取轨迹：按 sequence_index 排序的 loop_step_id 列表
      const sorted = [...detail.step_executions].sort(
        (a: any, b: any) => (a.sequence_index || 0) - (b.sequence_index || 0)
      );
      const tracedIds = sorted.map((s: any) => s.loop_step_id);
      const seqMap: Record<number, number> = {};
      sorted.forEach((s: any) => {
        if (s.sequence_index != null) seqMap[s.loop_step_id] = s.sequence_index;
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

  // 加载一页执行记录
  const loadPage = useCallback((p: number) => {
    setLoading(true);
    dbLoops.listExecutions(workspaceId ?? 0, loopId, { page: p, limit: DEFAULT_PAGE_LIMIT })
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
    // 依赖刻意不含 handleExpand：handleExpand 随 expandedId 变化会重建，
    // 若被 loadPage 依赖，展开行会连带触发 loadPage(1) 把列表重置回第 1 页（回归）。
  }, [loopId, workspaceId]);

  useEffect(() => { loadPage(1); }, [loadPage]);

  // 063：首屏数据到达后自动展开首条待审批执行，让「待审批」跳转一步到位看到审批按钮。
  // 独立 effect 监听 items 而非写进 loadPage.then：避免 loadPage 依赖 handleExpand 引发的上述回归。
  // autoExpandedRef 守卫保证仅触发一次——WebSocket 刷新更新 items 时不会重复展开/收起打扰用户。
  // 仅在当前页（首页）内查找：待审批执行按时间倒序大概率在首页，不跨页拉取（YAGNI）。
  useEffect(() => {
    if (!autoExpandFirstPending || autoExpandedRef.current) return;
    const firstPending = items.find((e) => e.pending_approval_count > 0);
    if (firstPending) {
      // 先置守卫再异步展开：防止展开过程中 items 被刷新再次命中本分支。
      autoExpandedRef.current = true;
      handleExpand(firstPending.id);
    }
  }, [items, autoExpandFirstPending, handleExpand]);

  // 实际的刷新逻辑：拉取列表 + 展开详情
  const doRefresh = useCallback(() => {
    dbLoops.listExecutions(workspaceId ?? 0, loopId, { page, limit: DEFAULT_PAGE_LIMIT })
      .then((res) => {
        setItems(res.items);
        setTotal(res.total);
        onTotalChange?.(res.total);
        if (expandedId !== null) {
          return dbLoops.getExecution(workspaceId ?? 0, loopId, expandedId);
        }
        return null;
      })
      .then((detail) => {
        if (detail) {
          setExpandedDetail(detail);
        }
      })
      .catch(() => {});
  }, [page, loopId, workspaceId, expandedId]);

  // WebSocket 事件触发刷新（后端写入完成后才发事件，无需延迟）
  useExecutionEvents(useCallback(() => {
    doRefresh();
  }, [doRefresh]));


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
                    {e.pending_approval_count > 0 && (
                      <Tag color="red" style={{ fontWeight: 600 }}>
                        <ExclamationCircleOutlined /> {e.pending_approval_count} 待审批
                      </Tag>
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
                  {/* 第三行：错误说明（仅在 status=failed 且有 error_message 时显示） */}
                  {e.status === 'failed' && e.error_message && (
                    <div style={{
                      marginTop: 6, padding: '4px 8px',
                      background: 'var(--color-error-bg, #fff1f0)',
                      border: '1px solid var(--color-error-border, #ffccc7)',
                      borderRadius: 4,
                      fontSize: 12, color: 'var(--color-error-text, #cf1322)',
                      lineHeight: 1.5, whiteSpace: 'pre-wrap',
                    }}>
                      {e.error_message}
                    </div>
                  )}
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
                        <StepExecList stepExecs={expandedDetail.step_executions} loopId={loopId} workspaceId={workspaceId ?? 0} executionId={expandedDetail.id} onApproved={() => loadPage(page)} />
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
          无需逐个展开每个环节的卡片查看结论。 */}
      <BlackboardDrawer
        open={blackboardOpen}
        stepExecs={blackboardExecs}
        workspaceId={workspaceId ?? 0}
        onClose={() => setBlackboardOpen(false)}
      />
    </div>
  );
}
