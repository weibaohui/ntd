import type { ReactNode } from 'react';

/**
 * 完成态渲染插槽的上下文：
 * - result：AI 生成的完整结果文本（与 useActionExecution.result 同源，完成态必有值）
 * - close：关闭当前 Drawer（ActionButton 的 handleClose）
 * - retry：用相同 prompt/executor 重跑一次（ActionButton 的 handleRetry）
 *
 * 让自定义完成视图能自洽地操作 Drawer 关闭与重试，不必把 ActionButton 内部状态抬到父组件。
 */
export interface CompletedViewCtx {
  result: string;
  close: () => void;
  retry: () => void;
}

export interface ActionButtonProps {
  /** 动作类型（如 "title_optimize"、"prompt_optimize"） */
  actionType: string;
  /** 动作键值（如 "default"、"aggressive"） */
  actionKey: string;
  /** Prompt 模板，支持 {{key}} 占位符 */
  prompt: string;
  /** 模板参数，键值对会替换 prompt 中对应的 {{key}} 占位符 */
  params: Record<string, string>;
  /**
   * 执行完成后「应用」的回调，参数为 AI 生成的结果文本（完整 markdown）。
   * 仅在未提供 completedView（走默认完成视图）时使用；提供 completedView 时可省略。
   */
  onApply?: (result: string) => void | Promise<void>;
  /** 工作空间 ID（可选，不传则使用默认工作空间） */
  workspaceId?: number;
  /** 按钮显示内容 */
  children?: ReactNode;
  /** 按钮类型（Ant Design） */
  buttonType?: 'primary' | 'default' | 'link' | 'text';
  /** 按钮图标 */
  icon?: ReactNode;
  /** 是否禁用 */
  disabled?: boolean;
  /** 面板标题（默认：智能执行） */
  panelTitle?: string;
  /** 面板描述（默认：将使用 AI 处理以下内容） */
  panelDescription?: string;
  /** 默认执行器类型 */
  executor?: string;
  /** 触发按钮尺寸，移动端工具栏传 'small' 与其它图标按钮对齐 */
  buttonSize?: 'small' | 'middle' | 'large';
  /** 是否显示触发按钮文字；移动端空间紧张可传 false 只留图标 */
  showLabel?: boolean;
  /**
   * 自定义完成态视图。提供后，completed 状态不再渲染默认的「结果原文 + 应用/拒绝」，
   * 改由该插槽全权负责（如 ProposalButton 把结果解析成建议列表 + 批量创建）。
   * 完成态的 Drawer footer 同时置空，操作按钮由插槽自行在视图内承载（建议用 sticky 底栏）。
   */
  completedView?: (ctx: CompletedViewCtx) => ReactNode;
}

export type ActionStatus = 'idle' | 'executing' | 'completed' | 'failed';
