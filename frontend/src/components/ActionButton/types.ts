import type { ReactNode } from 'react';

export interface ActionButtonProps {
  /** 动作类型（如 "title_optimize"、"prompt_optimize"） */
  actionType: string;
  /** 动作键值（如 "default"、"aggressive"） */
  actionKey: string;
  /** Prompt 模板，支持 {{key}} 占位符 */
  prompt: string;
  /** 模板参数，键值对会替换 prompt 中对应的 {{key}} 占位符 */
  params: Record<string, string>;
  /** 执行完成后「应用」的回调，参数为 AI 生成的结果文本（完整 markdown） */
  onApply: (result: string) => void | Promise<void>;
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
}

export type ActionStatus = 'idle' | 'executing' | 'completed' | 'failed';
