import { useEffect, useState, useCallback } from 'react';
import { RetweetOutlined, PlusOutlined, SettingOutlined } from '@ant-design/icons';
import { Button, message } from 'antd';
import { PageCard } from '../common/PageCard';
import { TodoList } from '../todo-list';
import { LoopDetailPanel } from '../LoopStudioDetailPanel';
import { EmptyDetailPlaceholder } from '../EmptyDetailPlaceholder';
import { WorkspaceLoopConfigPage } from '../settings/workspace/WorkspaceLoopConfigPage';
import { SIDEBAR_WIDTH } from '@/constants';
import * as db from '@/utils/database';
import * as dbLoops from '@/utils/database/loops';
import type { LoopDetail } from '@/types/loop';
import type { ProjectDirectory } from '@/types';

interface LoopMobilePageProps {
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
  workspaceId?: number | null;
}

/**
 * 移动端环路页面组件 —— 与桌面端统一设计
 * 与 PC 端 ListDetailPage 一样，使用 PageCard 包裹详情页内容
 * 列表页和详情页各自有 PageCard 容器，详情页内 LoopDetailPanel 渲染完整头部
 */
export function LoopMobilePage({
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
  effectiveMobilePanel,
  workspaceId,
}: LoopMobilePageProps) {
  const [loopDetail, setLoopDetail] = useState<LoopDetail | null>(null);
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

  const loadLoopDetail = useCallback(() => {
    if (selectedLoopId === null) {
      setLoopDetail(null);
      return;
    }
    dbLoops.getLoop(workspaceId ?? 0, selectedLoopId)
      .then(d => setLoopDetail(d))
      .catch(() => setLoopDetail(null));
  }, [selectedLoopId, workspaceId]);

  useEffect(() => {
    loadLoopDetail();
  }, [loadLoopDetail, loopUpdateCount]);

  const handleTrigger = useCallback(async () => {
    if (selectedLoopId === null) return;
    try {
      const res = await dbLoops.triggerLoop(workspaceId ?? 0, selectedLoopId);
      message.success(`已触发 (execution #${res.execution_id})`);
    } catch (err) {
      message.error(`触发失败: ${err instanceof Error ? err.message : '未知错误'}`);
    }
  }, [selectedLoopId, workspaceId]);

  const handleDuplicate = useCallback(async () => {
    if (selectedLoopId === null) return;
    try {
      await dbLoops.duplicateLoop(workspaceId ?? 0, selectedLoopId);
      message.success('已复制');
      onLoopChanged();
    } catch (err) {
      message.error(`复制失败: ${err instanceof Error ? err.message : '未知错误'}`);
    }
  }, [selectedLoopId, workspaceId, onLoopChanged]);

  const handleDelete = useCallback(async () => {
    if (selectedLoopId === null) return;
    try {
      await dbLoops.deleteLoop(workspaceId ?? 0, selectedLoopId);
      message.success('已删除');
      onLoopChanged();
    } catch {
      message.error('删除失败，环路可能正在被引用');
    }
  }, [selectedLoopId, workspaceId, onLoopChanged]);

  const handleToggleStatus = useCallback(async () => {
    if (selectedLoopId === null || !loopDetail) return;
    try {
      const next = loopDetail.status === 'enabled' ? 'paused' : 'enabled';
      await dbLoops.updateLoopStatus(workspaceId ?? 0, selectedLoopId, { status: next } as any);
      message.success(`已${next === 'enabled' ? '启用' : '暂停'}`);
      loadLoopDetail();
      onLoopChanged();
    } catch (err) {
      message.error(`状态切换失败: ${err instanceof Error ? err.message : '未知错误'}`);
    }
  }, [selectedLoopId, loopDetail, loadLoopDetail, onLoopChanged]);

  const listPage = (
    <PageCard
      icon={<RetweetOutlined />}
      title="环路"
      extra={
        <div style={{ display: 'flex', gap: 8 }}>
          {/* 环路配置按钮：放在新建按钮左边 */}
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
      style={{ height: '100%' }}
      contentStyle={{ padding: 0, height: 'calc(100% - 43px)' }}
    >
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
    </PageCard>
  );

  // 环路详情面板：有环路选中时显示，无则显示空状态占位
  const detailPanel = selectedLoopId !== null ? (
    <PageCard
      icon={<RetweetOutlined />}
      title="环路"
      style={{ height: '100%', flex: 1, minWidth: 0 }}
      contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(100% - 43px)', overflow: 'hidden' }}
    >
      <LoopDetailPanel
        loopId={selectedLoopId}
        workspaceId={workspaceId ?? null}
        tags={tags}
        onTrigger={handleTrigger}
        onDuplicate={handleDuplicate}
        onDelete={handleDelete}
        onToggleStatus={handleToggleStatus}
        onChanged={() => {
          loadLoopDetail();
          onLoopChanged();
        }}
      />
    </PageCard>
  ) : (
    <PageCard
      icon={<RetweetOutlined />}
      title="环路"
      style={{ height: '100%', flex: 1, minWidth: 0 }}
      contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(100% - 43px)', overflow: 'hidden' }}
    >
      <EmptyDetailPlaceholder />
    </PageCard>
  );

  // 配置页已移至全屏 overlay 渲染，detailPage 只做普通详情/空状态切换
  const detailPage = detailPanel;

  return (
    <>
      <div
        className={effectiveMobilePanel === 'list' ? 'animate-fade-in' : ''}
        style={{
          width: SIDEBAR_WIDTH.mobile,
          flexShrink: 0,
          height: '100%',
          display: effectiveMobilePanel === 'list' ? 'block' : 'none',
        }}
      >
        {listPage}
      </div>
      <div
        className={effectiveMobilePanel === 'detail' ? 'animate-slide-in-right' : ''}
        style={{
          flex: 1,
          minWidth: 0,
          height: '100%',
          overflow: 'hidden',
          display: effectiveMobilePanel === 'detail' ? 'block' : 'none',
        }}
      >
        {detailPage}
      </div>
      {/* 环路配置是全局页面，不属于任何环路，作为全屏覆盖层独立渲染 */}
      {showLoopConfig && currentWorkspace && (
        <div
          style={{
            position: 'fixed',
            inset: 0,
            zIndex: 1000,
            background: 'var(--ntd-bg-color)',
            display: 'flex',
            flexDirection: 'column',
          }}
        >
          <WorkspaceLoopConfigPage
            workspace={currentWorkspace}
            onBack={handleCloseLoopConfig}
          />
        </div>
      )}
    </>
  );
}
