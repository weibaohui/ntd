// Loop 环路执行看板。
//
// 将所有环路的执行历史以看板形式展示，参考 KanbanBoard 的列布局风格：
// - 列：运行中 / 待审批 / 成功 / 部分 / 失败 / 已取消 / 超限
// - 每列按时间倒序展示 execution 卡片
// - 搜索与时间过滤由父组件 MemorialBoard 统一管理，本组件不再重复渲染工具栏
//
// 交互说明：
// - 点击卡片：打开侧边栏，上方展示该环路的环节设计流程图（LoopFlowGraph），
//   下方展示该次执行的环节轨迹（StepExecList），实现「设计 vs 实际」对照。
// - 黑板按钮：打开黑板抽屉，展示该次执行中所有环节的结论摘要（复用 BlackboardDrawer 组件）
//
// 数据来源：遍历所有 loop，对每个 loop 调用 listExecutions 聚合结果。

import { useState, useEffect, useMemo, useCallback } from 'react';
import { Button, Drawer, Spin, Empty, Tag, Tooltip, Divider, App as AntApp } from 'antd';
import {
  ReadOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
  LoadingOutlined,
  MinusCircleOutlined,
  ExclamationCircleOutlined,
  ApartmentOutlined,
  HistoryOutlined,
} from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import { useApp } from '@/hooks/useApp';
import type { LoopExecutionDto, LoopListItem, LoopExecutionDetail, LoopDetail } from '@/types/loop';
import { formatRelativeTime } from '@/utils/datetime';
// 复用 LoopStudioExecutionsPanel 中的执行轨迹卡片列表与黑板抽屉组件，
// 确保环路执行历史在不同视图下的展示形态一致。
import { StepExecList, BlackboardDrawer, formatToken } from './LoopStudioExecutionsPanel';
// 复用环路流程设计图组件，在上方展示环节设计布局。
import { LoopFlowGraph } from '@/components/loop-flow/LoopFlowGraph';

// 环路执行记录增强类型：增加 loop_name 方便在卡片中直接显示环路名称，
// 避免在渲染时反复回查 loop 列表
interface LoopExecutionWithLoopName extends LoopExecutionDto {
  loop_name: string;
}

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

// 执行卡片组件：展示单个环路执行记录的核心信息。
// 拆分理由：原 renderCard 超过 30 行，抽成独立组件符合代码规范。
// 为什么接受 view 参数：避免在子组件内重复调用 execStatusView，提升性能。
interface ExecutionCardProps {
  exec: LoopExecutionWithLoopName;
  view: ReturnType<typeof execStatusView>;
  onClick?: (exec: LoopExecutionWithLoopName) => void;
  onBlackboard?: (exec: LoopExecutionWithLoopName) => void;
}

function ExecutionCard({ exec, view, onClick, onBlackboard }: ExecutionCardProps) {
  return (
    <div
      className="loop-kanban-card"
      style={{
        borderTop: `3px solid ${view.color}`,
        background: 'var(--color-bg-elevated, #ffffff)',
        border: '1px solid var(--color-border, #e2e8f0)',
        borderRadius: 8,
        padding: '10px 12px',
        marginBottom: 8,
        cursor: 'pointer',
        transition: 'box-shadow 200ms',
      }}
      onClick={() => onClick?.(exec)}
    >
      <CardHeader exec={exec} view={view} />
      <CardTrigger exec={exec} />
      <CardProgress exec={exec} />
      {/* Token 消耗汇总：从执行历史 Token 消耗聚合数据中提取，
          展示输入/输出/缓存读取的 token 数量，让用户快速了解资源消耗情况 */}
      <CardTokenSummary exec={exec} />
      {exec.pending_approval_count > 0 && <CardApprovalBadge count={exec.pending_approval_count} />}
      {/* 黑板按钮：点击后查看该次执行所有环节的结论摘要 */}
      <div style={{ marginTop: 6, display: 'flex', justifyContent: 'flex-end' }}>
        <Button
          type="link"
          size="small"
          icon={<ReadOutlined />}
          onClick={(e) => {
            // 阻止冒泡到卡片的点击事件，避免同时打开轨迹侧边栏
            e.stopPropagation();
            onBlackboard?.(exec);
          }}
          style={{ fontSize: 11, padding: 0, height: 'auto', lineHeight: '20px' }}
        >
          黑板
        </Button>
      </div>
    </div>
  );
}

