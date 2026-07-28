import { Button, Popconfirm, Tooltip } from 'antd';
import { EditOutlined, DeleteOutlined, RocketOutlined } from '@ant-design/icons';
import { ActionButton } from '@/components/ActionButton';
import type { Todo } from '@/types';

// 优化标题的 ActionButton prompt 模板。
// 独立常量，避免「自动优化标题」能力在多处使用时重复维护、口径漂移。
const TITLE_OPTIMIZE_PROMPT = `你是一个标题优化专家。请根据以下信息生成更优的标题。

当前标题：{{title}}
当前 Prompt：{{prompt}}

要求：
1. 保持原意
2. 更简洁有力
3. 适合 AI 驱动的任务引擎的场景

输出格式：用 RESULT 标记包裹最终标题，不要加任何其他内容。

RESULT
优化后的标题文本
RESULT`;

export interface TodoDetailActionsProps {
  todo: Todo;
  onDelete: () => Promise<void>;
  onEdit: () => void;
  /** 优化标题回调；未注入时不渲染优化标题按钮（与原 DetailHeader 行为一致）。 */
  onTitleUpdate?: (newTitle: string) => Promise<void>;
}

/**
 * 事项详情操作按钮组：优化标题（ActionButton）+ 编辑 + 删除。
 *
 * 抽成独立组件的目的：这套按钮在两个场景共用--
 *  1. DetailHeader：列表内嵌/移动端等「标题行可见」场景，按钮跟随标题行渲染；
 *  2. TodoDetailPage：独立路由场景，hideTitleRow=true 隐藏内层标题行后，
 *     按钮上提到外层 PageCard 的 titleSuffix 与「返回列表」并排。
 * 抽组件避免两处复制按钮 JSX 与 prompt 模板。
 */
export function TodoDetailActions({ todo, onDelete, onEdit, onTitleUpdate }: TodoDetailActionsProps) {
  return (
    <div style={{ display: 'flex', gap: 4, flexShrink: 0 }}>
      {onTitleUpdate && (
        <Tooltip title="自动优化标题">
          <ActionButton
            actionType="title_optimize"
            actionKey="default"
            prompt={TITLE_OPTIMIZE_PROMPT}
            params={{
              title: todo.title,
              prompt: todo.prompt || '',
            }}
            workspaceId={todo.workspace_id || undefined}
            onApply={onTitleUpdate}
            buttonType="text"
            icon={<RocketOutlined />}
            panelTitle="自动优化标题"
            panelDescription="AI 将根据当前标题和 Prompt 生成更优的版本"
          />
        </Tooltip>
      )}
      <Button type="text" icon={<EditOutlined />} onClick={onEdit} className="icon-btn" aria-label="编辑任务" />
      <Popconfirm title="删除任务" description="确定要删除吗？" onConfirm={onDelete}>
        <Button type="text" icon={<DeleteOutlined />} className="icon-btn" aria-label="删除任务" />
      </Popconfirm>
    </div>
  );
}
