import { useCallback, useEffect, useMemo, useState } from 'react';
import { Button, Empty, Input, Segmented, Spin, message } from 'antd';
import { AppstoreOutlined, PlusOutlined, ReloadOutlined, SearchOutlined } from '@ant-design/icons';
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

interface TodoCenterPageProps {
  /** 点击卡片进入现有事项详情页。 */
  onSelectTodo: (id: number) => void;
  /** 新建事项入口（复用全局 TodoDrawer）。 */
  onOpenCreateModal: () => void;
}

/**
 * 事项中心页面：五类驱动视图（手动/时间/事件/Loop/已归档）。
 *
 * 一次拉取全部分类（后端批量补算 computed_bucket / loop 引用计数 / 最近执行），
 * 前端按 computed_bucket 分桶并展示各 Tab 数量；切换 Tab 不再发请求，降低交互延迟。
 * 卡片点击进入现有 TodoDetail，详情页不在第一阶段重写。
 */
export function TodoCenterPage({ onSelectTodo, onOpenCreateModal }: TodoCenterPageProps) {
  const { state } = useApp();
  const workspaceId = state.selectedWorkspace ?? undefined;

  const [items, setItems] = useState<TodoCenterItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [activeBucket, setActiveBucket] = useState<ComputedBucket>('manual');
  const [search, setSearch] = useState('');

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

  // 当前 Tab 的卡片：先按分类，再按搜索词（标题/prompt 子串）过滤
  const visibleItems = useMemo(() => {
    const kw = search.trim().toLowerCase();
    return items.filter((it) => {
      if (it.computed_bucket !== activeBucket) return false;
      if (!kw) return true;
      return it.title.toLowerCase().includes(kw) || it.prompt.toLowerCase().includes(kw);
    });
  }, [items, activeBucket, search]);

  // 若当前 Tab 因操作后变空（如归档了最后一项），自动切回手动触发，避免停在空 Tab
  useEffect(() => {
    if (bucketCount[activeBucket] === 0 && bucketCount.manual > 0) {
      setActiveBucket('manual');
    }
  }, [bucketCount, activeBucket]);

  // 切换工作空间后重置 Tab 到手动触发，避免停留在上个工作空间可能为空的分类
  useEffect(() => {
    setActiveBucket('manual');
  }, [workspaceId]);

  return (
    <PageCard
      icon={<AppstoreOutlined />}
      title="事项中心"
      extra={
        <>
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
          <Button size="small" icon={<ReloadOutlined />} onClick={reload} loading={loading} aria-label="刷新">
            刷新
          </Button>
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
              />
            ))}
          </div>
        )}
      </Spin>
    </PageCard>
  );
}