// 卡片头部：环路名称 + 状态图标 + 状态标签。
// 为什么独立出来：header 布局逻辑独立，方便后续调整样式或增加交互。
function CardHeader({ exec, view }: { exec: LoopExecutionWithLoopName; view: ReturnType<typeof execStatusView> }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 6 }}>
      {view.icon}
      <span style={{ fontWeight: 600, fontSize: 13, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
        {exec.loop_name}
      </span>
      <Tag color={view.color} style={{ margin: 0, fontSize: 10 }}>{view.label}</Tag>
    </div>
  );
}

// 触发类型行。
// 为什么独立：触发信息是可选的辅助信息，未来可能需要扩展显示触发者、触发参数等。
function CardTrigger({ exec }: { exec: LoopExecutionWithLoopName }) {
  return (
    <div style={{ fontSize: 11, color: 'var(--color-text-tertiary)', marginBottom: 4 }}>
      触发: {exec.trigger_type}
    </div>
  );
}

// 进度与时间信息行。
// 为什么独立：进度计算逻辑相对复杂（相对时间 + 环节进度 + 耗时），单独组件便于测试。
function CardProgress({ exec }: { exec: LoopExecutionWithLoopName }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 11, color: 'var(--color-text-secondary)' }}>
      <Tooltip title={`开始: ${exec.started_at}`}>
        <span>{formatRelativeTime(exec.started_at)}</span>
      </Tooltip>
      <span>{exec.completed_steps}/{exec.total_steps} 环节</span>
      <span style={{ fontFamily: 'monospace', color: 'var(--color-text-tertiary)' }}>
        {durationLabel(exec.started_at, exec.finished_at)}
      </span>
    </div>
  );
}

// Token 消耗汇总行：展示该次执行的输入/输出/缓存读取 token 数量。
// 为什么独立成组件：条件渲染逻辑独立（只有 token_summary 存在且有消耗时才展示），
// 且数据来自后端聚合计算，与卡片其他信息解耦。
// 样式紧凑以适配看板卡片尺寸，颜色区分不同类型便于快速识别。
function CardTokenSummary({ exec }: { exec: LoopExecutionWithLoopName }) {
  const ts = exec.token_summary;
  if (!ts) return null;
  const hasTokens = ts.total_input_tokens > 0 || ts.total_output_tokens > 0 || (ts.total_cache_read_input_tokens ?? 0) > 0;
  if (!hasTokens) return null;
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 4, flexWrap: 'wrap', fontSize: 10, marginTop: 4, color: 'var(--color-text-tertiary)' }}>
      <span style={{ color: '#1677ff', fontWeight: 600 }}>输入 {formatToken(ts.total_input_tokens)}</span>
      <span style={{ color: 'var(--color-text-tertiary)' }}>/</span>
      <span style={{ color: '#52c41a', fontWeight: 600 }}>输出 {formatToken(ts.total_output_tokens)}</span>
      {ts.total_cache_read_input_tokens > 0 && (
        <>
          <span style={{ color: 'var(--color-text-tertiary)' }}>/</span>
          <span style={{ color: '#722ed1', fontWeight: 600 }}>缓存 {formatToken(ts.total_cache_read_input_tokens)}</span>
        </>
      )}
    </div>
  );
}

// 待审批徽章。
// 为什么独立：条件渲染逻辑独立，且未来可能需要支持点击跳转到审批页。
function CardApprovalBadge({ count }: { count: number }) {
  return (
    <div style={{ marginTop: 4 }}>
      <Tag color="red" style={{ fontSize: 10, fontWeight: 600 }}>
        <ExclamationCircleOutlined /> {count} 待审批
      </Tag>
    </div>
  );
}

