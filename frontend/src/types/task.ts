// 任务讨论区相关类型（需求 060：论坛跟帖 + @专家/@执行器 触发执行后回帖）。
//
// 注意：任务列表/详情既有的 TaskItem / TaskDetailData / StepInfo 等类型仍内联在
// components/tasks 下（历史原因，迁移成本高），本文件只新增讨论帖相关类型，
// 避免大规模改动现有 import。后续可单独立需求收口。

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
}
