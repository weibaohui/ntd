// LoopListPageParts — LoopListPage 的拆分子模块（响应 028 PR review 的函数体 ≤30 行规范）。
//
// 拆分原则：把 header JSX、行操作回调、配置页入口三块独立成组件/hook，
// 让 LoopListPage 主函数仅负责组合，函数体保持简短。
//
// 1. LoopListHeader：顶部 header（搜索 + 配置 + 刷新 + 新建）
// 2. useLoopRowActions：单行触发/复制/删除/启停状态切换
// 3. useLoopConfig：工作空间环路配置页入口（拉取 ProjectDirectory + 切换显示）

import { useCallback, useState, type ReactNode } from 'react';
import { Button, Input, message } from 'antd';
import { PlusOutlined, RetweetOutlined, SearchOutlined, SettingOutlined } from '@ant-design/icons';
import * as dbLoops from '@/utils/database/loops';
import { getProjectDirectories, type ProjectDirectory } from '@/utils/database/todos';
import type { LoopListItem } from '@/types/loop';

interface LoopListHeaderProps {
  searchKeyword: string;
  loading: boolean;
  workspaceId: number | null;
  onSearchChange: (kw: string) => void;
  onReload: () => void;
  onCreate: () => void;
  onOpenConfig: () => void;
}

/**
 * 环路列表顶部 header：搜索框 + 配置 + 刷新 + 新建。
 * 拆出独立组件避免 LoopListPage 主函数膨胀。
 */
export function LoopListHeader({
  searchKeyword,
  loading,
  workspaceId,
  onSearchChange,
  onReload,
  onCreate,
  onOpenConfig,
}: LoopListHeaderProps): ReactNode {
  return (
    <>
      <Input
        allowClear
        size="small"
        placeholder="搜索环路名称"
        prefix={<SearchOutlined />}
        value={searchKeyword}
        onChange={(e) => onSearchChange(e.target.value)}
        style={{ width: 200 }}
        data-testid="loop-list-search"
      />
      <Button
        size="small"
        icon={<SettingOutlined />}
        onClick={onOpenConfig}
        disabled={workspaceId == null}
      >
        配置
      </Button>
      <Button
        size="small"
        icon={<RetweetOutlined />}
        onClick={onReload}
        loading={loading}
        aria-label="刷新"
      >
        刷新
      </Button>
      <Button
        size="small"
        type="primary"
        icon={<PlusOutlined />}
        onClick={onCreate}
      >
        新建
      </Button>
    </>
  );
}

interface UseLoopRowActionsArgs {
  workspaceId: number | null;
  onReload: () => void;
  onLoopChanged?: () => void;
}

/**
 * 环路行操作：触发 / 复制 / 删除 / 启停状态切换。
 * 拆成 hook 让 LoopListPage 主函数保持简短，便于测试与复用。
 */
export function useLoopRowActions({ workspaceId, onReload, onLoopChanged }: UseLoopRowActionsArgs) {
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

  const handleDuplicate = useCallback(async (loop: LoopListItem) => {
    if (workspaceId == null) return;
    try {
      await dbLoops.duplicateLoop(workspaceId, loop.id);
      message.success('已复制');
      onReload();
      onLoopChanged?.();
    } catch (e) {
      message.error(`复制失败: ${e instanceof Error ? e.message : '未知错误'}`);
    }
  }, [workspaceId, onReload, onLoopChanged]);

  const handleDelete = useCallback(async (loop: LoopListItem) => {
    if (workspaceId == null) return;
    try {
      await dbLoops.deleteLoop(workspaceId, loop.id);
      message.success('已删除');
      onReload();
      onLoopChanged?.();
    } catch {
      message.error('删除失败，环路可能正在被引用');
    }
  }, [workspaceId, onReload, onLoopChanged]);

  const handleToggleStatus = useCallback(async (loop: LoopListItem) => {
    if (workspaceId == null) return;
    try {
      const next = loop.status === 'enabled' ? 'paused' : 'enabled';
      await dbLoops.updateLoopStatus(workspaceId, loop.id, { status: next });
      message.success(`已${next === 'enabled' ? '启用' : '暂停'}`);
      onReload();
      onLoopChanged?.();
    } catch (e) {
      message.error(`状态切换失败: ${e instanceof Error ? e.message : '未知错误'}`);
    }
  }, [workspaceId, onReload, onLoopChanged]);

  return { handleTrigger, handleDuplicate, handleDelete, handleToggleStatus };
}

interface UseLoopConfigArgs {
  workspaceId: number | null;
}

/**
 * 工作空间环路配置页入口：拉取 ProjectDirectory + 切换显示状态。
 * 拆成 hook 避免 LoopListPage 主函数膨胀，同时集中管理 config 相关 state。
 */
export function useLoopConfig({ workspaceId }: UseLoopConfigArgs) {
  const [loopConfigOpen, setLoopConfigOpen] = useState(false);
  const [currentWorkspace, setCurrentWorkspace] = useState<ProjectDirectory | null>(null);

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

  const handleCloseLoopConfig = useCallback(() => {
    setLoopConfigOpen(false);
    setCurrentWorkspace(null);
  }, []);

  return {
    loopConfigOpen,
    currentWorkspace,
    handleOpenLoopConfig,
    handleCloseLoopConfig,
  };
}