// 看板列组件：展示一列执行记录。
// 拆分理由：原 renderColumn 超过 30 行，抽成独立组件符合规范。
// 为什么接受 renderCard：避免在子组件内重新定义卡片渲染逻辑，保持单一数据源。
interface KanbanColumnProps {
  col: ColumnDef;
  items: LoopExecutionWithLoopName[];
  renderCard: (exec: LoopExecutionWithLoopName) => React.ReactNode;
}

function KanbanColumn({ col, items, renderCard }: KanbanColumnProps) {
  return (
    <div
      className="loop-kanban-column"
      style={{
        minWidth: 220,
        maxWidth: 280,
        flex: 1,
        display: 'flex',
        flexDirection: 'column',
        minHeight: 0,
      }}
    >
      <ColumnHeader col={col} count={items.length} />
      <ColumnBody items={items} renderCard={renderCard} />
    </div>
  );
}

// 列头：状态标签 + 数量徽章。
// 为什么独立：header 样式逻辑独立，未来可能需要支持拖拽排序或自定义显隐。
function ColumnHeader({ col, count }: { col: ColumnDef; count: number }) {
  return (
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
          {count}
        </span>
      </div>
    </div>
  );
}

// 列体：卡片列表或空状态。
// 为什么独立：body 的滚动容器逻辑与 header 独立，方便后续增加虚拟滚动优化。
function ColumnBody({ items, renderCard }: { items: LoopExecutionWithLoopName[]; renderCard: (exec: LoopExecutionWithLoopName) => React.ReactNode }) {
  return (
    <div className="loop-kanban-column-body" style={{ flex: 1, minHeight: 0, overflowY: 'auto', padding: '0 4px' }}>
      {items.length === 0 ? (
        <div style={{ textAlign: 'center', padding: '20px 0', color: 'var(--color-text-tertiary)', fontSize: 12 }}>
          暂无
        </div>
      ) : (
        items.map(renderCard)
      )}
    </div>
  );
}

// 自定义 Hook：加载并聚合所有环路的执行历史。
// 设计理由：
// - 数据获取逻辑独立成 hook，方便测试与复用
// - 使用 cancelledRef 防御快速切换时的竞态条件：晚返回的请求若发现已卸载，直接丢弃结果
// - limit=20 的边界：看板场景下只需展示近期执行，20 条足够覆盖常见时间窗口且避免首屏过慢
// - loading 状态在空列表时也能正确重置：确保空状态能正常展示，而非永久 loading
function useLoopExecutions(workspaceId?: number | null, hours?: number) {
  const [allLoops, setAllLoops] = useState<LoopListItem[]>([]);
  const [executions, setExecutions] = useState<LoopExecutionWithLoopName[]>([]);
  const [loading, setLoading] = useState(true);

  // 加载环路列表：按 workspace_id 过滤（如果传了）。
  // 切换 workspace 时先清空旧列表和执行记录，避免旧数据闪烁或触发无效的追加请求。
  useEffect(() => {
    let ignore = false;
    setAllLoops([]);
    setExecutions([]);
    setLoading(true);
    dbLoops.listLoops(workspaceId ?? undefined)
      .then(data => { if (!ignore) setAllLoops(data); })
      .catch(() => { if (!ignore) setAllLoops([]); })
      .finally(() => { if (!ignore) setLoading(false); });
    return () => { ignore = true; };
  }, [workspaceId]);

  // 环路列表加载后，批量并发拉取每个环路的执行历史。
  // allLoops 为空时（无内容 / 切换 workspace 已清空）也清空执行记录。
  // hours 变化时也重新拉取（时间过滤条件变了）。
  useEffect(() => {
    if (allLoops.length === 0) {
      setExecutions([]);
      return;
    }
    let cancelled = false;
    setLoading(true);

    Promise.all(
      allLoops.map(loop =>
        dbLoops.listExecutions(loop.id, { page: 1, limit: 20, hours: hours ?? undefined })
          .then(res => res.items.map(e => ({ ...e, loop_name: loop.name })))
          .catch(() => []) // 单个环路失败回退为空数组，不阻塞其他数据
      )
    )
      .then(results => {
        if (cancelled) return; // 防御竞态：组件已卸载则丢弃结果
        const flat = results.flat();
        // 按开始时间倒序：最新执行优先展示，符合看板习惯
        flat.sort((a, b) => new Date(b.started_at).getTime() - new Date(a.started_at).getTime());
        setExecutions(flat);
      })
      .catch(() => {
        if (!cancelled) setExecutions([]);
      })
      .finally(() => {
        // 确保 loading 总能重置，即使 allLoops 为空也不会永久 loading（Issue 2 核心修复）
        if (!cancelled) setLoading(false);
      });

    return () => { cancelled = true; };
  }, [allLoops, hours]);

  return { executions, loading };
}

