import { useEffect, useState, useMemo } from 'react';
import { Card, Segmented, Skeleton, Empty, Input, Select } from 'antd';
import {
  CheckCircleOutlined,
  CloseCircleOutlined,
  AppstoreOutlined,
  ProfileOutlined,
  SearchOutlined,
  FolderOutlined,
  ThunderboltOutlined,
  SyncOutlined,
  ReadOutlined,
} from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { useApp } from '@/hooks/useApp';
import { KanbanBoard } from './KanbanBoard';
import { RunningBoard } from './RunningBoard';
// 引入环路视图组件：与 KanbanBoard（todo 看板）、RunningBoard（运行视图）并列，
// 为什么需要：提供环路维度的执行历史聚合视图，补齐 todo 维度之外的监控缺口。
import { LoopKanban } from './LoopKanban';
import { TodoCard } from './TodoCard';
import * as db from '@/utils/database';
import { formatRelativeTime } from '@/utils/datetime';
import type { RecentCompletedTodo, Tag, ExecutionRecord, ProjectDirectory } from '@/types';

const TIME_OPTIONS: { label: string; value: number }[] = [
  { label: '6h', value: 6 },
  { label: '12h', value: 12 },
  { label: '24h', value: 24 },
  { label: '3d', value: 72 },
  { label: '7d', value: 168 },
];

// 看板模式类型：支持四种视图切换。
// 为什么新增 loop_kanban：环路执行历史需要独立视图，与 todo 维度的看板互补。
// 设计取舍：复用同一个 Segmented 切换，避免多入口导航混乱。
type BoardMode = 'memorial' | 'kanban' | 'running' | 'loop_kanban';

