// 任务页主壳。
// 三态视图切换：列表（Table）/ 看板（按状态分泳道）/ 卡片（卡片墙）。
// 详情独立路由：URL /#/tasks?id=123 进入详情全屏，无 id 时渲染当前视图模式。
// 列表/看板/卡片态全屏单页，不再用 ListDetailPage 双栏。

import { useEffect, useState, useCallback, useRef, useMemo } from 'react';
import { Input, Button, Segmented, message } from 'antd';
import {
  AppstoreOutlined,
  LayoutOutlined,
  PlusOutlined,
  ReloadOutlined,
  RocketOutlined,
  SearchOutlined,
  UnorderedListOutlined,
} from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { TimeRangeSegmented } from '@/components/common/TimeRangeSegmented';
import { TasksTableView } from '@/components/tasks/TasksTableView';
import { TasksKanbanView } from '@/components/tasks/TasksKanbanView';
import { TasksCardView } from '@/components/tasks/TasksCardView';
import { CreateTaskModal } from '@/components/tasks/CreateTaskModal';
import { TaskDetailPanel } from '@/components/tasks/TaskDetailPanel';
import bundledApi from '@/api/bundled';
import { listLoops } from '@/utils/database/loops';
// 109：列表形态直达路由——useViewState 提供 listView（URL ?view= 原文）。
import { useViewState, pickListView } from '@/hooks/useViewState';
import type { LoopLite, TaskItem, TasksViewMode } from '@/components/tasks/constants';
import { TASKS_VIEW_STORAGE_KEY } from '@/components/tasks/constants';

interface TasksPageProps {
  workspaceId: number | null;
}

/** 读取持久化的视图模式，默认 list。 */
function readInitialView(): TasksViewMode {
  try {
    const v = localStorage.getItem(TASKS_VIEW_STORAGE_KEY);
    if (v === 'list' || v === 'kanban' || v === 'card') return v;
  } catch {
    /* localStorage 不可用时静默降级 */
  }
  return 'list';
}

/** 把当前视图模式持久化到 localStorage。 */
function persistView(mode: TasksViewMode) {
  try {
    localStorage.setItem(TASKS_VIEW_STORAGE_KEY, mode);
  } catch {
    /* 静默降级 */
  }
}

/**
 * 从 URL hash 搜索参数中读取 id（任务详情 id）。
 * 返回 null 表示无 id 参数（列表态）。
 */
function readSelectedTaskId(): number | null {
  const hash = window.location.hash || '';
  const hashWithoutHash = hash.startsWith('#') ? hash.slice(1) : hash;
  const [, search] = hashWithoutHash.split('?', 2);
  const params = new URLSearchParams(search || '');
  const id = params.get('id');
  if (!id) return null;
  const n = Number(id);
  return Number.isFinite(n) ? n : null;
}