interface Props {
  searchText?: string;
  hours?: number;
  onSearchChange?: (v: string) => void;
  onHoursChange?: (h: number) => void;
}

export function LoopKanban({ searchText: externalSearch, hours: externalHours, onSearchChange: _onSearchChange, onHoursChange: _onHoursChange }: Props = {}) {
  // 为什么区分 internal/external：支持受控/非受控两种模式，
  // 外部传入时作为受控组件（MemorialBoard 统一管理 searchText/hours），
  // 未传入时作为非受控组件（独立状态）。
  // 注意：搜索与时间过滤的 UI 控件由父组件 MemorialBoard 渲染，
  // 本组件不再渲染工具栏，但保留受控 props 透传能力，
  // 使得 MemoriaBoard 的搜索/过滤对 LoopKanban 依然生效。
  const [internalSearch] = useState('');
  const [internalHours] = useState(24);
  const searchText = externalSearch ?? internalSearch;
  const hours = externalHours ?? internalHours;

  const { message } = AntApp.useApp();
  const { state } = useApp();

  // 使用自定义 Hook 加载数据，逻辑抽离后函数体长度可控
  const { executions, loading } = useLoopExecutions(state.selectedWorkspace, hours);

  // ── 轨迹侧边栏状态 ────────────────────────────────────
  const [selectedExec, setSelectedExec] = useState<LoopExecutionWithLoopName | null>(null);
  const [execDetail, setExecDetail] = useState<LoopExecutionDetail | null>(null);
  // 环路设计信息：包含步骤定义，供 LoopFlowGraph 渲染环节设计图
  const [loopDetail, setLoopDetail] = useState<LoopDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);

  // 打开执行轨迹侧边栏：点击卡片时触发，加载该次执行的环节详情 与 环路的设计步骤。
  // 为什么同时加载 loopDetail：在侧边栏上方展示环节设计流程图（LoopFlowGraph），
  // 下方展示实际执行轨迹（StepExecList），实现「设计 vs 实际」对照。
  // 为什么用 Promise.all：两个接口无依赖关系，可以并发请求减少总等待耗时。
  // 为什么用 try/catch 静默失败：个别加载失败不应影响看板整体使用。
  const handleCardClick = useCallback(async (exec: LoopExecutionWithLoopName) => {
    setSelectedExec(exec);
    setDrawerOpen(true);
    setDetailLoading(true);
    setExecDetail(null);
    setLoopDetail(null);
    try {
      const [detail, loop] = await Promise.all([
        dbLoops.getExecution(exec.loop_id, exec.id),
        dbLoops.getLoop(exec.loop_id),
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

  // 打开黑板抽屉：点击卡片上的「黑板」按钮时触发。
  // 每次打开都重新加载最新的执行详情，确保黑板数据是最新的。
  const handleOpenBlackboard = useCallback(async (exec: LoopExecutionWithLoopName) => {
    try {
      const detail = await dbLoops.getExecution(exec.loop_id, exec.id);
      setBlackboardExecs(detail.step_executions);
      setBlackboardOpen(true);
    } catch {
      message.error('加载黑板数据失败');
    }
  }, [message]);

  // 按时间窗口过滤执行记录。
  // 为什么要独立 memo：hours 变化频繁（用户切换 Segmented），避免重复计算 cutoff 和遍历。
  // cutoff = 0 的边界：hours 未设置时显示全部，避免误过滤。
  const timeFiltered = useMemo(() => {
    const cutoff = hours ? Date.now() - hours * 3600 * 1000 : 0;
    if (cutoff === 0) return executions;
    return executions.filter(e => {
      const t = new Date(e.started_at).getTime();
      return t >= cutoff;
    });
  }, [executions, hours]);

  // 按搜索关键词过滤。
  // 为什么同时匹配 loop_name 和 trigger_type：用户可能记得环路名或触发方式，两者都是有效的查找维度。
  // 为什么 toLowerCase：忽略大小写，提升搜索容错性。
  const filtered = useMemo(() => {
    if (!searchText.trim()) return timeFiltered;
    const q = searchText.toLowerCase();
    return timeFiltered.filter(e =>
      e.loop_name.toLowerCase().includes(q) ||
      e.trigger_type.toLowerCase().includes(q)
    );
  }, [timeFiltered, searchText]);

  // 按状态分组到看板列。
  // 为什么未知状态归入最后一列：后端可能新增状态，前端未同步时不能丢弃数据，
  // 归入"Token 超限"列作为兜底，便于用户发现异常并反馈。
  // 为什么先预初始化所有列：确保即使某列为空也能渲染"暂无"占位，保持列布局稳定。
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

  // 渲染单个执行卡片。
  // 为什么抽成独立组件：原 renderCard 函数超过 30 行，拆分后主体逻辑更清晰。
  // 为什么用 useCallback：避免每次渲染都重新创建函数，减少子组件不必要的 re-render。
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

  // 渲染看板列。
  // 为什么提取成组件：原函数超过 30 行，拆分后符合规范且便于测试列渲染逻辑。
  const renderColumn = (col: ColumnDef) => {
    const items = grouped[col.status] ?? [];
    return <KanbanColumn key={col.status} col={col} items={items} renderCard={renderCard} />;
  };

  return (
    <div className="loop-kanban-board">
      {/* 搜索与时间过滤由父组件 MemorialBoard 统一管理，此处不再重复渲染工具栏 */}

      {/* 看板列 */}
      {loading ? (
        <div style={{ textAlign: 'center', padding: 60 }}>
          <Spin tip="加载环路执行历史…" />
        </div>
      ) : filtered.length === 0 ? (
        <Empty description="暂无环路执行记录" style={{ padding: 60 }} />
      ) : (
        <>
          {/* 让超宽只发生在看板内部，而不是把外层 PageCard / App 主视图撑宽。
              这样切换到环路视图时，顶部视图切换按钮仍然留在屏幕内。 */}
          <div className="loop-kanban-columns-container">
          {COLUMNS.map(renderColumn)}
          </div>
        </>
      )}

      {/* 执行轨迹侧边栏：点击卡片时打开，上方展示该环路的环节设计流程图，
          下方展示该次执行的环节轨迹，实现「设计 vs 实际」对照。
          复用 LoopFlowGraph（环节设计图）和 StepExecList（实际执行轨迹）。 */}
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
            {/* 上方：环节设计流程图 — 展示该环路的标准步骤设计图，
                让用户对照「预期设计」与「实际执行」。 */}
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

            {/* 下方：实际执行轨迹 — 展示该次执行中各环节的真实运行情况。 */}
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
                executionId={execDetail.id}
                onApproved={() => {
                  // 审批通过后重新加载执行详情，保证环节状态与后端一致
                  dbLoops.getExecution(selectedExec.loop_id, selectedExec.id)
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