export function MemorialBoard() {
  const { state, dispatch } = useApp();
  const [boardMode, setBoardMode] = useState<BoardMode>('memorial');
  const [items, setItems] = useState<RecentCompletedTodo[]>([]);
  const [loading, setLoading] = useState(true);
  // 为什么 hours 和 searchText 在 MemorialBoard 层管理：
  // 四种视图（memorial/kanban/running/loop_kanban）共享同一个时间过滤和搜索状态，
  // 用户切换视图时保持筛选条件，避免重复输入，提升体验。
  const [hours, setHours] = useState(24);
  const [searchText, setSearchText] = useState('');
  const [expandedIds, setExpandedIds] = useState<Set<number>>(new Set());
  const [promptExpandedIds, setPromptExpandedIds] = useState<Set<number>>(new Set());
  const [projectDirectories, setProjectDirectories] = useState<ProjectDirectory[]>([]);
  // selectedProject 使用 workspace_id（project_directories.id），不再用 path。
  const [selectedProject, setSelectedProject] = useState<number | null>(null);

  /* ─── Run history switching ─── */
  const [selectedRunIndex, setSelectedRunIndex] = useState<Record<number, number>>({});
  const [totalRunsCache, setTotalRunsCache] = useState<Record<number, number>>({});
  const [runDataCache, setRunDataCache] = useState<Record<number, (ExecutionRecord | null)[]>>({});
  const [loadingRunIndex, setLoadingRunIndex] = useState<Record<number, number | null>>({});

  useEffect(() => {
    if (boardMode !== 'memorial') return;
    let cancelled = false;
    setItems([]); // 切换 workspace 或重新加载时先清空旧数据
    setLoading(true);
    db.getRecentCompletedTodos(hours, state.selectedWorkspace ?? undefined)
      .then(data => {
        if (!cancelled) {
          setItems(data);
          // Fetch total run count for each todo
          for (const item of data) {
            if (!totalRunsCache[item.todo_id]) {
              db.getExecutionRecords(item.todo_id, 1, 1, undefined, undefined, state.selectedWorkspace ?? undefined).then(page => {
                if (page.total > 0) {
                  setTotalRunsCache(prev => ({ ...prev, [item.todo_id]: page.total }));
                }
              }).catch(() => {});
            }
          }
        }
      })
      .catch(() => {
        if (!cancelled) setItems([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => { cancelled = true; };
  }, [hours, boardMode, state.selectedWorkspace]);

  // 切换工作空间后立即拉取该 workspace 的 todo，保证数据最新。
  useEffect(() => {
    const wid = state.selectedWorkspace;
    if (wid == null) return;
    db.getAllTodos(wid).then(todos => {
      dispatch({ type: 'SET_TODOS_BY_WORKSPACE', workspaceId: wid, payload: todos });
    });
  }, [state.selectedWorkspace, dispatch]);

  // 加载项目目录列表，供项目维度过滤使用。
  // 与 KanbanBoard 逻辑一致：首次加载 + 监听 TodoDrawer 快速新增事件刷新。
  // 失败时回退为空数组，不影响纪念板主体展示。
  useEffect(() => {
    const reload = () => {
      db.getProjectDirectories() // 从后端拉全量目录
        .then(setProjectDirectories) // 更新下拉数据源
        .catch(() => setProjectDirectories([])); // 静默失败：项目过滤退化，不阻塞主流程
    };
    reload(); // 首次挂载加载
    window.addEventListener('projectDirectoryAdded', reload); // 跨组件刷新
    return () => window.removeEventListener('projectDirectoryAdded', reload); // 清理监听
  }, []);

  const toggleExpand = (todoId: number) => {
    setExpandedIds(prev => {
      const next = new Set(prev);
      if (next.has(todoId)) {
        next.delete(todoId);
      } else {
        next.add(todoId);
      }
      return next;
    });
  };

  const togglePromptExpand = (todoId: number) => {
    setPromptExpandedIds(prev => {
      const next = new Set(prev);
      if (next.has(todoId)) {
        next.delete(todoId);
      } else {
        next.add(todoId);
      }
      return next;
    });
  };

  /* ─── Select run index (on-demand fetch) ─── */
  const handleSelectRun = async (todoId: number, runIndex: number) => {
    if (selectedRunIndex[todoId] === runIndex) return;
    setSelectedRunIndex(prev => ({ ...prev, [todoId]: runIndex }));

    if (runDataCache[todoId]?.[runIndex]) return;

    if (runIndex === 0) {
      const item = items.find(i => i.todo_id === todoId);
      if (item) {
        const record: ExecutionRecord = {
          id: item.record_id,
          todo_id: item.todo_id,
          status: item.execution_status === 'success' ? 'success' : 'failed',
          command: '',
          stdout: '',
          stderr: '',
          result: item.result,
          started_at: '',
          finished_at: item.completed_at,
          usage: item.usage,
          executor: item.executor,
          model: item.model,
          trigger_type: item.trigger_type,
          pid: null,
        };
        setRunDataCache(prev => {
          const arr = prev[todoId] || [];
          const next = [...arr];
          next[0] = record;
          return { ...prev, [todoId]: next };
        });
      }
      return;
    }

    setLoadingRunIndex(prev => ({ ...prev, [todoId]: runIndex }));
    try {
      const page = await db.getExecutionRecords(todoId, runIndex + 1, 1, undefined, undefined, state.selectedWorkspace ?? undefined);
      if (page.records.length > 0) {
        const record = page.records[0];
        setRunDataCache(prev => {
          const arr = prev[todoId] || [];
          const next = [...arr];
          next[runIndex] = record;
          return { ...prev, [todoId]: next };
        });
        if (!totalRunsCache[todoId] && page.total > 0) {
          setTotalRunsCache(prev => ({ ...prev, [todoId]: page.total }));
        }
      }
    } catch {
      // silently ignore
    } finally {
      setLoadingRunIndex(prev => ({ ...prev, [todoId]: null }));
    }
  };

  const handleSelectTodo = (todoId: number, e: React.MouseEvent) => {
    e.stopPropagation();
    dispatch({ type: 'SELECT_TODO', payload: todoId });
  };

  const filteredItems = useMemo(() => {
    let result = items;
    // 按工作空间过滤：selectedProject 为 workspace_id。
    // items 是轻量快照（不含 workspace 字段），需要回查 state.todos 取 workspace_id
    if (selectedProject != null) {
      result = result.filter(i => {
        const todo = state.todos.find(t => t.id === i.todo_id);
        return todo?.workspace_id === selectedProject;
      });
    }
    // 按搜索文本过滤：匹配标题或 prompt
    if (searchText.trim()) {
      const q = searchText.toLowerCase();
      result = result.filter(i =>
        i.title.toLowerCase().includes(q) ||
        (i.prompt && i.prompt.toLowerCase().includes(q))
      );
    }
    return result;
  }, [items, searchText, selectedProject, state.todos]);

  /* ─── Responsive column count ─── */
  const [columnCount, setColumnCount] = useState(() => {
    const w = typeof window !== 'undefined' ? window.innerWidth : 1600;
    if (w >= 1600) return 4;
    if (w >= 1100) return 3;
    if (w >= 769) return 2;
    return 1;
  });

  useEffect(() => {
    let timeoutId: ReturnType<typeof setTimeout>;
    const onResize = () => {
      clearTimeout(timeoutId);
      timeoutId = setTimeout(() => {
        const w = window.innerWidth;
        setColumnCount(
          w >= 1600 ? 4 :
          w >= 1100 ? 3 :
          w >= 769  ? 2 :
                      1
        );
      }, 150);
    };
    window.addEventListener('resize', onResize);
    return () => {
      clearTimeout(timeoutId);
      window.removeEventListener('resize', onResize);
    };
  }, []);

  /* ─── Split items into columns ─── */
  const columns = useMemo(() => {
    const cols: typeof filteredItems[] = Array.from({ length: columnCount }, () => []);
    filteredItems.forEach((item, i) => {
      cols[i % columnCount].push(item);
    });
    return cols;
  }, [filteredItems, columnCount, loading]);

  const successCount = filteredItems.filter(i => i.execution_status === 'success').length;
  const failedCount = filteredItems.filter(i => i.execution_status === 'failed').length;

  const kanbanStats = useMemo(() => {
    const cutoff = hours ? Date.now() - hours * 3600 * 1000 : 0;
    return state.todos.filter(t => {
      if ((t.status === 'completed' || t.status === 'failed') && cutoff > 0) {
        const tUpdated = new Date(t.updated_at).getTime();
        if (isNaN(tUpdated) || tUpdated < cutoff) return false;
      }
      if (searchText.trim()) {
        const q = searchText.toLowerCase();
        return t.title.toLowerCase().includes(q) || (t.prompt && t.prompt.toLowerCase().includes(q));
      }
      return true;
    });
  }, [state.todos, searchText, hours]);
  const kanbanStatsCount = { pending: 0, running: 0, completed: 0, failed: 0 };
  kanbanStats.forEach(t => { if (kanbanStatsCount[t.status] !== undefined) kanbanStatsCount[t.status]++; });

  const renderCard = (item: RecentCompletedTodo) => {
    const isSuccess = item.execution_status === 'success';
    const expanded = expandedIds.has(item.todo_id);
    const resolvedTags = item.tag_ids.map(tid => state.tags.find(t => t.id === tid)).filter(Boolean) as Tag[];
    // 获取项目名称（按 workspace_id 匹配）
    const todo = state.todos.find(t => t.id === item.todo_id);
    const projectDir = projectDirectories.find(d => d.id === todo?.workspace_id);
    const projectName = projectDir?.name || null;

    // Run history: determine which run to display
    const runIdx = selectedRunIndex[item.todo_id] ?? 0;
    const cachedRun = runDataCache[item.todo_id]?.[runIdx];
    let resultText: string;
    let displayModel: string | null | undefined;
    let displayUsage: ExecutionRecord['usage'] | null | undefined;
    let displayTriggerType: string | undefined;

    if (runIdx === 0) {
      resultText = item.result || '';
      displayModel = item.model;
      displayUsage = item.usage;
      displayTriggerType = item.trigger_type;
    } else if (cachedRun) {
      resultText = cachedRun.result || '';
      displayModel = cachedRun.model;
      displayUsage = cachedRun.usage;
      displayTriggerType = cachedRun.trigger_type;
    } else {
      resultText = '';
      displayModel = null;
      displayUsage = null;
    }

    // Rating belongs to the ExecutionRecord, so each historical run lives in
    // runDataCache with its own score. For the latest run we fall back to the
    // `item.rating` field that the recent-completed endpoint now returns.
    let displayRating: number | null | undefined;
    if (runIdx === 0) {
      displayRating = item.rating;
    } else if (cachedRun) {
      displayRating = cachedRun.rating;
    } else {
      displayRating = null;
    }

    const isLoadingRun = loadingRunIndex[item.todo_id] != null && loadingRunIndex[item.todo_id] === runIdx && runIdx > 0;
    const runCount = totalRunsCache[item.todo_id] ?? 1;

    return (
      <Card
        key={item.todo_id}
        className={`memorial-card ${expanded ? 'expanded' : ''}`}
        size='small'
        onClick={() => toggleExpand(item.todo_id)}
        style={{
          borderTop: `3px solid ${isSuccess ? '#22c55e' : '#ef4444'}`,
        }}
        bodyStyle={{ padding: 0 }}
      >
        <TodoCard
          id={item.todo_id}
          title={item.title}
          prompt={item.prompt}
          resultText={resultText}
          isSuccess={isSuccess}
          showResultSection={true}
          executor={item.executor}
          time={formatRelativeTime(item.completed_at)}
          model={displayModel}
          projectName={projectName}
          tags={resolvedTags}
          usage={displayUsage}
          triggerType={displayTriggerType}
          promptExpanded={promptExpandedIds.has(item.todo_id)}
          resultExpanded={expanded}
          onTogglePrompt={() => togglePromptExpand(item.todo_id)}
          onToggleResult={() => toggleExpand(item.todo_id)}
          onSelectTodo={(e) => handleSelectTodo(item.todo_id, e)}
          runCount={runCount}
          selectedRun={runIdx}
          onSelectRun={(index) => handleSelectRun(item.todo_id, index)}
          isLoadingRun={isLoadingRun}
          rating={displayRating}
        />
      </Card>
    );
  };

  return (
    <PageCard
      icon={<ReadOutlined />}
      title="看板"
      extra={
        <>
          {/* 视图模式切换：四种视图平铺展示。
              为什么新增"环路视图"选项：
              - memorial：按完成时间聚合 todo 的执行结论，适合快速回顾成果
              - kanban：todo 维度的状态流转看板（待办/进行中/已完成/失败）
              - running：实时运行状态监控
              - loop_kanban：环路维度的执行历史看板，补齐 loop 视角的监控缺口
              为什么用 SyncOutlined 图标：loop 强调循环执行，sync 图标语义匹配。
          */}
          <Segmented
            size="small"
            value={boardMode}
            onChange={value => setBoardMode(value as BoardMode)}
            options={[
              { label: <span><ProfileOutlined /> 结论视图</span>, value: 'memorial' },
              { label: <span><AppstoreOutlined /> 看板视图</span>, value: 'kanban' },
              { label: <span><ThunderboltOutlined /> 运行视图</span>, value: 'running' },
              { label: <span><SyncOutlined /> 环路视图</span>, value: 'loop_kanban' },
            ]}
          />
        </>
      }
    >
      <div className="memorial-board">
        <div className="memorial-toolbar">
          <Input
            placeholder="搜索任务…"
            prefix={<SearchOutlined />}
            value={searchText}
            onChange={e => setSearchText(e.target.value)}
            allowClear
            size="small"
            style={{ width: 200 }}
          />
          <Segmented
            size="small"
            options={TIME_OPTIONS.map(o => ({ label: o.label, value: o.label }))}
            value={TIME_OPTIONS.find(o => o.value === hours)?.label || '24h'}
            onChange={label => {
              const opt = TIME_OPTIONS.find(o => o.label === label);
              if (opt) setHours(opt.value);
            }}
          />
          {/* 项目过滤下拉：value 为 workspace_id，label 优先显示项目名 */}
          <Select
            size="small"
            placeholder="项目过滤"
            allowClear
            value={selectedProject}
            onChange={setSelectedProject}
            style={{ width: 150 }}
            suffixIcon={<FolderOutlined />}
            options={projectDirectories.map(d => ({
              value: d.id, // value 用 workspace_id（唯一键）
              label: d.name || d.path,
            }))}
          />
          {boardMode === 'memorial' ? (
            <div className="memorial-summary">
              <span className="memorial-stat-dot memorial-stat-all">共 <strong>{filteredItems.length}</strong> 条</span>
              <span className="memorial-stat-dot memorial-stat-success">
                <CheckCircleOutlined /> <strong>{successCount}</strong> 成功
              </span>
              <span className="memorial-stat-dot memorial-stat-failed">
                <CloseCircleOutlined /> <strong>{failedCount}</strong> 失败
              </span>
            </div>
          ) : boardMode === 'kanban' ? (
            <div className="memorial-summary">
              <span className="memorial-stat-dot memorial-stat-all">共 <strong>{kanbanStats.length}</strong> 条</span>
              <span className="memorial-stat-dot" style={{ color: '#3b82f6' }}>待办 <strong>{kanbanStatsCount.pending}</strong></span>
              <span className="memorial-stat-dot" style={{ color: '#f59e0b' }}>进行中 <strong>{kanbanStatsCount.running}</strong></span>
              <span className="memorial-stat-dot" style={{ color: '#22c55e' }}>已完成 <strong>{kanbanStatsCount.completed}</strong></span>
              <span className="memorial-stat-dot" style={{ color: '#ef4444' }}>失败 <strong>{kanbanStatsCount.failed}</strong></span>
            </div>
          ) : null}
        </div>

        {/* 根据 boardMode 渲染对应视图。
            为什么 loop_kanban 分支复用 searchText 和 hours：
            - LoopKanban 支持受控模式，接受外部 searchText/hours 并回传 onChange
            - 用户从其他视图切换过来时，保持已输入的搜索词和时间窗口，避免状态丢失
            - onSearchChange/onHoursChange 回传给 MemorialBoard，使得多视图间筛选条件同步
            为什么 loop_kanban 不传 selectedProject：
            - loop 没有 workspace 概念，项目过滤仅适用于 todo 维度（memorial/kanban/running）
        */}
        {boardMode === 'running' ? (
          <RunningBoard searchText={searchText} hours={hours} selectedProject={selectedProject} />
        ) : boardMode === 'kanban' ? (
          <KanbanBoard searchText={searchText} hours={hours} onSearchChange={setSearchText} onHoursChange={setHours} />
        ) : boardMode === 'loop_kanban' ? (
          <LoopKanban searchText={searchText} hours={hours} onSearchChange={setSearchText} onHoursChange={setHours} />
        ) : loading ? (
          <div className="memorial-grid">
            {Array.from({ length: columnCount }).map((_, colIdx) => (
              <div key={colIdx} className="memorial-column">
                {Array.from({ length: 6 }).map((__, idx) => (
                  <Card key={`skeleton-${colIdx}-${idx}`} className="memorial-card" size="small" bodyStyle={{ padding: 12 }}>
                    <Skeleton active paragraph={{ rows: 4 }} />
                  </Card>
                ))}
              </div>
            ))}
          </div>
        ) : items.length === 0 ? (
          <div className="memorial-empty">
            <Empty description={<span style={{ color: 'var(--color-text-tertiary)' }}>最近 {hours} 小时内暂无完成的任务</span>} />
          </div>
        ) : (
          <div className="memorial-grid">
            {columns.map((col, colIdx) => (
              <div key={colIdx} className="memorial-column">
                {col.map(item => renderCard(item))}
              </div>
            ))}
          </div>
        )}
      </div>
    </PageCard>
  );
}
