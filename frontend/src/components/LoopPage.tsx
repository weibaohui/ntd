import { RetweetOutlined } from '@ant-design/icons';
import { message } from 'antd';
import { ListDetailPage } from './ListDetailPage';
import { TodoList } from './TodoList';
import { LoopDetailPanel } from './LoopStudioDetailPanel';
import { EmptyDetailPlaceholder } from './EmptyDetailPlaceholder';
import { useIsMobile } from '@/hooks/useIsMobile';
import { SIDEBAR_WIDTH } from '@/constants';
import * as dbLoops from '@/utils/database/loops';

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
}

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
  effectiveMobilePanel,
}: LoopPageProps) {
  const isMobile = useIsMobile();

  const listPanel = (
    <TodoList
      onOpenCreateModal={onOpenCreateModal}
      onSelectTodo={onSelectTodo}
      loopUpdateCount={loopUpdateCount}
      onSelectLoop={onSelectLoop}
      onCreateLoop={onCreateLoop}
      forcedListMode={forcedListMode}
      onListModeChange={onListModeChange}
    />
  );

  const detailPanel = selectedLoopId !== null ? (
    <LoopDetailPanel
      loopId={selectedLoopId}
      tags={tags}
      onTrigger={async () => {
        try {
          const res = await dbLoops.triggerLoop(selectedLoopId);
          message.success(`已触发 (execution #${res.execution_id})`);
        } catch (err) {
          message.error(`触发失败: ${err instanceof Error ? err.message : '未知错误'}`);
        }
      }}
      onDuplicate={async () => {
        try {
          await dbLoops.duplicateLoop(selectedLoopId);
          message.success('已复制');
        } catch (err) {
          message.error(`复制失败: ${err instanceof Error ? err.message : '未知错误'}`);
        }
      }}
      onDelete={async () => {
        try {
          await dbLoops.deleteLoop(selectedLoopId);
          message.success('已删除');
          onLoopChanged();
        } catch (err) {
          message.error('删除失败，环路可能正在被引用');
        }
      }}
      onToggleStatus={async () => {
        try {
          const loops = await dbLoops.listLoops();
          const loop = loops.find(l => l.id === selectedLoopId);
          if (!loop) return;
          const next = loop.status === 'enabled' ? 'paused' : 'enabled';
          await dbLoops.updateLoopStatus(selectedLoopId, { status: next } as any);
          message.success(`已${next === 'enabled' ? '启用' : '暂停'}`);
        } catch (err) {
          message.error(`状态切换失败: ${err instanceof Error ? err.message : '未知错误'}`);
        }
      }}
      onChanged={onLoopChanged}
    />
  ) : null;

  if (isMobile) {
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
          {listPanel}
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
          {detailPanel ?? <EmptyDetailPlaceholder />}
        </div>
      </>
    );
  }

  return (
    <ListDetailPage
      icon={<RetweetOutlined />}
      title="环路"
      listPanel={listPanel}
      detailPanel={detailPanel}
    />
  );
}
