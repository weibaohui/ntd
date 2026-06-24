/**
 * TodoDrawer 表单状态管理
 *
 * 使用 useReducer 替代多个 useState，将相关状态集中管理。
 * 表单状态（title、prompt、executor 等）在编辑模式和创建模式之间切换时
 * 需要批量重置，useReducer 可以通过单个 RESET_FORM action 原子性地完成。
 *
 * todo hook 已整块移除（plan `purring-forging-petal`），表单状态里
 * 也不再持有 `hooks: TodoHookItem[]`。
 */

import type { Todo } from '@/types';
import { DEFAULT_EXECUTOR } from '@/types/execution';

/** 表单数据状态 */
export interface TodoFormState {
  /** 任务标题 */
  title: string;
  /** 任务提示词 */
  prompt: string;
  /** 选中的标签 ID 列表 */
  selectedTags: number[];
  /** 执行器名称 */
  executor: string;
  /** 工作目录路径 */
  workspace: string;
  /** 是否启用调度器 */
  schedulerEnabled: boolean;
  /** 调度器 cron 表达式 */
  schedulerConfig: string;
  /** 验收标准 */
  acceptanceCriteria: string;
  /** 是否启用自动评审 */
  autoReviewEnabled: boolean;
}

/** 表单 action 类型 — 泛型联合确保 field 与 value/updater 类型一致 */
export type TodoFormAction =
  | { [K in keyof TodoFormState]: { type: 'SET_FIELD'; field: K; value: TodoFormState[K] } }[keyof TodoFormState]
  | { [K in keyof TodoFormState]: { type: 'SET_FIELD_UPDATER'; field: K; updater: (prev: TodoFormState[K]) => TodoFormState[K] } }[keyof TodoFormState]
  | { type: 'SET_MULTIPLE'; fields: Partial<TodoFormState> }
  | { type: 'RESET_FORM'; todo?: Todo | null }
  | { type: 'RESET_CREATE_MODE' };

/** 初始状态（创建模式） */
export const initialFormState: TodoFormState = {
  title: '',
  prompt: '',
  selectedTags: [],
  executor: DEFAULT_EXECUTOR,
  workspace: '',
  schedulerEnabled: false,
  schedulerConfig: '',
  acceptanceCriteria: '',
  autoReviewEnabled: true,
};

/** 表单 reducer */
export function todoFormReducer(state: TodoFormState, action: TodoFormAction): TodoFormState {
  switch (action.type) {
    case 'SET_FIELD':
      return { ...state, [action.field]: action.value };

    // 功能性更新：接收 prev => newValue 函数，避免 closure 捕获导致的并发回归
    // TS 无法对泛型 discriminated union 做窄化，updater 参数用 any 做内部桥接
    case 'SET_FIELD_UPDATER':
      return { ...state, [action.field]: (action.updater as (prev: any) => any)(state[action.field]) };

    case 'SET_MULTIPLE':
      return { ...state, ...action.fields };

    case 'RESET_FORM':
      if (action.todo) {
        return {
          title: action.todo.title || '',
          prompt: action.todo.prompt || '',
          selectedTags: action.todo.tag_ids || [],
          executor: action.todo.executor || DEFAULT_EXECUTOR,
          workspace: action.todo.workspace || '',
          schedulerEnabled: action.todo.scheduler_enabled || false,
          schedulerConfig: action.todo.scheduler_config || '',
          acceptanceCriteria: action.todo.acceptance_criteria ?? '',
          autoReviewEnabled: action.todo.auto_review_enabled ?? true,
        };
      }
      return initialFormState;

    case 'RESET_CREATE_MODE':
      return initialFormState;

    default:
      return state;
  }
}