// LoopKanban 主组件：环路执行看板。
//
// 子组件（ExecutionCard / KanbanColumn / useLoopExecutions / helpers）
// 在本目录内自用，外部 caller 直接 import 本文件即可。

import { useState, useMemo, useCallback } from 'react';
import { Drawer, Spin, Empty, Divider, App as AntApp } from 'antd';
import {
  ReadOutlined,
  ApartmentOutlined,
  HistoryOutlined,
} from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import { useApp } from '@/hooks/useApp';
import type { LoopExecutionDetail, LoopDetail } from '@/types/loop';
import { StepExecList } from '@/components/loop-studio/executions/StepExecList';
import { BlackboardDrawer } from '@/components/loop-studio/executions/BlackboardDrawer';
import { LoopFlowGraph } from '@/components/loop-flow/LoopFlowGraph';
import { KanbanColumn } from './KanbanColumn';
import { ExecutionCard } from './ExecutionCard';
import { useLoopExecutions, type LoopExecutionWithLoopName } from './useLoopExecutions';
import { COLUMNS, execStatusView } from './helpers';

interface Props {
  searchText?: string;
  hours?: number;
  onSearchChange?: (v: string) => void;
  onHoursChange?: (h: number) => void;
}

export function LoopKanban({ searchText: externalSearch, hours: externalHours, onSearchChange: _onSearchChange, onHoursChange: _onHoursChange }: Props = {}) {
  const [internalSearch] = useState('');
  const [internalHours] = useState(24);
  const searchText = externalSearch ?? internalSearch;
  const hours = externalHours ?? internalHours;

  const { message } = AntApp.useApp();
  const { state } = useApp();

  const { executions, loading } = useLoopExecutions(state.selectedWorkspace ?? null, hours);
  const wsId = state.selectedWorkspace ?? 0;

  // ── 轨迹侧边栏状态 ────────────────────────────────────
  const [selectedExec, setSelectedExec] = useState<LoopExecutionWithLoopName | null>(null);
  const [execDetail, setExecDetail] = useState<LoopExecutionDetail | null>(null);
  const [loopDetail, setLoopDetail] = useState<LoopDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);

  const handleCardClick = useCallback(async (exec: LoopExecutionWithLoopName) => {
    setSelectedExec(exec);
    setDrawerOpen(true);
    setDetailLoading(true);
    setExecDetail(null);
    setLoopDetail(null);
    try {
      const [detail, loop] = await Promise.all([
        dbLoops.getExecution(wsId, exec.loop_id, exec.id),
        dbLoops.getLoop(wsId, exec.loop_id),
      ]);
      setExecDetail(detail);
      setLoopDetail(loop);
    } catch {
      message.error('加载执行轨迹失败');
    } finally {
      setDetailLoading(false);
    }
  }, [message]);

  // ── 黑板抽屉状态 ────────────────────────────────────
  const [blackboardOpen, setBlackboardOpen] = useState(false);
  const [blackboardExecs, setBlackboardExecs] = useState<Record<string, any>[]>([]);

  const handleOpenBlackboard = useCallback(async (exec: LoopExecutionWithLoopName) => {
    try {
      const detail = await dbLoops.getExecution(wsId, exec.loop_id, exec.id);
      setBlackboardExecs(detail.step_executions);
      setBlackboardOpen(true);
    } catch {
      message.error('加载黑板数据失败');
    }
  }, [message]);

  // 按时间窗口过滤
  const timeFiltered = useMemo(() => {
    const cutoff = hours ? Date.now() - hours * 3600 * 1000 : 0;
    if (cutoff === 0) return executions;
    return executions.filter(e => {
      const t = new Date(e.started_at).getTime();
      return t >= cutoff;
    });
  }, [executions, hours]);

  // 按搜索关键词过滤
  const filtered = useMemo(() => {
    if (!searchText.trim()) return timeFiltered;
    const q = searchText.toLowerCase();
    return timeFiltered.filter(e =>
      e.loop_name.toLowerCase().includes(q) ||
      e.trigger_type.toLowerCase().includes(q)
    );
  }, [timeFiltered, searchText]);

  // 按状态分组到看板列
  const grouped = useMemo(() => {
    const map: Record<string, LoopExecutionWithLoopName[]> = {};
    for (const col of COLUMNS) map[col.status] = [];
    for (const exec of filtered) {
      if (map[exec.status]) {
        map[exec.status].push(exec);
      } else {
        const lastCol = COLUMNS[COLUMNS.length - 1];
        map[lastCol.status].push(exec);
      }
    }
    return map;
  }, [filtered]);

  // 渲染单个执行卡片
  const renderCard = useCallback((exec: LoopExecutionWithLoopName) => {
    const view = execStatusView(exec.status);
    return (
      <ExecutionCard
        key={`${exec.loop_id}-${exec.id}`}
        exec={exec}
        view={view}
        onClick={handleCardClick}
        onBlackboard={handleOpenBlackboard}
      />
    );
  }, [handleCardClick, handleOpenBlackboard]);

  // 渲染看板列
  const renderColumn = (col: typeof COLUMNS[number]) => {
    const items = grouped[col.status] ?? [];
    return <KanbanColumn key={col.status} col={col} items={items} renderCard={renderCard} />;
  };

  return (
    <div className="loop-kanban-board">
      {loading ? (
        <div style={{ textAlign: 'center', padding: 60 }}>
          <Spin tip="加载环路执行历史…" />
        </div>
      ) : filtered.length === 0 ? (
        <Empty description="暂无环路执行记录" style={{ padding: 60 }} />
      ) : (
        <div className="loop-kanban-columns-container">
          {COLUMNS.map(renderColumn)}
        </div>
      )}

      {/* 执行轨迹侧边栏 */}
      <Drawer
        title={
          selectedExec ? (
            <span>
              <ReadOutlined style={{ marginRight: 8 }} />
              执行轨迹 · {selectedExec.loop_name}
              <span style={{ marginLeft: 8, fontSize: 12, color: 'var(--color-text-tertiary)', fontWeight: 400 }}>
                #{selectedExec.id}
              </span>
            </span>
          ) : '执行轨迹'
        }
        placement="right"
        width={640}
        open={drawerOpen}
        onClose={() => {
          setDrawerOpen(false);
          setSelectedExec(null);
          setExecDetail(null);
          setLoopDetail(null);
        }}
        destroyOnClose
      >
        {detailLoading ? (
          <div style={{ textAlign: 'center', padding: 40 }}>
            <Spin tip="加载执行轨迹…" />
          </div>
        ) : execDetail && selectedExec && loopDetail ? (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
            <div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 8 }}>
                <ApartmentOutlined style={{ color: 'var(--color-primary, #0891b2)' }} />
                <span style={{ fontWeight: 600, fontSize: 14, color: 'var(--color-text, #0f172a)' }}>
                  环节设计
                </span>
                <span style={{ fontSize: 11, color: 'var(--color-text-tertiary, #94a3b8)' }}>
                  {loopDetail.steps.length} 个环节
                </span>
              </div>
              <div style={{
                background: 'var(--color-bg-elevated, #ffffff)',
                border: '1px solid var(--color-border, #e2e8f0)',
                borderRadius: 8,
                padding: '8px 12px',
              }}>
                <LoopFlowGraph
                  steps={loopDetail.steps}
                  selectedStepId={null}
                  onSelectStep={() => {}}
                  onAddStep={() => {}}
                />
              </div>
            </div>

            <Divider style={{ margin: '4px 0' }} />

            <div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 8 }}>
                <HistoryOutlined style={{ color: 'var(--color-primary, #0891b2)' }} />
                <span style={{ fontWeight: 600, fontSize: 14, color: 'var(--color-text, #0f172a)' }}>
                  执行轨迹
                </span>
              </div>
              <StepExecList
                stepExecs={execDetail.step_executions}
                loopId={execDetail.loop_id}
                workspaceId={wsId}
                executionId={execDetail.id}
                onApproved={() => {
                  dbLoops.getExecution(wsId, selectedExec.loop_id, selectedExec.id)
                    .then(setExecDetail)
                    .catch(() => {});
                }}
              />
            </div>
          </div>
        ) : (
          <Empty description="无执行轨迹数据" />
        )}
      </Drawer>

      {/* 黑板抽屉 */}
      <BlackboardDrawer
        open={blackboardOpen}
        stepExecs={blackboardExecs}
        workspaceId={wsId}
        onClose={() => setBlackboardOpen(false)}
      />
    </div>
  );
}
