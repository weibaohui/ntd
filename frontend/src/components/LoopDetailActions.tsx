import { Button, Space, Tooltip, Popconfirm } from 'antd';
import {
  ThunderboltOutlined,
  CopyOutlined,
  ExportOutlined,
  EditOutlined,
  DeleteOutlined,
} from '@ant-design/icons';
import type { LoopDetail } from '@/types/loop';

export interface LoopDetailActionsProps {
  detail: LoopDetail;
  onTrigger: () => void;
  onDuplicate: () => void;
  onExport: () => void;
  onEdit: () => void;
  onDelete: () => void;
}

/**
 * 环路详情操作按钮组：触发/复制/导出/编辑/删除。
 *
 * 抽成独立组件的目的：这套按钮在两个场景共用--
 *  1. LoopStudioDetailPanel：列表内嵌等「标题行可见」场景，按钮跟随标题行渲染；
 *  2. LoopDetailPage：独立路由场景，hideTitleRow=true 隐藏内层标题行后，
 *     按钮上提到外层 PageCard 的 titleSuffix 与「返回列表」并排。
 * 抽组件避免两处复制按钮 JSX。
 */
export function LoopDetailActions({
  detail,
  onTrigger,
  onDuplicate,
  onExport,
  onEdit,
  onDelete,
}: LoopDetailActionsProps) {
  return (
    <Space size={4}>
      <Tooltip title="手动触发">
        <Button
          size="small"
          type="primary"
          icon={<ThunderboltOutlined />}
          onClick={onTrigger}
          disabled={detail.status !== 'enabled'}
        />
      </Tooltip>
      <Tooltip title="复制">
        <Button type="text" size="small" icon={<CopyOutlined />} onClick={onDuplicate} />
      </Tooltip>
      <Tooltip title="导出">
        <Button type="text" size="small" icon={<ExportOutlined />} onClick={onExport} />
      </Tooltip>
      <Tooltip title="编辑">
        <Button type="text" size="small" icon={<EditOutlined />} onClick={onEdit} />
      </Tooltip>
      <Popconfirm
        title="删除 loop"
        description="将级联删除 triggers/steps,无法恢复"
        okType="danger"
        onConfirm={onDelete}
      >
        <Tooltip title="删除">
          <Button type="text" size="small" icon={<DeleteOutlined />} />
        </Tooltip>
      </Popconfirm>
    </Space>
  );
}
