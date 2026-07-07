import { RetweetOutlined, PlusOutlined } from '@ant-design/icons';
import { Button, message } from 'antd';
import { ListDetailPage } from './ListDetailPage';
import { TodoList } from './todo-list';
import { LoopDetailPanel } from './LoopStudioDetailPanel';
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

/**
 * 桌面端环路页面组件
 * 使用 ListDetailPage 实现左侧列表 + 右侧详情的双栏布局
 * 移动端逻辑已独立到 LoopMobilePage 组件
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
}: LoopPageProps) {
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
        } catch {
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

  return (
    <ListDetailPage
      icon={<RetweetOutlined />}
      title="环路"
      storageKey="loop_page_sidebar_collapsed"
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
      listPanel={listPanel}
      detailPanel={detailPanel}
    />
  );
}
