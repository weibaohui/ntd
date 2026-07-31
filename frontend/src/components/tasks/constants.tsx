// 任务页共享常量：状态色板、复杂度色板、视图模式、共享类型。
// 集中管理避免列表/看板/卡片三态视图各写一套产生口径漂移。

import type { ReactNode } from 'react';

/**
 * 任务状态色板。
 * 与 ProcessExecutionBoard 的 statusColor 保持一致：
 *   pending  → default（灰，未启动）
 *   running  → blue    （蓝，运行中）
 *   success  → green   （绿，完成）
 *   failed   → red     （红，失败）
 */
export const STATUS_COLOR: Record<string, string> = {
  pending: 'default',
  running: 'blue',
  success: 'green',
  failed: 'red',
};

/** 状态中文标签，未匹配时回退原值。 */
export const STATUS_LABEL: Record<string, string> = {
  pending: '待执行',
  running: '进行中',
  success: '已完成',
  failed: '失败',
};

/**
 * 复杂度色板。
 * 与 ProcessPage 的 complexityColor 对齐：
 *   light    → green  （轻量，低风险）
 *   standard → blue   （标准，正常流程）
 *   complex  → purple （复杂，需多轮迭代）
 */
export const COMPLEXITY_COLOR: Record<string, string> = {
  light: 'green',
  standard: 'blue',
  complex: 'purple',
};

/** 复杂度中文标签。 */
export const COMPLEXITY_LABEL: Record<string, string> = {
  light: '轻量',
  standard: '标准',
  complex: '复杂',
};

/**
 * 看板泳道列定义。
 * 顺序即泳道从左到右的排列顺序，与 COLUMNS（kanban/constants.ts）保持一致语义。
 */
export interface TaskLane {
  status: string;
  label: string;
  color: string;
}

export const TASK_LANES: TaskLane[] = [
  { status: 'pending', label: '待执行', color: '#3b82f6' },
  { status: 'running', label: '进行中', color: '#f59e0b' },
  { status: 'success', label: '已完成', color: '#22c55e' },
  { status: 'failed', label: '失败', color: '#ef4444' },
];

/**
 * 任务列表项类型。
 * 与 bundledApi.listTasks 返回结构一致，未设为全局类型以避免影响其他调用点。
 */
export interface TaskItem {
  id: number;
  title: string;
  description: string;
  status: string;
  template_id?: number;
  template_name?: string;
  template_version?: string;
  complexity?: string;
  loop_id?: number;
  workspace_id?: number;
  latest_execution_status?: string;
  latest_execution_requirement?: string;
  created_at?: string;
}

/** 三态视图模式。 */
export type TasksViewMode = 'list' | 'kanban' | 'card';

/** localStorage 键：记住用户上次选的视图模式。 */
export const TASKS_VIEW_STORAGE_KEY = 'ntd_tasks_view';

/**
 * 工艺环路精简类型。
 * 049 起扩展工艺来源字段（与 LoopListItem 同名对齐），
 * 供新建任务下拉拼装「#环路ID 名称（#工艺ID 工艺名 版本）」；
 * 全部可选以保持向后兼容，无工艺来源时 label 退化为 041 格式。
 */
export interface LoopLite {
  id: number;
  name: string;
  /** 来源工艺模板 ID（新建任务下拉只列非空环路，此字段理论上必有值） */
  process_template_id?: number | null;
  /** 来源工艺显示名（优先展示） */
  process_template_display_name?: string | null;
  /** 来源工艺标识名（display_name 缺失时回退） */
  process_template_name?: string | null;
  /** 实例化时的工艺版本快照 */
  process_template_version?: string | null;
}

/**
 * 拼装新建任务下拉的选项文案：`#<环路ID> 环路名称（#工艺ID 工艺名称 工艺版本）`。
 * 抽成纯函数便于单测（与 041 loop-list 的 loopProcessText 同模式）。
 *
 * 回退口径与 utils/processText 的 formatProcessText 保持一致：
 * - 无工艺来源（process_template_id 为空）→ 退化 041 格式 `#<环路ID> <名称>`；
 * - 工艺名 display_name → name → `#<工艺ID>` 逐级回退，避免空名称段；
 * - 版本缺失用 '—' 占位，保持括号内三段式结构可读。
 */
export function loopOptionLabel(l: LoopLite): string {
  // 基础段：041 格式，同名环路靠 ID 区分。
  const base = `#${l.id} ${l.name}`;
  // 防御分支：调用方已过滤只传工艺环路，但类型上字段可选，无来源时保持原样。
  if (l.process_template_id == null) return base;
  // 名称逐级回退：两个字段分别 trim 判定，display_name 为纯空白串时继续尝试标识名，
  // 避免「display_name='  ' 且 name 有效」被错误兜底成 #id（PR #959 CodeRabbit 评审发现）；
  // 返回 trim 后的值，防止首尾空白污染 label 版式。
  const pname =
    l.process_template_display_name?.trim() ||
    l.process_template_name?.trim() ||
    `#${l.process_template_id}`;
  // 版本缺失或纯空白时用占位符，避免出现「#3 名称 」尾部空段；同样取 trim 后值保持口径一致。
  const version = l.process_template_version?.trim() || '—';
  return `${base}（#${l.process_template_id} ${pname} ${version}）`;
}

/**
 * 取状态色，未匹配回退 default。
 * 抽成函数而非直接查表，便于将来扩展「状态加图标」等需求。
 */
export function statusColor(status: string): string {
  return STATUS_COLOR[status] ?? 'default';
}

/** 取复杂度色，未匹配回退 default。 */
export function complexityColor(complexity?: string): string {
  if (!complexity) return 'default';
  return COMPLEXITY_COLOR[complexity] ?? 'default';
}

/** 取复杂度中文标签，未匹配回退原值。 */
export function complexityLabel(complexity?: string): string {
  if (!complexity) return complexity ?? '';
  return COMPLEXITY_LABEL[complexity] ?? complexity;
}

/** 把 ISO 时间截取为 YYYY-MM-DD，undefined 时回退 '—'。 */
export function formatDateShort(iso?: string): string {
  if (!iso) return '—';
  return iso.slice(0, 10);
}

/** ReactNode 形式的状态标签，便于多处复用。 */
export function StatusTag({ status }: { status: string }): ReactNode {
  return (
    <span
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 4,
        padding: '2px 8px',
        borderRadius: 4,
        fontSize: 12,
        color: STATUS_COLOR[status] === 'default' ? '#6b7280' : '#fff',
        background: laneColorForStatus(status),
      }}
    >
      {STATUS_LABEL[status] ?? status}
    </span>
  );
}

/** 取泳道色（看板列头 + 状态标签共用）。 */
export function laneColorForStatus(status: string): string {
  const lane = TASK_LANES.find((l) => l.status === status);
  return lane?.color ?? '#6b7280';
}
