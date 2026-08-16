import type { ReactNode } from 'react';
import { useCallback, useEffect, useState } from 'react';
import { Empty, Pagination, Segmented, Select, Spin, message } from 'antd';
import { AppstoreOutlined } from '@ant-design/icons';
import { TODO_LIST_REFRESH_EVENT } from '@/constants';
// 093：本组件只消费 todo 域状态，用细粒度 useTodos 替代合并版 useApp，
// 执行态（进度/统计推送）变化不再触发本组件重渲染。
import { useTodos } from '@/hooks/useTodoContext';
import { PageCard } from '@/components/common/PageCard';
import { TodoCenterCard, sourceLabel } from '@/components/TodoCenterCard';
import * as db from '@/utils/database';
import type { ComputedBucket, TodoCenterItem } from '@/types';

/** 五类驱动 Tab 的展示顺序与中文标签。顺序即默认 Tab 优先级（手动触发在前）。 */
const BUCKETS: { value: ComputedBucket; label: string }[] = [
  { value: 'manual', label: '手动触发' },
  { value: 'time_driven', label: '时间驱动' },
  { value: 'event_driven', label: '事件驱动' },
  { value: 'loop_driven', label: 'Loop 驱动' },
  { value: 'archived', label: '已归档' },
];

const EMPTY_TEXT: Record<ComputedBucket, string> = {
  manual: '暂无手动触发事项',
  time_driven: '暂无时间驱动事项',
  event_driven: '暂无事件驱动事项',
  loop_driven: '暂无被 Loop 引用的事项',
  archived: '暂无已归档事项',
};

interface TodoCenterCardViewProps {
  /** 点击卡片：由宿主（ItemsPage）包装为「选中并切到列表模式打开详情」。 */
  onSelectTodo: (id: number) => void;
  /** 点击所属 Loop 跳转 Loop 详情。 */
  onSelectLoop: (loopId: number) => void;
  /** 移动端：精简 header（隐藏搜索/筛选），保留切换器 + 新建 + Tab + 卡片。 */
  isMobile?: boolean;
  /** 统一搜索词（来自 ItemsPage 顶层搜索框），由 ItemsPage 负责渲染输入框。 */
  searchKeyword?: string;
  /** 111：时间窗（null=全部），与列表形态共用 TodoListPage 的页级 state，下推服务端过滤。 */
  hours?: number | null;
  /** ItemsPage 顶层构建的完整 header extra（搜索框 + 刷新 + Segmented + 新建）。 */
  extra?: ReactNode;
  /** 刷新信号，ItemsPage 点击刷新按钮时自增，触发本组件重载数据。 */
  refreshKey?: number;
}

/**
 * 事项中心卡片视图：五类驱动（手动/时间/事件/Loop/已归档）的卡片墙。
 *
 * 它是合并后「事项」页的卡片形态；列表形态由 ItemsPage 切到原 TodoPage（双栏）。
 * 一次拉取全部分类（后端批量补算 computed_bucket / loop 引用计数 / 最近执行），
 * 前端按 computed_bucket 分桶并展示各 Tab 数量；切换 Tab 不再发请求，降低交互延迟。
 */
