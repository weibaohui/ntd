import { useEffect, useState, useCallback } from 'react';
import { RetweetOutlined, PlusOutlined, ThunderboltOutlined, CopyOutlined, EditOutlined, DeleteOutlined } from '@ant-design/icons';
import { Button, Space, Tooltip, Popconfirm, message } from 'antd';
import { PageCard } from '../common/PageCard';
import { TodoList } from '../TodoList';
import { LoopDetailPanel } from '../LoopStudioDetailPanel';
import { LoopFormModal } from '../LoopFormModal';
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
 * 移动端环路页面组件
 * 列表页和详情页为两个独立的 PageCard 页面，各自有完整的标题栏
 * 列表页：PageCard 标题为"环路"
 * 详情页：PageCard 标题为具体环路标题，操作按钮在标题栏右侧
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
  const [editModalOpen, setEditModalOpen] = useState(false);

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
      title={loopDetail?.name ?? '加载中...'}
      extra={
        <Space size={4}>
          <Tooltip title="手动触发">
            <Button
              type="primary"
              size="small"
              icon={<ThunderboltOutlined />}
              onClick={handleTrigger}
              disabled={loopDetail?.status !== 'enabled'}
            />
          </Tooltip>
          <Tooltip title="复制">
            <Button type="text" size="small" icon={<CopyOutlined />} onClick={handleDuplicate} />
          </Tooltip>
          <Tooltip title="编辑">
            <Button type="text" size="small" icon={<EditOutlined />} onClick={() => setEditModalOpen(true)} />
          </Tooltip>
          <Popconfirm
            title="删除 loop"
            description="将级联删除 triggers/steps,无法恢复"
            okType="danger"
            onConfirm={handleDelete}
          >
            <Tooltip title="删除">
              <Button type="text" size="small" icon={<DeleteOutlined />} />
            </Tooltip>
          </Popconfirm>
        </Space>
      }
      style={{ height: '100%' }}
      contentStyle={{ padding: 0, height: 'calc(100% - 43px)' }}
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
        hideTitleRow={true}
      />
      <LoopFormModal
        open={editModalOpen}
        mode="edit"
        loopId={selectedLoopId ?? undefined}
        initialData={loopDetail ? {
          name: loopDetail.name,
          description: loopDetail.description,
          workspace: loopDetail.workspace,
          webhook_enabled: loopDetail.webhook_enabled,
          icon: loopDetail.icon,
          review_template_id: loopDetail.review_template_id ?? null,
          tag_ids: loopDetail.tag_ids ?? [],
          limits_config: loopDetail.limits_config,
          abnormal_handler_todo_id: loopDetail.abnormal_handler_todo_id ?? null,
          abnormal_handler_trigger_on: loopDetail.abnormal_handler_trigger_on ?? '["capped_step","capped_token","failed"]',
        } : undefined}
        tags={tags}
        onSaved={() => {
          setEditModalOpen(false);
          loadLoopDetail();
          onLoopChanged();
        }}
        onClose={() => setEditModalOpen(false)}
      />
    </PageCard>
  ) : (
    <PageCard
      style={{ height: '100%' }}
      contentStyle={{ padding: 0, height: 'calc(100% - 43px)' }}
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
