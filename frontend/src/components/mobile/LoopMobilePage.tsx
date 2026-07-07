import { useEffect, useState, useCallback } from 'react';
import { RetweetOutlined, PlusOutlined } from '@ant-design/icons';
import { Button, message } from 'antd';
import { PageCard } from '../common/PageCard';
import { TodoList } from '../todo-list';
import { LoopDetailPanel } from '../LoopStudioDetailPanel';
import { EmptyDetailPlaceholder } from '../EmptyDetailPlaceholder';
import { SIDEBAR_WIDTH } from '@/constants';
import * as dbLoops from '@/utils/database/loops';
import type { LoopDetail } from '@/types/loop';

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
}: LoopMobilePageProps) {
  const [loopDetail, setLoopDetail] = useState<LoopDetail | null>(null);

  const loadLoopDetail = useCallback(() => {
    if (selectedLoopId === null) {
      setLoopDetail(null);
      return;
    }
    dbLoops.getLoop(selectedLoopId)
      .then(d => setLoopDetail(d))
      .catch(() => setLoopDetail(null));
  }, [selectedLoopId]);

  useEffect(() => {
    loadLoopDetail();
  }, [loadLoopDetail, loopUpdateCount]);

  const handleTrigger = useCallback(async () => {
    if (selectedLoopId === null) return;
    try {
      const res = await dbLoops.triggerLoop(selectedLoopId);
      message.success(`已触发 (execution #${res.execution_id})`);
    } catch (err) {
      message.error(`触发失败: ${err instanceof Error ? err.message : '未知错误'}`);
    }
  }, [selectedLoopId]);

  const handleDuplicate = useCallback(async () => {
    if (selectedLoopId === null) return;
    try {
      await dbLoops.duplicateLoop(selectedLoopId);
      message.success('已复制');
      onLoopChanged();
    } catch (err) {
      message.error(`复制失败: ${err instanceof Error ? err.message : '未知错误'}`);
    }
  }, [selectedLoopId, onLoopChanged]);

  const handleDelete = useCallback(async () => {
    if (selectedLoopId === null) return;
    try {
      await dbLoops.deleteLoop(selectedLoopId);
      message.success('已删除');
      onLoopChanged();
    } catch {
      message.error('删除失败，环路可能正在被引用');
    }
  }, [selectedLoopId, onLoopChanged]);

  const handleToggleStatus = useCallback(async () => {
    if (selectedLoopId === null || !loopDetail) return;
    try {
      const next = loopDetail.status === 'enabled' ? 'paused' : 'enabled';
      await dbLoops.updateLoopStatus(selectedLoopId, { status: next } as any);
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
        <Button
          type="primary"
          size="small"
          icon={<PlusOutlined />}
          onClick={onCreateLoop}
        >
          新建
        </Button>
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

  const detailPage = selectedLoopId !== null ? (
    <PageCard
      icon={<RetweetOutlined />}
      title="环路"
      style={{ height: '100%', flex: 1, minWidth: 0 }}
      contentStyle={{ padding: 0, display: 'flex', flexDirection: 'column', height: 'calc(100% - 43px)', overflow: 'hidden' }}
    >
      <LoopDetailPanel
        loopId={selectedLoopId}
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
    </>
  );
}