export function TodoCenterCardView({
  onSelectTodo,
  onSelectLoop,
  isMobile,
  searchKeyword = '',
  hours = null,
  extra,
  refreshKey,
}: TodoCenterCardViewProps) {
  const { state } = useTodos();
  // v1 纯 workspace-scoped：selectedWorkspace 必须有值才能拉 todos/center
  const workspaceId = state.selectedWorkspace ?? 0;

  // 056：服务端分页——items 仅当前页；bucket_counts/action_types 由后端聚合返回
  const [items, setItems] = useState<TodoCenterItem[]>([]);
  const [bucketCounts, setBucketCounts] = useState<Record<string, number>>({});
  const [actionTypeOptions, setActionTypeOptions] = useState<string[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(24);
  // 加载态控制 Spin + 刷新按钮 loading
  const [loading, setLoading] = useState(false);
  // 搜索词防抖（与列表页同口径：停顿 300ms 后再发请求）
  const [debouncedSearch, setDebouncedSearch] = useState('');
  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedSearch(searchKeyword.trim());
      setPage(1);
    }, 300);
    return () => clearTimeout(timer);
  }, [searchKeyword]);
  // 111：时间窗变化回第 1 页——窗口收窄后当前页可能超出有效页数，
  // 与筛选 setter 的回页策略一致，避免出现空页或截断页困惑。
  useEffect(() => {
    setPage(1);
  }, [hours]);
  // 当前 Tab（五类驱动），默认手动触发；持久化到 localStorage 记住用户上次选择
  const [activeBucket, setActiveBucket] = useState<ComputedBucket>(() => {
    try {
      return (localStorage.getItem('ntd_items_tab') as ComputedBucket) || 'manual';
    } catch {
      return 'manual';
    }
  });
  // 状态筛选（设计文档工具栏「状态筛选」下拉）：'all' 或具体 status
  const [statusFilter, setStatusFilter] = useState<string>('all');
  // 动作类型筛选（设计文档工具栏「动作类型筛选」下拉）：'all' 或具体 action_type
  const [actionTypeFilter, setActionTypeFilter] = useState<string>('all');

  // 筛选 setter 合并「回第 1 页」：与条件变更同批次提交（React 批处理后单次 render），
  // 避免「条件先变→用旧页码发请求→页码再变→再发一次」的双重请求与响应竞争（评审 C1）。
  const changeBucket = useCallback((b: ComputedBucket) => {
    setActiveBucket(b);
    setPage(1);
  }, []);
  const changeStatusFilter = useCallback((s: string) => {
    setStatusFilter(s);
    setPage(1);
  }, []);
  const changeActionTypeFilter = useCallback((t: string) => {
    setActionTypeFilter(t);
    setPage(1);
  }, []);

  // 拉取事项中心当前页（056：bucket/search/status/actionType/分页全部下推 SQL）。
  // 工作空间变化、筛选变化、翻页或手动刷新时触发；
  // 卡片操作（归档/恢复/webhook/执行）完成后也会调它重拉，保持口径一致。
  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const data = await db.getTodoCenter(workspaceId, {
        bucket: activeBucket,
        search: debouncedSearch || undefined,
        status: statusFilter,
        actionType: actionTypeFilter,
        page,
        pageSize,
        // 111：时间窗与列表形态同源，SQL 下推保证 Tab 角标（bucket_counts）同口径
        hours,
      });
      setItems(data.items);
      setTotal(data.total);
      setBucketCounts(data.bucket_counts);
      setActionTypeOptions(data.action_types);
    } catch (e) {
      message.error(`加载事项中心失败：${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setLoading(false);
    }
  }, [workspaceId, activeBucket, debouncedSearch, statusFilter, actionTypeFilter, page, pageSize, hours]);

  useEffect(() => {
    reload();
  }, [reload, refreshKey]);

  // activeBucket 变化时持久化到 localStorage
  useEffect(() => {
    try {
      localStorage.setItem('ntd_items_tab', activeBucket);
    } catch {
      /* localStorage 不可用时静默降级 */
    }
  }, [activeBucket]);

  // TodoDrawer 新建/保存事项后，通知卡片墙也刷新
  useEffect(() => {
    const handler = () => reload();
    window.addEventListener(TODO_LIST_REFRESH_EVENT, handler);
    return () => window.removeEventListener(TODO_LIST_REFRESH_EVENT, handler);
  }, [reload]);

  // 056：Tab 计数与卡片数据均来自服务端（bucket_counts 后端聚合），不再页内分桶
  const bucketCount = (b: ComputedBucket): number => bucketCounts[b] ?? 0;

  // 当前 Tab 的卡片即服务端当前页（bucket/search/status/actionType 过滤已在 SQL 层完成）
  const visibleItems = items;

  return (
    <PageCard
      icon={<AppstoreOutlined />}
      title="事项"
      // flex:1 让 PageCard 在 Content 的 flex-row 里撑满宽度，
      // 否则会塌缩成内容宽度（卡片只剩单列、右侧大片留白）
      style={{ flex: 1 }}
      extra={extra}
      contentClassName="todo-center-page-content"
    >
      <Spin spinning={loading}>
        {/* Tab 分段器 + 状态/来源筛选器同行排列，flex-wrap 让其按屏幕宽度自动换行 */}
        <div className="todo-center-tabs-toolbar">
          <Segmented
            value={activeBucket}
            onChange={(val) => changeBucket(val as ComputedBucket)}
            options={BUCKETS.map((b) => ({
              label: (
                <span data-testid={`todo-center-tab-${b.value}`}>
                  {b.label} <span className="todo-center-tab-count">{bucketCount(b.value)}</span>
                </span>
              ),
              value: b.value,
            }))}
          />

          {/* 移动端隐藏——空间有限，手机端主要浏览 Tab + 卡片，筛选留到桌面端。 */}
          {!isMobile && (
            <>
              <Select
                size="small"
                value={statusFilter}
                onChange={changeStatusFilter}
                style={{ width: 120 }}
                options={[
                  { value: 'all', label: '全部状态' },
                  { value: 'pending', label: '待执行' },
                  { value: 'running', label: '运行中' },
                  { value: 'completed', label: '已完成' },
                  { value: 'failed', label: '失败' },
                ]}
                data-testid="todo-center-status-filter"
              />
              <Select
                size="small"
                value={actionTypeFilter}
                onChange={changeActionTypeFilter}
                style={{ width: 140 }}
                options={[{ value: 'all', label: '全部来源' }, ...actionTypeOptions.map((t) => ({ value: t, label: sourceLabel(t) ?? t }))]}
                data-testid="todo-center-action-filter"
              />
            </>
          )}
        </div>

        {visibleItems.length === 0 ? (
          <Empty description={EMPTY_TEXT[activeBucket]} style={{ marginTop: 48 }} />
        ) : (
          <>
            <div className="todo-center-grid">
              {visibleItems.map((item) => (
                <TodoCenterCard
                  key={item.id}
                  item={item}
                  onChanged={reload}
                  onSelectTodo={onSelectTodo}
                  onSelectLoop={onSelectLoop}
                />
              ))}
            </div>
            {/* 056：卡片墙服务端分页——翻页只影响当前 Tab 的卡片集 */}
            <div style={{ display: 'flex', justifyContent: 'flex-end', padding: '12px 4px' }}>
              <Pagination
                current={page}
                pageSize={pageSize}
                total={total}
                onChange={(p, ps) => { setPage(ps !== pageSize ? 1 : p); setPageSize(ps); }}
                showSizeChanger
                pageSizeOptions={['24', '48', '96']}
                showTotal={(t) => `共 ${t} 项`}
                size="small"
              />
            </div>
          </>
        )}
      </Spin>
    </PageCard>
  );
}
