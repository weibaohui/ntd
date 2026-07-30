// LoopListPage — 028-列表详情独立路由：环路列表独立页容器。
//
// 设计要点（028-列表详情独立路由-设计 §4.1）：
// 1. 替代原 LoopPage 的列表部分；环路详情已独立到 LoopDetailPage（URL: /#/loops/:id）。
// 2. 拉取环路列表 → 注入 LoopListView 渲染 table。
// 3. 顶部 header：搜索框 + 刷新 + 工作空间配置入口（原 LoopMobilePage 的
//    WorkspaceLoopConfigPage 入口迁到这里，保持列表页能直接管理评审模板）。
//    044：环路只由工艺 install/upgrade 产生，「新建」按钮已下线。
// 4. 监听 loopUpdateCount 触发重拉，让外部（如 LoopDetailPage 删了一个环路）能联动刷新。
// 5. 单函数 ≤ 30 行：数据拉取/过滤/回调已拆到子模块。

import { useCallback, useEffect, useMemo, useState } from 'react';
import { message } from 'antd';
import { RetweetOutlined } from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import { useApp } from '@/hooks/useApp';
import { PageCard } from '@/components/common/PageCard';
import { WorkspaceLoopConfigPage } from '@/components/settings/workspace/WorkspaceLoopConfigPage';
import { LoopListView } from './LoopListView';
import {
  LoopListHeader,
  useLoopRowActions,
  useLoopConfig,
} from './LoopListPageParts';
import type { LoopListItem } from '@/types/loop';

/** 环路列表数据加载 hook：响应 workspace/loopUpdateCount 变化。 */
function useLoopListData(workspaceId: number | null, loopUpdateCount: number) {
  const [items, setItems] = useState<LoopListItem[]>([]);
  const [loading, setLoading] = useState(false);

  // reload 用 useCallback 保证引用稳定，避免 useEffect 依赖循环
  const reload = useCallback(async () => {
    if (workspaceId == null) { setItems([]); return; }
    setLoading(true);
    try {
      const data = await dbLoops.listLoops(workspaceId);
      setItems(data);
    } catch (e) {
      message.error(`加载环路列表失败：${e instanceof Error ? e.message : String(e)}`);
      setItems([]);
    } finally {
      setLoading(false);
    }
  }, [workspaceId]);

  // 工作空间变化或外部通知时重拉
  useEffect(() => { reload(); }, [reload, loopUpdateCount]);

  return { items, loading, reload };
}

interface LoopListPageProps {
  /** 点击行跳转：由父组件 pushUrl('loops', { id })。 */
  onSelectLoop: (id: number) => void;
  /** 触发外部计数变化，让父组件刷新 LoopDetailPage 等。 */
  onLoopChanged?: () => void;
  /** 父组件维护的刷新信号（如 LoopDetailPage 删除后递增），变化时重拉列表。 */
  loopUpdateCount?: number;
}

/**
 * 环路列表独立页：URL `/#/loops`。
 *
 * 整体处理思路：
 * 1. 挂载时拉取环路列表；工作空间变化 / loopUpdateCount 变化时重拉。
 * 2. 顶部 header 提供搜索 + 刷新 + 新建 + 配置按钮（LoopListHeader）。
 * 3. 行操作回调拆到 useLoopRowActions；配置页入口拆到 useLoopConfig。
 * 4. 把过滤后的列表传给 LoopListView 渲染 table。
 */
export function LoopListPage({
  onSelectLoop,
  onLoopChanged,
  loopUpdateCount = 0,
}: LoopListPageProps) {
  const { state } = useApp();
  const workspaceId = state.selectedWorkspace;
  const [searchKeyword, setSearchKeyword] = useState('');

  // 列表数据加载 hook
  const { items, loading, reload } = useLoopListData(workspaceId, loopUpdateCount);

  // 行操作回调（已拆到 useLoopRowActions）
  // 044：触发/复制已随手工环路能力下线，只剩删除与启停
  const { handleDelete, handleToggleStatus } = useLoopRowActions({
    workspaceId, onReload: reload, onLoopChanged,
  });

  // 配置页入口（已拆到 useLoopConfig）
  const { loopConfigOpen, currentWorkspace, handleOpenLoopConfig, handleCloseLoopConfig } = useLoopConfig({
    workspaceId,
  });

  // 工作空间切换时关闭配置页
  useEffect(() => { handleCloseLoopConfig(); }, [workspaceId, handleCloseLoopConfig]);

  // 按搜索词过滤：useMemo 避免每次渲染都重新计算
  const filteredItems = useMemo(() => {
    const kw = searchKeyword.trim().toLowerCase();
    if (!kw) return items;
    return items.filter(l => (l.name || '').toLowerCase().includes(kw));
  }, [items, searchKeyword]);

  // 配置态：渲染 WorkspaceLoopConfigPage 替代列表
  if (loopConfigOpen && currentWorkspace) {
    return (
      <WorkspaceLoopConfigPage workspace={currentWorkspace} onBack={handleCloseLoopConfig} />
    );
  }

  // 列表态：PageCard + LoopListView；header 内联避免额外函数
  return (
    <PageCard
      icon={<RetweetOutlined />}
      title="环路"
      extra={
        <LoopListHeader
          searchKeyword={searchKeyword}
          loading={loading}
          workspaceId={workspaceId}
          onSearchChange={setSearchKeyword}
          onReload={reload}
          onOpenConfig={handleOpenLoopConfig}
        />
      }
      style={{ flex: 1, height: '100%' }}
      contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(100% - 43px)' }}
    >
      <LoopListView
        items={filteredItems}
        loading={loading}
        tags={state.tags}
        onSelectLoop={onSelectLoop}
        onDelete={handleDelete}
        onToggleStatus={handleToggleStatus}
        onRefresh={reload}
      />
    </PageCard>
  );
}
