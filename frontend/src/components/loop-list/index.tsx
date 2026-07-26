// LoopListPage — 028-列表详情独立路由：环路列表独立页容器。
//
// 设计要点（028-列表详情独立路由-设计 §4.1）：
// 1. 替代原 LoopPage 的列表部分；环路详情已独立到 LoopDetailPage（URL: /#/loops/:id）。
// 2. 拉取环路列表 → 注入 LoopListView 渲染 table。
// 3. 顶部 header：搜索框 + 刷新 + 新建 + 工作空间配置入口（原 LoopMobilePage 的
//    WorkspaceLoopConfigPage 入口迁到这里，保持列表页能直接管理评审模板）。
// 4. 监听 loopUpdateCount 触发重拉，让外部（如 LoopDetailPage 删了一个环路）能联动刷新。
// 5. 单函数 ≤ 30 行：数据拉取、过滤、回调已拆到 LoopListPageParts。

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

interface LoopListPageProps {
  /** 新建环路入口（顶部「新建」按钮）。 */
  onCreateLoop: () => void;
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
 * 2. 顶部 header 提供搜索 + 刷新 + 新建 + 配置按钮（已拆到 LoopListHeader）。
 * 3. 行操作回调拆到 useLoopRowActions；配置页入口拆到 useLoopConfig。
 * 4. 把过滤后的列表传给 LoopListView 渲染 table。
 */
export function LoopListPage({
  onCreateLoop,
  onSelectLoop,
  onLoopChanged,
  loopUpdateCount = 0,
}: LoopListPageProps) {
  const { state } = useApp();
  const workspaceId = state.selectedWorkspace;
  const [items, setItems] = useState<LoopListItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [searchKeyword, setSearchKeyword] = useState('');

  // 拉取环路列表：useCallback 保证引用稳定，避免 useEffect 依赖循环导致无限闪动
  // 依赖 workspaceId：工作空间切换时重新拉取
  const reload = useCallback(async () => {
    if (workspaceId == null) {
      setItems([]);
      return;
    }
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

  // 行操作回调（已拆到 useLoopRowActions）
  const { handleTrigger, handleDuplicate, handleDelete, handleToggleStatus } = useLoopRowActions({
    workspaceId,
    onReload: reload,
    onLoopChanged,
  });

  // 配置页入口（已拆到 useLoopConfig）
  const { loopConfigOpen, currentWorkspace, handleOpenLoopConfig, handleCloseLoopConfig } = useLoopConfig({
    workspaceId,
  });

  // 拉取环路列表：工作空间变化、loopUpdateCount 变化时触发
  // reload 已用 useCallback 稳定引用，不会每次渲染都触发
  useEffect(() => {
    reload();
  }, [reload, loopUpdateCount]);

  // 工作空间切换时关闭配置页 + 清空 currentWorkspace，
  // 避免 WorkspaceLoopConfigPage 继续按旧工作空间渲染
  useEffect(() => {
    handleCloseLoopConfig();
  }, [workspaceId, handleCloseLoopConfig]);

  // 按搜索词过滤：useMemo 避免每次渲染都重新计算
  const filteredItems = useMemo(() => {
    const kw = searchKeyword.trim().toLowerCase();
    if (!kw) return items;
    return items.filter(l => (l.name || '').toLowerCase().includes(kw));
  }, [items, searchKeyword]);

  // 配置态：渲染 WorkspaceLoopConfigPage 替代列表
  if (loopConfigOpen && currentWorkspace) {
    return (
      <WorkspaceLoopConfigPage
        workspace={currentWorkspace}
        onBack={handleCloseLoopConfig}
      />
    );
  }

  // 列表态：PageCard + LoopListView
  return (
    <PageCard
      icon={<RetweetOutlined />}
      title="环路"
      extra={renderHeader()}
      style={{ flex: 1, height: '100%' }}
      contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(100% - 43px)' }}
    >
      <LoopListView
        items={filteredItems}
        loading={loading}
        tags={state.tags}
        onSelectLoop={onSelectLoop}
        onTrigger={handleTrigger}
        onDuplicate={handleDuplicate}
        onDelete={handleDelete}
        onToggleStatus={handleToggleStatus}
        onRefresh={reload}
      />
    </PageCard>
  );

  /** 顶部 header：搜索 + 配置 + 刷新 + 新建。 */
  function renderHeader() {
    return (
      <LoopListHeader
        searchKeyword={searchKeyword}
        loading={loading}
        workspaceId={workspaceId}
        onSearchChange={setSearchKeyword}
        onReload={reload}
        onCreate={onCreateLoop}
        onOpenConfig={handleOpenLoopConfig}
      />
    );
  }
}