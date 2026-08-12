// LoopListPageParts — LoopListPage 的拆分子模块（响应 028 PR review 的函数体 ≤30 行规范）。
//
// 拆分原则：把 header JSX、行操作回调、配置页入口三块独立成组件/hook，
// 让 LoopListPage 主函数仅负责组合，函数体保持简短。
//
// 1. LoopListHeader：顶部 header（搜索 + 配置 + 刷新）
// 2. useLoopRowActions：单行删除/启停状态切换（044：触发/复制已随手工环路能力下线）
// 3. useLoopConfig：工作空间环路配置页入口（拉取 ProjectDirectory + 切换显示）

import { useCallback, useState, type ReactNode } from 'react';
import { Button, Input, Modal, Segmented, message } from 'antd';
import { AppstoreOutlined, ReloadOutlined, SearchOutlined, SettingOutlined, UnorderedListOutlined } from '@ant-design/icons';
import { TimeRangeSegmented } from '@/components/common/TimeRangeSegmented';
import * as dbLoops from '@/utils/database/loops';
import { getProjectDirectories, type ProjectDirectory } from '@/utils/database/todos';
import type { LoopListItem } from '@/types/loop';

interface LoopListHeaderProps {
  /** 视图模式：list loop 定义 table / kanban loop 执行历史看板。 */
  viewMode: 'list' | 'kanban';
  onViewChange: (m: 'list' | 'kanban') => void;
  searchKeyword: string;
  /** kanban 态时间窗：下推给 LoopKanban 过滤执行历史。 */
  hours: number;
  onHoursChange: (h: number) => void;
  loading: boolean;
  workspaceId: number | null;
  onSearchChange: (kw: string) => void;
  onReload: () => void;
  onOpenConfig: () => void;
}

/**
 * 环路列表顶部 header：搜索框 + 配置 + 刷新。
 * 044：环路仅由工艺 install/upgrade 产生，不再有「新建」按钮。
 * 拆出独立组件避免 LoopListPage 主函数膨胀。
 */
export function LoopListHeader({
  viewMode,
  onViewChange,
  searchKeyword,
  hours,
  onHoursChange,
  loading,
  workspaceId,
  onSearchChange,
  onReload,
  onOpenConfig,
}: LoopListHeaderProps): ReactNode {
  // 视图切换：list（定义 table）/ kanban（执行历史看板）。两者数据维度不同，切换会换数据对象。
  const segmented = (
    <Segmented
      size="small"
      value={viewMode}
      onChange={(v) => onViewChange(v as 'list' | 'kanban')}
      options={[
        { value: 'list', icon: <UnorderedListOutlined />, title: '列表' },
        { value: 'kanban', icon: <AppstoreOutlined />, title: '看板' },
      ]}
      data-testid="loop-list-view-toggle"
    />
  );
  return (
    <>
      {segmented}
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
      {/* kanban 态时间窗：LoopKanban 受控，下推 hours 过滤执行历史。 */}
      {viewMode === 'kanban' && (
        <TimeRangeSegmented value={hours} onChange={onHoursChange} />
      )}
      {/* list 态专属：环路配置 + 刷新（kanban 态 LoopKanban 自拉执行历史）。 */}
      {viewMode === 'list' && (
        <Button
          size="small"
          icon={<SettingOutlined />}
          onClick={onOpenConfig}
          disabled={workspaceId == null}
        >
          配置
        </Button>
      )}
      {viewMode === 'list' && (
        <Button
          size="small"
          icon={<ReloadOutlined />}
          onClick={onReload}
          loading={loading}
          aria-label="刷新"
        >
          刷新
        </Button>
      )}
    </>
  );
}

interface UseLoopRowActionsArgs {
  workspaceId: number | null;
  onReload: () => void;
  onLoopChanged?: () => void;
}

/**
 * 环路行操作：删除 / 启停状态切换。
 * 044：环路是工艺的运行时承载，手动触发与复制入口已下线——
 * 唯一执行入口是「创建任务选工艺环路」，复制由工艺重新 install 承担。
 * 拆成 hook 让 LoopListPage 主函数保持简短，便于测试与复用。
 */
export function useLoopRowActions({ workspaceId, onReload, onLoopChanged }: UseLoopRowActionsArgs) {
  // NTD-014-C：删除前必须二次确认——环路被任务引用时删除会连带破坏任务执行，
  // 用 Modal.confirm 兜底误触；确认后仍失败（如被引用）由 catch 弹后端错误。
  const handleDelete = useCallback((loop: LoopListItem) => {
    if (workspaceId == null) return;
    Modal.confirm({
      title: `确定删除环路「${loop.name}」？`,
      content: '删除后引用该环路的任务将无法再执行，此操作不可恢复。',
      okText: '删除',
      okType: 'danger',
      cancelText: '取消',
      onOk: async () => {
        try {
          await dbLoops.deleteLoop(workspaceId, loop.id);
          message.success('已删除');
          onReload();
          onLoopChanged?.();
        } catch (e) {
          message.error('删除失败，环路可能正在被引用');
          // 失败时 re-throw：antd 据此保持确认框打开（loading 复位），用户可原地重试；
          // 若吞错 resolve，对话框会直接关闭，重试需重新打开行菜单（review finding 1）。
          throw e;
        }
      },
    });
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

  return { handleDelete, handleToggleStatus };
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