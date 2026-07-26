// LoopListPage — 028-列表详情独立路由：环路列表独立页容器。
//
// 设计要点（028-列表详情独立路由-设计 §4.1）：
// 1. 替代原 LoopPage 的列表部分；环路详情已独立到 LoopDetailPage（URL: /#/loops/:id）。
// 2. 拉取环路列表 → 注入 LoopListView 渲染 table。
// 3. 顶部 header：搜索框 + 刷新 + 新建 + 工作空间配置入口（原 LoopMobilePage 的
//    WorkspaceLoopConfigPage 入口迁到这里，保持列表页能直接管理评审模板）。
// 4. 监听 loopUpdateCount 触发重拉，让外部（如 LoopDetailPage 删了一个环路）能联动刷新。
// 5. 单函数 ≤ 30 行：数据拉取、过滤、回调拆为独立函数。

import { useCallback, useEffect, useMemo, useState } from 'react';
import { Input, message } from 'antd';
import { PlusOutlined, RetweetOutlined, SearchOutlined, SettingOutlined } from '@ant-design/icons';
import { Button } from 'antd';
import * as dbLoops from '@/utils/database/loops';
import { getProjectDirectories, type ProjectDirectory } from '@/utils/database/todos';
import { useApp } from '@/hooks/useApp';
import { PageCard } from '@/components/common/PageCard';
import { WorkspaceLoopConfigPage } from '@/components/settings/workspace/WorkspaceLoopConfigPage';
import { LoopListView } from './LoopListView';
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
 * 2. 顶部 header 提供搜索 + 刷新 + 新建 + 配置按钮。
 * 3. 把过滤后的列表传给 LoopListView 渲染 table，行操作回调转给父组件。
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
  // 工作空间环路配置页显隐：原 LoopMobilePage 的入口迁到此处，
  // 用 useState 切换显示 WorkspaceLoopConfigPage 替代 LoopListView
  const [loopConfigOpen, setLoopConfigOpen] = useState(false);
  // 当前工作空间对象：WorkspaceLoopConfigPage 需要 name 渲染标题，
  // 在用户点击「配置」时按需拉取一次，避免列表页常驻拉 projectDirectories
  const [currentWorkspace, setCurrentWorkspace] = useState<ProjectDirectory | null>(null);

  // 拉取环路列表：工作空间变化、loopUpdateCount 变化时触发
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

  useEffect(() => {
    reload();
  }, [reload, loopUpdateCount]);

  // 按搜索词过滤：环路名称包含关键字（不区分大小写）
  const filteredItems = useMemo(() => {
    const kw = searchKeyword.trim().toLowerCase();
    if (!kw) return items;
    return items.filter(l => (l.name || '').toLowerCase().includes(kw));
  }, [items, searchKeyword]);

  // 单行触发：调 API 后通知父组件刷新
  const handleTrigger = useCallback(async (loop: LoopListItem) => {
    if (workspaceId == null) return;
    try {
      const res = await dbLoops.triggerLoop(workspaceId, loop.id);
      message.success(`已触发 (execution #${res.execution_id})`);
      onLoopChanged?.();
    } catch (e) {
      message.error(`触发失败: ${e instanceof Error ? e.message : '未知错误'}`);
    }
  }, [workspaceId, onLoopChanged]);

  // 单行复制：调 API 后重拉列表
  const handleDuplicate = useCallback(async (loop: LoopListItem) => {
    if (workspaceId == null) return;
    try {
      await dbLoops.duplicateLoop(workspaceId, loop.id);
      message.success('已复制');
      reload();
      onLoopChanged?.();
    } catch (e) {
      message.error(`复制失败: ${e instanceof Error ? e.message : '未知错误'}`);
    }
  }, [workspaceId, reload, onLoopChanged]);

  // 单行删除：调 API 后重拉列表 + 通知父组件（避免 LoopDetailPage 仍指向已删除 id）
  const handleDelete = useCallback(async (loop: LoopListItem) => {
    if (workspaceId == null) return;
    try {
      await dbLoops.deleteLoop(workspaceId, loop.id);
      message.success('已删除');
      reload();
      onLoopChanged?.();
    } catch {
      message.error('删除失败，环路可能正在被引用');
    }
  }, [workspaceId, reload, onLoopChanged]);

  // 切换启用/暂停状态
  const handleToggleStatus = useCallback(async (loop: LoopListItem) => {
    if (workspaceId == null) return;
    try {
      const next = loop.status === 'enabled' ? 'paused' : 'enabled';
      await dbLoops.updateLoopStatus(workspaceId, loop.id, { status: next });
      message.success(`已${next === 'enabled' ? '启用' : '暂停'}`);
      reload();
      onLoopChanged?.();
    } catch (e) {
      message.error(`状态切换失败: ${e instanceof Error ? e.message : '未知错误'}`);
    }
  }, [workspaceId, reload, onLoopChanged]);

  // 打开工作空间环路配置页：拉取 ProjectDirectory 找到当前 workspace 后切换显示
  // 这里只拉一次（用户点击时），列表态本身不需要 projectDirectories 数据
  const handleOpenLoopConfig = useCallback(async () => {
    if (workspaceId == null) return;
    try {
      const dirs = await getProjectDirectories();
      const found = dirs.find(d => d.id === workspaceId);
      if (!found) {
        message.warning('未找到当前工作空间');
        return;
      }
      setCurrentWorkspace(found);
      setLoopConfigOpen(true);
    } catch (e) {
      message.error(`加载工作空间失败：${e instanceof Error ? e.message : String(e)}`);
    }
  }, [workspaceId]);

  // 顶部 header：搜索框 + 刷新 + 配置 + 新建
  const headerExtra = (
    <>
      <Input
        allowClear
        size="small"
        placeholder="搜索环路名称"
        prefix={<SearchOutlined />}
        value={searchKeyword}
        onChange={(e) => setSearchKeyword(e.target.value)}
        style={{ width: 200 }}
        data-testid="loop-list-search"
      />
      <Button
        size="small"
        icon={<SettingOutlined />}
        onClick={handleOpenLoopConfig}
        disabled={workspaceId == null}
      >
        配置
      </Button>
      <Button
        size="small"
        icon={<RetweetOutlined />}
        onClick={reload}
        loading={loading}
        aria-label="刷新"
      >
        刷新
      </Button>
      <Button
        size="small"
        type="primary"
        icon={<PlusOutlined />}
        onClick={onCreateLoop}
      >
        新建
      </Button>
    </>
  );

  // 配置态：渲染 WorkspaceLoopConfigPage 替代列表，onBack 返回列表态
  // 列表态保持原 PageCard + LoopListView 结构
  if (loopConfigOpen && currentWorkspace) {
    return (
      <WorkspaceLoopConfigPage
        workspace={currentWorkspace}
        onBack={() => {
          setLoopConfigOpen(false);
          setCurrentWorkspace(null);
        }}
      />
    );
  }

  return (
    <PageCard
      icon={<RetweetOutlined />}
      title="环路"
      extra={headerExtra}
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
}
