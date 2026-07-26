// LoopListPage — 028-列表详情独立路由：环路列表独立页容器。
//
// 设计要点（028-列表详情独立路由-设计 §4.1）：
// 1. 替代原 LoopPage 的列表部分；环路详情已独立到 LoopDetailPage（URL: /#/loops/:id）。
// 2. 拉取环路列表 → 注入 LoopListView 渲染 table。
// 3. 顶部 header：搜索框 + 刷新 + 新建 + 工作空间配置入口（原 LoopMobilePage 的
//    WorkspaceLoopConfigPage 入口迁到这里，保持列表页能直接管理评审模板）。
// 4. 监听 loopUpdateCount 触发重拉，让外部（如 LoopDetailPage 删了一个环路）能联动刷新。
// 5. 单函数 ≤ 30 行：数据拉取、过滤、回调已拆到 LoopListPageParts。

import { useEffect, useState } from 'react';
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
  useEffect(() => {
    reload();
  }, [reload, loopUpdateCount]);

  // 工作空间切换时关闭配置页 + 清空 currentWorkspace，
  // 避免 WorkspaceLoopConfigPage 继续按旧工作空间渲染
  useEffect(() => {
    handleCloseLoopConfig();
  }, [workspaceId, handleCloseLoopConfig]);

  return (
    <>
      {renderContent()}
    </>
  );

  // ─── 以下为内部函数，拆分以保持主函数体 ≤30 行 ───

  /** 拉取环路列表：workspaceId 为空时返回空列表。 */
  async function reload() {
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
  }

  /** 按搜索词过滤：环路名称包含关键字（不区分大小写）。 */
  function filterItems(): LoopListItem[] {
    const kw = searchKeyword.trim().toLowerCase();
    if (!kw) return items;
    return items.filter(l => (l.name || '').toLowerCase().includes(kw));
  }

  /** 渲染主内容：配置态显示 WorkspaceLoopConfigPage；列表态显示 PageCard + LoopListView。 */
  function renderContent() {
    const headerExtra = (
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
        extra={headerExtra}
        style={{ flex: 1, height: '100%' }}
        contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(100% - 43px)' }}
      >
        <LoopListView
          items={filterItems()}
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
  }
}