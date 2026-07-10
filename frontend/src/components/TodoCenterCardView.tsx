import type { ReactNode } from 'react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { Empty, Segmented, Select, Spin, message } from 'antd';
import { AppstoreOutlined } from '@ant-design/icons';
import { useApp } from '@/hooks/useApp';
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
  extra,
  refreshKey,
}: TodoCenterCardViewProps) {
  const { state } = useApp();
  const workspaceId = state.selectedWorkspace ?? undefined;

  // 全量事项（后端已按 computed_bucket 分桶补算），前端再做筛选/分组
  const [items, setItems] = useState<TodoCenterItem[]>([]);
  // 加载态控制 Spin + 刷新按钮 loading
  const [loading, setLoading] = useState(false);
  // 当前 Tab（五类驱动），默认手动触发；持久化到 localStorage 记住用户上次选择
  const [activeBucket, setActiveBucket] = useState<ComputedBucket>(() => {
    try {
      return (localStorage.getItem('ntd_items_tab') as ComputedBucket) || 'manual';
    } catch {
      return 'manual';
    }
  });
  // 状态筛选（设计文档工具栏「状态筛选」）：'all' 或具体 status
  const [statusFilter, setStatusFilter] = useState<string>('all');
  // 动作类型筛选（设计文档工具栏「动作类型筛选」）：'all' 或具体 action_type
  const [actionTypeFilter, setActionTypeFilter] = useState<string>('all');

  // 拉取事项中心列表。工作空间变化或手动刷新时触发；
  // 卡片操作（归档/恢复/webhook/执行）完成后也会调它重拉，保持口径一致。
  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const data = await db.getTodoCenter(workspaceId);
      setItems(data);
    } catch (e) {
      message.error(`加载事项中心失败：${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setLoading(false);
    }
  }, [workspaceId]);

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
    window.addEventListener('todoListRefresh', handler);
    return () => window.removeEventListener('todoListRefresh', handler);
  }, [reload]);

  // 按 computed_bucket 分桶，用于 Tab 计数与卡片过滤
  const bucketCount = useMemo(() => {
    const counts: Record<ComputedBucket, number> = {
      manual: 0, time_driven: 0, event_driven: 0, loop_driven: 0, archived: 0,
    };
    for (const it of items) counts[it.computed_bucket]++;
    return counts;
  }, [items]);

  // 当前 Tab 的卡片：按分类 + 搜索 + 状态 + 动作类型过滤
  const visibleItems = useMemo(() => {
    const kw = searchKeyword.trim().toLowerCase();
    return items.filter((it) => {
      if (it.computed_bucket !== activeBucket) return false;
      if (statusFilter !== 'all' && it.status !== statusFilter) return false;
      if (actionTypeFilter !== 'all' && (it.action_type ?? 'none') !== actionTypeFilter) return false;
      if (!kw) return true;
      return it.title.toLowerCase().includes(kw) || it.prompt.toLowerCase().includes(kw);
    });
  }, [items, activeBucket, searchKeyword, statusFilter, actionTypeFilter]);

  // 动作类型筛选项：从当前数据动态去重，避免硬编码漏掉新类型
  const actionTypeOptions = useMemo(() => {
    const set = new Set<string>();
    for (const it of items) if (it.action_type) set.add(it.action_type);
    return Array.from(set);
  }, [items]);

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
            onChange={(val) => setActiveBucket(val as ComputedBucket)}
            options={BUCKETS.map((b) => ({
              label: (
                <span data-testid={`todo-center-tab-${b.value}`}>
                  {b.label} <span className="todo-center-tab-count">{bucketCount[b.value]}</span>
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
                onChange={setStatusFilter}
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
                onChange={setActionTypeFilter}
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
        )}
      </Spin>
    </PageCard>
  );
}
