/**
 * AI 创建专家 — 复用 ActionButton 交互。
 *
 * 直接使用 ActionButton 承载完整交互流程：
 * 1. 点击按钮 → 打开 Drawer，展示工作空间选择、prompt 模板（含 {{description}} 占位符）
 * 2. 用户在模板中填写描述 → 点击执行
 * 3. executing 态：ActionButton 自动展示 ChatView 日志流
 * 4. completed 态：由 completedView 插槽渲染 ExpertCreateCompleted（结果编辑 + 创建）
 *
 * 与 ProposalButton 的模式完全一致，只是完成态不同。
 */

import { ThunderboltOutlined } from '@ant-design/icons';
import { ActionButton } from '@/components/ActionButton';
import { ExpertCreateCompleted } from './ExpertCreateCompleted';
import {
  EXPERT_CREATE_PROMPT,
  EXPERT_CREATE_ACTION_TYPE,
  EXPERT_CREATE_ACTION_KEY,
  EXPERT_CREATE_EXECUTOR,
} from './expert-create-prompt';
import { useTodos } from '@/hooks/useTodoContext';

interface ExpertCreateModalProps {
  /** 创建成功后回调（用于刷新列表） */
  onCreated: () => void;
}

/**
 * AI 创建专家触发按钮。
 *
 * 使用 useTodos 获取页面左上角选中的工作空间，传给 ActionButton 作为默认值。
 */
export function ExpertCreateModal({ onCreated }: ExpertCreateModalProps) {
  // 获取页面左上角选中的工作空间
  const { state } = useTodos();
  const workspaceId = state.selectedWorkspace;

  return (
    <ActionButton
      actionType={EXPERT_CREATE_ACTION_TYPE}
      actionKey={EXPERT_CREATE_ACTION_KEY}
      prompt={EXPERT_CREATE_PROMPT}
      params={{ description: '' }}
      workspaceId={workspaceId ?? undefined}
      executor={EXPERT_CREATE_EXECUTOR}
      buttonType="primary"
      buttonSize="small"
      icon={<ThunderboltOutlined />}
      panelTitle="AI 创建专家"
      panelDescription="在下方填写模板参数，AI 会自动生成完整的 plugin.json 和 agent.md"
      completedView={({ result, close }) => (
        <ExpertCreateCompleted
          result={result}
          close={close}
          onCreated={onCreated}
        />
      )}
    >
      AI 创建专家
    </ActionButton>
  );
}
