// Loop 环路执行看板。
//
// 将所有环路的执行历史以看板形式展示，参考 KanbanBoard 的列布局风格：
// - 列：运行中 / 待审批 / 成功 / 部分 / 失败 / 已取消 / 超限
// - 每列按时间倒序展示 execution 卡片
// - 支持时间范围过滤和环路名称搜索
//
// 数据来源：遍历所有 loop，对每个 loop 调用 listExecutions 聚合结果。

import { useState, useEffect, useMemo, useCallback } from 'react';
import { Input, Segmented, Spin, Empty, Tag, Tooltip } from 'antd';
import {
  SearchOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
  LoadingOutlined,
  MinusCircleOutlined,
  ExclamationCircleOutlined,
} from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import type { LoopExecutionDto, LoopListItem } from '@/types/loop';
import { formatRelativeTime } from '@/utils/datetime';

const TIME_OPTIONS: { label: string; value: number }[] = [
  { label: '6h',  value: 6 },
  { label: '12h', value: 12 },
  { label: '24h', value: 24 },
  { label: '3d',  value: 72 },
  { label: '7d',  value: 168 },
];

// 状态 → 颜色 + 图标
function execStatusView(status: string): { color: string; icon: React.ReactNode; label: string } {
  switch (status) {
    case 'success':        return { color: 'green',    icon: <CheckCircleOutlined />,       label: '成功' };
    case 'failed':         return { color: 'red',      icon: <CloseCircleOutlined />,       label: '失败' };
    case 'partial':        return { color: 'orange',   icon: <CloseCircleOutlined />,       label: '部分' };
    case 'running':        return { color: 'blue',     icon: <LoadingOutlined />,           label: '运行中' };
    case 'cancelled':      return { color: 'default',  icon: <MinusCircleOutlined />,       label: '已取消' };
    case 'capped_step':    return { color: 'gold',     icon: <MinusCircleOutlined />,       label: '步数超限' };
    case 'capped_token':   return { color: 'purple',  icon: <MinusCircleOutlined />,       label: 'Token 超限' };
    case 'pending_approval': return { color: 'orange', icon: <ExclamationCircleOutlined />, label: '待审批' };
    default:               return { color: 'default',  icon: <MinusCircleOutlined />,       label: status };
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

// 看板列定义
interface ColumnDef {
  status: string;
  label: string;
  color: string;
}

const COLUMNS: ColumnDef[] = [
  { status: 'running',         label: '运行中',    color: '#3b82f6' },
  { status: 'pending_approval', label: '待审批',   color: '#f59e0b' },
  { status: 'success',         label: '成功',      color: '#22c55e' },
  { status: 'partial',         label: '部分',      color: '#f97316' },
  { status: 'failed',          label: '失败',      color: '#ef4444' },
  { status: 'cancelled',       label: '已取消',    color: '#94a3b8' },
  { status: 'capped_step',     label: '步数超限',  color: '#eab308' },
  { status: 'capped_token',   label: 'Token超限', color: '#a855f7' },
];

interface LoopExecutionWithLoopName extends LoopExecutionDto {
  loop_name: string;
}

interface Props {
  searchText?: string;
  hours?: number;
  onSearchChange?: (v: string) => void;
  onHoursChange?: (h: number) => void;
}

export function LoopKanban({ searchText: externalSearch, hours: externalHours, onSearchChange, onHoursChange }: Props = {}) {
  const [internalSearch, setInternalSearch] = useState('');
  const [internalHours, setInternalHours] = useState(24);
  const searchText = externalSearch ?? internalSearch;
  const hours = externalHours ?? internalHours;
  const handleSearchChange = (v: string) => { if (onSearchChange) onSearchChange(v); else setInternalSearch(v); };
  const handleHoursChange = (h: number) => { if (onHoursChange) onHoursChange(h); else setInternalHours(h); };

  const [allLoops, setAllLoops] = useState<LoopListItem[]>([]);
  const [executions, setExecutions] = useState<LoopExecutionWithLoopName[]>([]);
  const [loading, setLoading] = useState(true);

  // 加载所有环路列表
  useEffect(() => {
    dbLoops.listLoops().then(setAllLoops).catch(() => setAllLoops([]));
  }, []);

  // 加载所有环路的执行历史（批量聚合）
  useEffect(() => {
    if (allLoops.length === 0) return;
    let cancelled = false;
    setLoading(true);

    // 批量并发拉取每个环路的最新执行记录（limit=20 足够展示近期）
    Promise.all(
      allLoops.map(loop =>
        dbLoops.listExecutions(loop.id, { page: 1, limit: 20 })
          .then(res => res.items.map(e => ({ ...e, loop_name: loop.name })))
          .catch(() => [])
      )
    ).then(results => {
      if (cancelled) return;
      // 合并并按 started_at 倒序
      const flat = results.flat();
      flat.sort((a, b) => new Date(b.started_at).getTime() - new Date(a.started_at).getTime());
      setExecutions(flat);
    }).catch(() => {
      if (!cancelled) setExecutions([]);
    }).finally(() => {
      if (!cancelled) setLoading(false);
    });

    return () => { cancelled = true; };
  }, [allLoops]);

  // 按时间过滤
  const timeFiltered = useMemo(() => {
    const cutoff = hours ? Date.now() - hours * 3600 * 1000 : 0;
    if (cutoff === 0) return executions;
    return executions.filter(e => {
      const t = new Date(e.started_at).getTime();
      return t >= cutoff;
    });
  }, [executions, hours]);

  // 按环路名称搜索过滤
  const filtered = useMemo(() => {
    if (!searchText.trim()) return timeFiltered;
    const q = searchText.toLowerCase();
    return timeFiltered.filter(e =>
      e.loop_name.toLowerCase().includes(q) ||
      e.trigger_type.toLowerCase().includes(q)
    );
  }, [timeFiltered, searchText]);

  // 按状态分组
  const grouped = useMemo(() => {
    const map: Record<string, LoopExecutionWithLoopName[]> = {};
    for (const col of COLUMNS) map[col.status] = [];
    for (const exec of filtered) {
      if (map[exec.status]) {
        map[exec.status].push(exec);
      } else {
        // 未知状态归入最后一列
        const lastCol = COLUMNS[COLUMNS.length - 1];
        map[lastCol.status].push(exec);
      }
    }
    return map;
  }, [filtered]);

  // 统计
  const stats = useMemo(() => {
    const result: Record<string, number> = {};
    for (const col of COLUMNS) result[col.label] = grouped[col.status]?.length ?? 0;
    return result;
  }, [grouped]);

  // 渲染单个 execution 卡片
  const renderCard = useCallback((exec: LoopExecutionWithLoopName) => {
    const view = execStatusView(exec.status);
    return (
      <div
        key={`${exec.loop_id}-${exec.id}`}
        className="loop-kanban-card"
        style={{
          borderTop: `3px solid ${view.color}`,
          background: 'var(--color-bg-elevated, #ffffff)',
          border: '1px solid var(--color-border, #e2e8f0)',
          borderRadius: 8,
          padding: '10px 12px',
          marginBottom: 8,
          cursor: 'default',
        }}
      >
        {/* 环路名称 + ID */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 6 }}>
          {view.icon}
          <span style={{ fontWeight: 600, fontSize: 13, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {exec.loop_name}
          </span>
          <Tag color={view.color} style={{ margin: 0, fontSize: 10 }}>{view.label}</Tag>
        </div>

        {/* 触发类型 */}
        <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 4 }}>
          触发: {exec.trigger_type}
        </div>

        {/* 时间 + 进度 */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 11, color: 'var(--color-text-secondary)' }}>
          <Tooltip title={`开始: ${exec.started_at}`}>
            <span>{formatRelativeTime(exec.started_at)}</span>
          </Tooltip>
          <span>{exec.completed_steps}/{exec.total_steps} 环节</span>
          <span style={{ fontFamily: 'monospace', color: 'var(--color-text-tertiary)' }}>
            {durationLabel(exec.started_at, exec.finished_at)}
          </span>
        </div>

        {/* 待审批标记 */}
        {exec.pending_approval_count > 0 && (
          <div style={{ marginTop: 4 }}>
            <Tag color="red" style={{ fontSize: 10, fontWeight: 600 }}>
              <ExclamationCircleOutlined /> {exec.pending_approval_count} 待审批
            </Tag>
          </div>
        )}
      </div>
    );
  }, []);

  // 渲染列
  const renderColumn = (col: ColumnDef) => {
    const items = grouped[col.status] ?? [];
    return (
      <div
        key={col.status}
        className="loop-kanban-column"
        style={{
          minWidth: 220,
          maxWidth: 280,
          flex: 1,
          display: 'flex',
          flexDirection: 'column',
        }}
      >
        {/* 列头 */}
        <div
          className="loop-kanban-column-header"
          style={{
            borderBottom: `3px solid ${col.color}`,
            padding: '8px 12px',
            marginBottom: 8,
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <div
              className="loop-kanban-column-dot"
              style={{ width: 8, height: 8, borderRadius: 4, backgroundColor: col.color }}
            />
            <span style={{ fontWeight: 600, fontSize: 13 }}>{col.label}</span>
            <span
              className="loop-kanban-column-count"
              style={{
                background: `${col.color}18`,
                color: col.color,
                borderRadius: 10,
                padding: '0 6px',
                fontSize: 11,
                fontWeight: 600,
              }}
            >
              {items.length}
            </span>
          </div>
        </div>

        {/* 列体 */}
        <div className="loop-kanban-column-body" style={{ flex: 1, overflowY: 'auto', padding: '0 4px' }}>
          {items.length === 0 ? (
            <div style={{ textAlign: 'center', padding: '20px 0', color: 'var(--color-text-tertiary)', fontSize: 12 }}>
              暂无
            </div>
          ) : (
            items.map(renderCard)
          )}
        </div>
      </div>
    );
  };

  return (
    <div className="loop-kanban-board">
      {/* 工具栏 */}
      <div
        className="loop-kanban-toolbar"
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '8px 16px',
          borderBottom: '1px solid var(--color-border)',
          flexWrap: 'wrap',
        }}
      >
        <Input
          placeholder="搜索环路名称或触发类型…"
          prefix={<SearchOutlined style={{ color: 'var(--color-text-tertiary)' }} />}
          value={searchText}
          onChange={e => handleSearchChange(e.target.value)}
          allowClear
          size="small"
          style={{ width: 220 }}
        />
        <Segmented
          size="small"
          options={TIME_OPTIONS.map(o => ({ label: o.label, value: o.label }))}
          value={TIME_OPTIONS.find(o => o.value === hours)?.label || '24h'}
          onChange={label => {
            const opt = TIME_OPTIONS.find(o => o.label === label);
            if (opt) handleHoursChange(opt.value);
          }}
        />
        {/* 统计 */}
        <div style={{ marginLeft: 'auto', display: 'flex', gap: 12, flexWrap: 'wrap' }}>
          {COLUMNS.map(col => (
            <span key={col.status} style={{ fontSize: 12, color: col.color }}>
              {col.label} <strong>{stats[col.label]}</strong>
            </span>
          ))}
        </div>
      </div>

      {/* 看板列 */}
      {loading ? (
        <div style={{ textAlign: 'center', padding: 60 }}>
          <Spin tip="加载环路执行历史…" />
        </div>
      ) : filtered.length === 0 ? (
        <Empty description="暂无环路执行记录" style={{ padding: 60 }} />
      ) : (
        <div
          style={{
            display: 'flex',
            gap: 12,
            padding: '12px 16px',
            overflowX: 'auto',
            alignItems: 'flex-start',
          }}
        >
          {COLUMNS.map(renderColumn)}
        </div>
      )}
    </div>
  );
}