export function TasksPage({ workspaceId }: TasksPageProps) {
  // 没有选中工作空间时回退到 1，与原实现一致。
  // 因为 wsId 在 API 调用路径中是必填项，1 是开发环境默认工作空间。
  const wsId = workspaceId ?? 1;

  // 视图路由：useViewState 提供 pushUrl/replaceUrl，用于驱动 URL hash；
  // listView 是 URL ?view= 原文（列表形态直达路由）。
  const { pushUrl, replaceUrl, listView } = useViewState();

  // 视图模式：URL ?view= 优先（直达指定形态），无参数/非法值回退 localStorage 记忆。
  // storedView 只在挂载时读一次：URL 变化走 listView 同步，localStorage 只作无参数兜底。
  const [storedView] = useState<TasksViewMode>(readInitialView);
  const viewMode = pickListView(listView, ['list', 'kanban', 'card'], storedView) as TasksViewMode;

  // 任务列表数据（三态视图共享）。
  const [tasks, setTasks] = useState<TaskItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [refreshKey, setRefreshKey] = useState(0);

  // 顶栏搜索词（三态共享）。
  const [searchKeyword, setSearchKeyword] = useState('');

  // 顶栏时间窗（三态共享）：null = 全部不过滤。
  // 默认 null 而非 24h：任务页是管理视角，默认收窄会让老任务「消失」；
  // 不持久化，与看板 hours 不持久化的现状一致（需求 031 结论 2C）。
  const [hours, setHours] = useState<number | null>(null);

  // 环路列表（用于新建任务 Modal 的下拉）。
  // 只列出 process_template_id 非空的环路（即带工艺模板的环路才能创建任务）。
  const [loops, setLoops] = useState<LoopLite[]>([]);
  const [createModalOpen, setCreateModalOpen] = useState(false);

  // 从 URL 读取当前选中的任务 id。
  // null = 列表态，非 null = 详情全屏态。
  const [selectedTaskId, setSelectedTaskId] = useState<number | null>(readSelectedTaskId);

  // 监听 popstate：浏览器前进/后退时同步 selectedTaskId。
  useEffect(() => {
    const onPopState = () => {
      setSelectedTaskId(readSelectedTaskId());
    };
    window.addEventListener('popstate', onPopState);
    return () => window.removeEventListener('popstate', onPopState);
  }, []);

  // 切换视图：写 localStorage 兜底 + replaceUrl 同步 URL（?view=），使形态可直达/分享。
  const handleViewChange = (mode: TasksViewMode) => {
    persistView(mode);
    replaceUrl('tasks', { view: mode });
  };

  // 拉取任务列表。
  // 这里拉全量（不带 status），三态视图各自在前端做筛选；
  // 后端目前不支持 search 参数，keyword 过滤也在前端做。
  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const data = await bundledApi.listTasks(wsId);
      setTasks(data);
      // 顺便拉环路列表用于新建 Modal。
      // listLoops 返回的是带 process_template_id 的环路（实现细节，过滤放外面）。
      const lpList = await listLoops(wsId);
      setLoops(
        lpList
          .filter((l) => l.process_template_id != null)
          // 049：透传工艺来源字段，供新建任务下拉显示「（#工艺ID 工艺名 版本）」；
          // listLoops 返回的 LoopListItem 已由后端注入这些字段，此前映射时被裁掉。
          .map((l) => ({
            id: l.id,
            name: l.name,
            process_template_id: l.process_template_id,
            process_template_display_name: l.process_template_display_name,
            process_template_name: l.process_template_name,
            process_template_version: l.process_template_version,
          })),
      );
    } catch (e) {
      message.error(`加载任务失败：${e instanceof Error ? e.message : String(e)}`);
      setTasks([]);
    } finally {
      setLoading(false);
    }
  }, [wsId]);

  // workspace 变化或手动刷新时重拉。
  // 不依赖 loading/tasks，避免 reload 自身变化触发循环。
  useEffect(() => {
    reload();
  }, [reload, refreshKey]);

  // 点击任务：通过路由跳转到详情页。
  // pushUrl 会更新 URL hash，但本组件 selectedTaskId 由 popstate 监听同步，
  // 所以这里手动 setSelectedTaskId 确保 SPA 内点击立即响应。
  // tab 可选（063）：点「待审批」标记时传 'exec'，详情直接落到执行历史 Tab 并自动展开待审批执行。
  const handleSelectTask = useCallback(
    (taskId: number | null, tab?: string) => {
      if (taskId == null) {
        // 返回列表：replaceUrl 避免详情页占历史栈；带 view 参数保持 URL 显式表达当前形态。
        replaceUrl('tasks', { view: viewMode });
        setSelectedTaskId(null);
      } else {
        pushUrl('tasks', { id: taskId, tab });
        setSelectedTaskId(taskId);
      }
    },
    [pushUrl, replaceUrl, viewMode],
  );

  // 新建任务 Modal 提交后回调：关闭 Modal + 刷新列表。
  const handleCreated = () => {
    setCreateModalOpen(false);
    setRefreshKey((k) => k + 1);
  };

  // 时间过滤：hours 为 null（全部）时原样透传；
  // 否则只保留 created_at 在最近 N 小时内的任务（所有状态统一过滤，需求 031 结论 1A）。
  // created_at 缺失/非法时视为不在窗口内，与 原 KanbanBoard 对非法时间 NaN-drop 的处理对齐。
  // 过滤放在页级而非各视图内：与 searchKeyword 的共享方式一致，三态视图 props 零改动。
  const timeFilteredTasks = useMemo(() => {
    if (hours == null) return tasks;
    const cutoff = Date.now() - hours * 3600 * 1000;
    return tasks.filter((t) => {
      const ts = t.created_at ? new Date(t.created_at).getTime() : NaN;
      return !Number.isNaN(ts) && ts >= cutoff;
    });
  }, [tasks, hours]);

  // —— 顶栏 extra ——
  // 搜索框：所有视图共享（Table/卡片在前端 filter，看板不做 keyword filter）。
  // 时间分段：TimeRangeSegmented（showAll 形态），页级按 created_at 过滤，三态共享。
  // 刷新按钮：自增 refreshKey 触发 useEffect 重拉。
  // Segmented：三态视图切换，与 原运行中心 的 Segmented 风格一致。
  // 新建按钮：打开 CreateTaskModal。
  const searchInput = (
    <Input
      allowClear
      size="small"
      placeholder="搜索任务标题或需求"
      prefix={<SearchOutlined />}
      value={searchKeyword}
      onChange={(e) => setSearchKeyword(e.target.value)}
      style={{ width: 220 }}
      data-testid="tasks-page-search"
    />
  );

  const reloadButton = (
    <Button
      size="small"
      icon={<ReloadOutlined />}
      onClick={() => setRefreshKey((k) => k + 1)}
      loading={loading}
    >
      刷新
    </Button>
  );

  const viewSwitch = (
    <Segmented
      size="small"
      value={viewMode}
      onChange={(v) => handleViewChange(v as TasksViewMode)}
      options={[
        { value: 'list', icon: <UnorderedListOutlined />, title: '列表' },
        { value: 'kanban', icon: <AppstoreOutlined />, title: '看板' },
        { value: 'card', icon: <LayoutOutlined />, title: '卡片' },
      ]}
      data-testid="tasks-view-toggle"
    />
  );

  const createButton = (
    <Button
      size="small"
      type="primary"
      icon={<PlusOutlined />}
      onClick={() => setCreateModalOpen(true)}
    >
      新建
    </Button>
  );

  // 详情态顶栏：返回按钮由 PageCard onBack 统一渲染在 extra 最右端（062）。
  // 列表/看板/卡片态顶栏 extra：搜索 + 时间分段 + 刷新 + 视图切换 + 新建。
  const isDetail = selectedTaskId != null;

  // 062：详情态动态标题「任务 #id: 标题」，标题由 TaskDetailPanel 加载后上报。
  const [detailTitle, setDetailTitle] = useState<string | null>(null);

  // 切换任务/返回列表时重置标题，避免新任务数据未加载时闪现旧任务标题。
  useEffect(() => {
    setDetailTitle(null);
  }, [selectedTaskId]);

  // 时间分段：搜索框之后（与看板页顶栏顺序一致：搜索 → 时间分段）。
  // showAll 形态提供「全部」选项，value null 表示不过滤。
  const timeRangeSegment = (
    <TimeRangeSegmented showAll value={hours} onChange={setHours} />
  );

  const listExtra = (
    <>
      {searchInput}
      {timeRangeSegment}
      {reloadButton}
      {viewSwitch}
      {createButton}
    </>
  );

  // 工作空间切换时退出详情态：详情 id 属于旧工作空间，继续停留无意义。
  // 用 ref 记录上次 workspaceId，变化时清空 URL id 并回到列表态。
  const prevWsRef = useRef(wsId);
  useEffect(() => {
    if (prevWsRef.current !== wsId) {
      prevWsRef.current = wsId;
      if (selectedTaskId != null) {
        handleSelectTask(null);
      }
    }
  }, [wsId, selectedTaskId, handleSelectTask]);

  // —— 渲染分发 ——
  // 详情态分支整体思路（062 对齐 TaskDetailPage 独立路由页的表现）：
  // 1. 标题回退策略：TaskDetailPanel 数据未就绪时显示「任务 #id」，就绪后经 onTitleReady
  //    回传任务标题拼成「任务 #id: 标题」；切任务/回列表时由上方 effect 重置，避免闪现旧标题。
  // 2. 返回按钮：走 PageCard onBack（extra 最右端统一样式），点击调 handleSelectTask(null)
  //    用 replaceUrl 回列表路由，详情态不占历史条目。
  // 3. 布局：PageCard 全屏（flex:1），内容区高度扣掉 43px 页头后自滚动。
  // 列表/看板/卡片态：全屏单页 PageCard，根据 viewMode 渲染对应视图。
  if (isDetail) {
    return (
      <PageCard
        icon={<RocketOutlined />}
        title={detailTitle ?? `任务 #${selectedTaskId}`}
        onBack={() => handleSelectTask(null)}
        style={{ flex: 1, height: '100%' }}
        contentStyle={{ height: 'calc(100% - 43px)', overflow: 'auto' }}
      >
        <TaskDetailPanel
          taskId={selectedTaskId!}
          workspaceId={wsId}
          onTitleReady={(title) => setDetailTitle(`任务 #${selectedTaskId}: ${title}`)}
          onTriggered={() => setRefreshKey((k) => k + 1)}
        />
      </PageCard>
    );
  }

  // 列表态：PageCard 全屏 + TasksTableView。
  // contentStyle 与事项/环路列表页对齐：padding:0 消除表格与卡片边缘的默认
  // 16px/20px 间距（PageCard 内容区默认 padding），flex 列布局撑满高度。
  if (viewMode === 'list') {
    return (
      <>
        <PageCard
          icon={<RocketOutlined />}
          title="任务"
          extra={listExtra}
          style={{ flex: 1, height: '100%' }}
          contentStyle={{
            padding: 0,
            display: 'flex',
            flexDirection: 'column',
            height: 'calc(100% - 43px)',
            overflow: 'hidden',
          }}
        >
          <TasksTableView
            tasks={timeFilteredTasks}
            loading={loading}
            searchKeyword={searchKeyword}
            workspaceId={wsId}
            selectedTaskId={selectedTaskId}
            onSelectTask={handleSelectTask}
            // 批量删除成功后复用既有 refreshKey 链路重拉列表，避免子组件另造刷新入口。
            onChanged={() => setRefreshKey((k) => k + 1)}
          />
        </PageCard>
        <CreateTaskModal
          open={createModalOpen}
          workspaceId={wsId}
          loops={loops}
          onCreated={handleCreated}
          onCancel={() => setCreateModalOpen(false)}
        />
      </>
    );
  }

  // 看板态：PageCard 全屏 + TasksKanbanView。
  if (viewMode === 'kanban') {
    return (
      <>
        <PageCard
          icon={<RocketOutlined />}
          title="任务"
          extra={listExtra}
          style={{ flex: 1, height: '100%' }}
          contentStyle={{ height: 'calc(100% - 43px)', overflow: 'hidden' }}
        >
          <TasksKanbanView
            tasks={timeFilteredTasks}
            loading={loading}
            workspaceId={wsId}
            onSelectTask={handleSelectTask}
          />
        </PageCard>
        <CreateTaskModal
          open={createModalOpen}
          workspaceId={wsId}
          loops={loops}
          onCreated={handleCreated}
          onCancel={() => setCreateModalOpen(false)}
        />
      </>
    );
  }

  // 卡片态：PageCard 全屏 + TasksCardView。
  return (
    <>
      <PageCard
        icon={<RocketOutlined />}
        title="任务"
        extra={listExtra}
        style={{ flex: 1, height: '100%' }}
        contentStyle={{ height: 'calc(100% - 43px)', overflow: 'auto' }}
      >
        <TasksCardView
          tasks={timeFilteredTasks}
          loading={loading}
          searchKeyword={searchKeyword}
          workspaceId={wsId}
          onSelectTask={handleSelectTask}
        />
      </PageCard>
      <CreateTaskModal
        open={createModalOpen}
        workspaceId={wsId}
        loops={loops}
        onCreated={handleCreated}
        onCancel={() => setCreateModalOpen(false)}
      />
    </>
  );
}
