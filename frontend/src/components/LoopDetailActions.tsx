import { Button, Tooltip, Popconfirm } from 'antd';
import { DeleteOutlined } from '@ant-design/icons';

export interface LoopDetailActionsProps {
  onDelete: () => void;
}

/**
 * 环路详情操作按钮组。
 *
 * 044（环路瘦身）：环路降级为「工艺的运行时承载」，触发/复制/导出/编辑按钮
 * 整体下线——唯一执行入口是「创建任务选工艺环路」，定义编辑由工艺 YAML 承担。
 * 仅保留删除（实例不再需要时清理运行时数据）；启停走详情面板的 Switch。
 *
 * 抽成独立组件的目的：删除按钮在两个场景共用--
 *  1. LoopStudioDetailPanel：列表内嵌等「标题行可见」场景，按钮跟随标题行渲染；
 *  2. LoopDetailPage：独立路由场景，hideTitleRow=true 隐藏内层标题行后，
 *     按钮上提到外层 PageCard 的 extra 区（右上角），「返回列表」按钮由 PageCard
 *     按 062 约定统一渲染在其最右端。
 * 抽组件避免两处复制按钮 JSX。
 */
export function LoopDetailActions({ onDelete }: LoopDetailActionsProps) {
  return (
    <Popconfirm
      title="删除 loop"
      description="将级联删除环节与执行记录，无法恢复"
      okType="danger"
      onConfirm={onDelete}
    >
      <Tooltip title="删除">
        <Button type="text" size="small" icon={<DeleteOutlined />} />
      </Tooltip>
    </Popconfirm>
  );
}
