import { useState, useEffect, useCallback } from 'react';
import { RetweetOutlined, PlusOutlined, SettingOutlined } from '@ant-design/icons';
import { Button, message } from 'antd';
import { ListDetailPage } from './ListDetailPage';
import { TodoList } from './todo-list';
import { LoopDetailPanel } from './LoopStudioDetailPanel';
import { WorkspaceLoopConfigPage } from './settings/workspace/WorkspaceLoopConfigPage';
import * as db from '@/utils/database';
import * as dbLoops from '@/utils/database/loops';
import type { ProjectDirectory } from '@/types';

interface LoopPageProps {
  selectedLoopId: number | null;
  tags: Array<{ id: number; name: string; color: string }>;
  onOpenCreateModal: () => void;
  onSelectTodo: (todoId: string | number | null) => void;
  loopUpdateCount: number;
  onSelectLoop: (loopId: number) => void;
  onCreateLoop: () => void;
  forcedListMode?: 'item' | 'loop';
  onListModeChange: () => void;
  onLoopChanged: () => void;
  effectiveMobilePanel: 'list' | 'detail';
  // 当前选中的工作空间 id，用于环路配置功能
  workspaceId?: number | null;
}

/**
 * 桌面端环路页面组件
 * 使用 ListDetailPage 实现左侧列表 + 右侧详情的双栏布局
 * 移动端逻辑已独立到 LoopMobilePage 组件
 * 支持工作空间环路配置功能，配置按钮位于右上角新建按钮左侧
 */
export function LoopPage({
  selectedLoopId,
  tags,
  onOpenCreateModal,
  onSelectTodo,
  loopUpdateCount,
  onSelectLoop,
  onCreateLoop,
  forcedListMode,
  onListModeChange,
  onLoopChanged,
  workspaceId,
}: LoopPageProps) {
  // 状态：是否显示环路配置页面（替代默认的环路详情页）
  const [showLoopConfig, setShowLoopConfig] = useState(false);
  // 当前选中的工作空间对象，用于传递给 WorkspaceLoopConfigPage
  const [currentWorkspace, setCurrentWorkspace] = useState<ProjectDirectory | null>(null);

  // 当 workspaceId 变化时，重新加载工作空间信息
  useEffect(() => {
    if (workspaceId == null) {
      setCurrentWorkspace(null);
      return;
    }
    // 拉取工作空间详情，用于环路配置页面显示工作空间名称
    db.getProjectDirectories().then(dirs => {
      const dir = dirs.find(d => d.id === workspaceId);
      if (dir) setCurrentWorkspace(dir);
    }).catch(() => {
      // 加载失败时静默处理，不影响主流程
    });
  }, [workspaceId]);

  // 打开环路配置页面的处理函数
  const handleOpenLoopConfig = useCallback(() => {
    if (workspaceId == null) {
      message.warning('请先选择工作空间');
      return;
    }
    setShowLoopConfig(true);
  }, [workspaceId]);

  // 关闭环路配置页面，回到环路列表+详情视图
  const handleCloseLoopConfig = useCallback(() => {
    setShowLoopConfig(false);
  }, []);

  const listPanel = (
    <TodoList
      onOpenCreateModal={onOpenCreateModal}
      onSelectTodo={onSelectTodo}
      loopUpdateCount={loopUpdateCount}
      onSelectLoop={onSelectLoop}
      onCreateLoop={onCreateLoop}
      forcedListMode={forcedListMode}
      onListModeChange={onListModeChange}
      hideCreateButton={true}
    />
  );

  const detailPanel = selectedLoopId !== null ? (
    <LoopDetailPanel
      loopId={selectedLoopId}
      workspaceId={workspaceId ?? null}
      tags={tags}
      onTrigger={async () => {
        try {
          const res = await dbLoops.triggerLoop(workspaceId ?? 0, selectedLoopId);
          message.success(`已触发 (execution #${res.execution_id})`);
        } catch (err) {
          message.error(`触发失败: ${err instanceof Error ? err.message : '未知错误'}`);
        }
      }}
      onDuplicate={async () => {
        try {
          await dbLoops.duplicateLoop(workspaceId ?? 0, selectedLoopId);
          message.success('已复制');
        } catch (err) {
          message.error(`复制失败: ${err instanceof Error ? err.message : '未知错误'}`);
        }
      }}
      onDelete={async () => {
        try {
          await dbLoops.deleteLoop(workspaceId ?? 0, selectedLoopId);
          message.success('已删除');
          onLoopChanged();
        } catch {
          message.error('删除失败，环路可能正在被引用');
        }
      }}
      onToggleStatus={async () => {
        try {
          const loops = await dbLoops.listLoops(workspaceId ?? null);
          const loop = loops.find(l => l.id === selectedLoopId);
          if (!loop) return;
          const next = loop.status === 'enabled' ? 'paused' : 'enabled';
          await dbLoops.updateLoopStatus(workspaceId ?? 0, selectedLoopId, { status: next } as any);
          message.success(`已${next === 'enabled' ? '启用' : '暂停'}`);
        } catch (err) {
          message.error(`状态切换失败: ${err instanceof Error ? err.message : '未知错误'}`);
        }
      }}
      onChanged={onLoopChanged}
    />
  ) : null;

  // 若显示环路配置页面，则替换右侧详情面板为 WorkspaceLoopConfigPage
  // 否则显示默认的环路详情面板（selectedLoopId 选中的环路）
  const effectiveDetailPanel = showLoopConfig && currentWorkspace ? (
    <WorkspaceLoopConfigPage
      workspace={currentWorkspace}
      onBack={handleCloseLoopConfig}
    />
  ) : detailPanel;

  return (
    <ListDetailPage
      icon={<RetweetOutlined />}
      title="环路"
      storageKey="loop_page_sidebar_collapsed"
      extra={
        <div style={{ display: 'flex', gap: 8 }}>
          {/* 环路配置按钮：放在新建按钮左边，点击后打开配置页面 */}
          <Button
            size="small"
            icon={<SettingOutlined />}
            onClick={handleOpenLoopConfig}
            disabled={workspaceId == null}
          >
            配置
          </Button>
          {/* 新建环路按钮 */}
          <Button
            type="primary"
            size="small"
            icon={<PlusOutlined />}
            onClick={onCreateLoop}
          >
            新建
          </Button>
        </div>
      }
      listPanel={listPanel}
      detailPanel={effectiveDetailPanel}
    />
  );
}
