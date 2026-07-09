import { useCallback, useEffect, useMemo, useState } from 'react';
import { Button, Empty, Input, Segmented, Select, Spin, message } from 'antd';
import { AppstoreOutlined, PlusOutlined, ReloadOutlined, SearchOutlined, UnorderedListOutlined } from '@ant-design/icons';
import { useApp } from '@/hooks/useApp';
import { PageCard } from '@/components/common/PageCard';
import { TodoCenterCard } from '@/components/TodoCenterCard';
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
  /** 新建事项入口（复用全局 TodoDrawer）。 */
  onOpenCreateModal: () => void;
  /** 当前视图模式（卡片/列表），由宿主持有；卡片页只在 header 里展示切换器。 */
  viewMode: 'card' | 'list';
  onViewModeChange: (m: 'card' | 'list') => void;
  /** 移动端：精简 header（隐藏搜索/筛选），保留切换器 + 新建 + Tab + 卡片。 */
  isMobile?: boolean;
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
  onOpenCreateModal,
  viewMode,
  onViewModeChange,
  isMobile,
}: TodoCenterCardViewProps) {
  const { state } = useApp();
  const workspaceId = state.selectedWorkspace ?? undefined;

  // 全量事项（后端已按 computed_bucket 分桶补算），前端再做筛选/分组
  const [items, setItems] = useState<TodoCenterItem[]>([]);
  // 加载态控制 Spin + 刷新按钮 loading
  const [loading, setLoading] = useState(false);
  // 当前 Tab（五类驱动），默认手动触发
  const [activeBucket, setActiveBucket] = useState<ComputedBucket>('manual');
  // 搜索词：标题/prompt 子串，前端即时过滤（数据全量在端，无需回服务端）
  const [search, setSearch] = useState('');
  // 状态筛选（设计文档工具栏「状态筛选」）：'all' 或具体 status
  const [statusFilter, setStatusFilter] = useState<string>('all');
  // 动作类型筛选（设计文档工具栏「动作类型筛选」）：'all' 或具体 action_type
  const [actionTypeFilter, setActionTypeFilter] = useState<string>('all');
  // 手动触发 Tab 专属：仅看绑定了斜杠命令的可命令触发事项（设计文档 manual 筛选项）
  const [commandOnly, setCommandOnly] = useState(false);

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
  }, [reload]);

  // 按 computed_bucket 分桶，用于 Tab 计数与卡片过滤
  const bucketCount = useMemo(() => {
    const counts: Record<ComputedBucket, number> = {
      manual: 0, time_driven: 0, event_driven: 0, loop_driven: 0, archived: 0,
    };
    for (const it of items) counts[it.computed_bucket]++;
    return counts;
  }, [items]);

  // 当前 Tab 的卡片：按分类 + 搜索 + 状态 + 动作类型 + （手动 Tab）命令绑定过滤
  const visibleItems = useMemo(() => {
    const kw = search.trim().toLowerCase();
    return items.filter((it) => {
      if (it.computed_bucket !== activeBucket) return false;
      if (statusFilter !== 'all' && it.status !== statusFilter) return false;
      if (actionTypeFilter !== 'all' && (it.action_type ?? 'none') !== actionTypeFilter) return false;
      // 手动触发 Tab 的「仅看可命令触发」：只留绑定了斜杠命令的事项
      if (commandOnly && activeBucket === 'manual' && !it.bound_slash_command) return false;
      if (!kw) return true;
      return it.title.toLowerCase().includes(kw) || it.prompt.toLowerCase().includes(kw);
    });
  }, [items, activeBucket, search, statusFilter, actionTypeFilter, commandOnly]);

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
      extra={
        <>
          {/* 桌面端才放搜索框；移动端 header 精简，避免拥挤 */}
          {!isMobile && (
            <Input
              allowClear
              size="small"
              placeholder="搜索标题或 Prompt"
              prefix={<SearchOutlined />}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              style={{ width: 200 }}
              data-testid="todo-center-search"
            />
          )}
          {/* 卡片/列表切换：列表形态切到原 TodoPage（双栏），由宿主 ItemsPage 控制 */}
          <Segmented
            size="small"
            value={viewMode}
            onChange={(v) => onViewModeChange(v as 'card' | 'list')}
            options={[
              { value: 'card', icon: <AppstoreOutlined />, title: '卡片视图' },
              { value: 'list', icon: <UnorderedListOutlined />, title: '列表（双栏）' },
            ]}
            data-testid="todo-center-view-toggle"
          />
          {!isMobile && (
            <Button size="small" icon={<ReloadOutlined />} onClick={reload} loading={loading} aria-label="刷新">
              刷新
            </Button>
          )}
          <Button size="small" type="primary" icon={<PlusOutlined />} onClick={onOpenCreateModal}>
            新建
          </Button>
        </>
      }
      contentClassName="todo-center-page-content"
    >
      <Spin spinning={loading}>
        <div className="todo-center-tabs">
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
        </div>

        {/* 筛选栏（设计文档工具栏：状态筛选 + 动作类型筛选；手动 Tab 额外的「仅看可命令触发」）。
            移动端隐藏——空间有限，手机端主要浏览 Tab + 卡片，筛选留到桌面端。 */}
        {!isMobile && (
        <div className="todo-center-filters">
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
            options={[{ value: 'all', label: '全部来源' }, ...actionTypeOptions.map((t) => ({ value: t, label: t }))]}
            data-testid="todo-center-action-filter"
          />
          {activeBucket === 'manual' && (
            <label className="todo-center-cmd-only" data-testid="todo-center-command-only">
              <input
                type="checkbox"
                checked={commandOnly}
                onChange={(e) => setCommandOnly(e.target.checked)}
              />
              仅看可命令触发
            </label>
          )}
        </div>
        )}

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
