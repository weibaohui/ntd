// 任务领域共享类型。
//
// 本文件收口两类跨文件复用的任务类型：
// - 讨论区（需求 060）：TaskMention / TaskPost。
// - 任务详情（需求 093 重构）：TaskDetailData / ExecInfo，原内联于
//   components/tasks/TaskDetailPanel.tsx，抽 useTaskDetail hook + TaskDetailHeader
//   后被多文件复用，故上提到此。StepInfo 仍定义在 TaskDetailTabs.tsx（与 Tab 组件
//   强绑定），此处跨文件按需 import，不强行搬迁以免连带改动。

import type { StepInfo } from '@/components/tasks/TaskDetailTabs';

/** 一条 @ 提及（执行器或专家）。后端 mentions 列存的是它的 JSON 数组字符串。 */
export interface TaskMention {
  type: 'expert' | 'executor';
  /** 规范名（执行器如 'codex'；专家如 '前端架构师'）。 */
  name: string;
  /** 展示名。 */
  display: string;
}

/** 任务讨论帖（对应后端 task_posts 表的一行）。 */
export interface TaskPost {
  id: number;
  task_id: number;
  /** 楼中楼被回复楼层；null=主楼层。深度 ≤1（仅允许回复主楼层）。 */
  parent_post_id: number | null;
  kind: 'human' | 'agent';
  author_name: string;
  executor: string | null;
  expert_name: string | null;
  /** Markdown 正文（人帖=用户输入；智能体帖=执行结论 / 占位文案）。 */
  content: string;
  /** 结构化提及的 JSON 字符串：TaskMention[]。展示徽标时需 JSON.parse。 */
  mentions: string;
  /** 人帖恒 'sent'；智能体帖 'running'|'success'|'failed'。 */
  status: 'sent' | 'running' | 'success' | 'failed';
  /** 智能体帖来源 execution_records.id（可跳转执行明细）。 */
  source_execution_id: number | null;
  source_todo_id: number | null;
  created_at: string | null;
  updated_at: string | null;
  /**
   * 楼中楼回复；仅主楼层在列表接口返回时由后端组装携带（id ASC）。
   * 单帖 / create 接口返回的帖子不带此字段。
   */
  replies?: TaskPost[];
}

// ===== 任务详情（TaskDetailPanel / useTaskDetail / TaskDetailHeader 共享）=====

/**
 * 任务详情接口返回的单次执行摘要。
 * 字段集对齐后端 get_task_detail 的 executions 段；可选字段按后端可能缺省声明。
 */
interface ExecInfo {
  id: number;
  status: string;
  started_at?: string;
  finished_at?: string;
  total_steps: number;
  completed_steps: number;
  failed_steps: number;
  requirement?: string;
  pending_approval_count?: number;
}

/** 任务详情主体（get_task_detail 返回）。task 子对象展开为内联形态，与后端 JSON 一致。 */
export interface TaskDetailData {
  task: {
    id: number;
    title: string;
    status: string;
    description?: string;
    workspace_id?: number;
    loop_id?: number;
    execution_mode?: string;
    assignee_kind?: string;
    assignee_name?: string;
    auto_continue?: boolean;
    continue_rounds?: number;
    // 接力轮数上限三级配置：raw 覆盖 / 后端解析的有效值 / 清除覆盖后的回退值。
    delegate_max_rounds?: number | null;
    delegate_max_rounds_effective?: number;
    delegate_max_rounds_fallback?: number;
  };
  template?: { display_name?: string; version?: string; complexity?: string };
  steps: StepInfo[];
  executions: ExecInfo[];
  loop?: { id: number; workspace_id?: number };
}
