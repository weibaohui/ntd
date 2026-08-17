// 任务页共享常量：状态色板、复杂度色板、视图模式、共享类型。
// 集中管理避免列表/看板/卡片三态视图各写一套产生口径漂移。

import type { MouseEvent, ReactNode } from 'react';
import { Tag } from 'antd';
import { ExclamationCircleOutlined } from '@ant-design/icons';

/**
 * 任务状态色板。
 * 与后端执行状态口径保持一致：
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
const COMPLEXITY_COLOR: Record<string, string> = {
  light: 'green',
  standard: 'blue',
  complex: 'purple',
};

/** 复杂度中文标签。 */
const COMPLEXITY_LABEL: Record<string, string> = {
  light: '轻量',
  standard: '标准',
  complex: '复杂',
};

/**
 * 看板泳道列定义。
 * 顺序即泳道从左到右的排列顺序，与 COLUMNS（原 kanban/constants.ts）保持一致语义。
 */
export interface TaskLane {
  status: string;
  label: string;
  color: string;
}

/**
 * 「待审批」虚拟泳道键（063）：不对应 task.status 的任何真实枚举值，
 * 仅用于看板分组与筛选下拉的 value，避免与后端状态机混淆。
 */
export const PENDING_APPROVAL_LANE = 'pending_approval';

export const TASK_LANES: TaskLane[] = [
  // 待审批放第一列：需要人处理的事项优先级最高，进页即见；橙色与红色标记形成同族警示色。
  { status: PENDING_APPROVAL_LANE, label: '待审批', color: '#fa8c16' },
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
  /** 该任务所有执行中未处理的待审批环节总数（063：后端派生下发，>0 即需用户去审批）。 */
  pending_approval_count?: number;
  created_at?: string;
  // 需求 092：委派执行相关。loop 任务 execution_mode 缺省/为 'loop'，delegate 任务带处理人/接力信息。
  execution_mode?: string;
  assignee_kind?: string;
  assignee_name?: string;
  auto_continue?: boolean;
  continue_rounds?: number;
}

/** 待审批判定：>0 即成立。抽成函数供三态视图与筛选共用，避免各处各写 >0 漂移口径。 */
export function isPendingApproval(task: Pick<TaskItem, 'pending_approval_count'>): boolean {
  return (task.pending_approval_count ?? 0) > 0;
}

/**
 * 状态筛选项（列表/卡片视图共享）。
 * 集中定义的原因：两视图曾各持一份同构数组（063 加「待审批」时需同步改两处），
 * 任一处漏改就会筛选项不一致且无编译报错——与 原 kanban/constants.ts 头部记录的
 * TIME_OPTIONS 三处漂移历史同型，故收口为唯一事实源。
 * pending_approval 为 063 虚拟选项：按待审批数过滤而非匹配 task.status。
 */
export const TASK_STATUS_FILTER_OPTIONS = [
  { value: 'all', label: '全部状态' },
  { value: PENDING_APPROVAL_LANE, label: '待审批' },
  { value: 'pending', label: '待执行' },
  { value: 'running', label: '进行中' },
  { value: 'success', label: '已完成' },
  { value: 'failed', label: '失败' },
];

/**
 * 状态筛选谓词（与 TASK_STATUS_FILTER_OPTIONS 配套的共享实现）。
 * 'all' 恒真；pending_approval 虚拟项按待审批数判定；其余按 task.status 精确匹配。
 */
export function matchesTaskStatusFilter(task: TaskItem, statusFilter: string): boolean {
  if (statusFilter === 'all') return true;
  if (statusFilter === PENDING_APPROVAL_LANE) return isPendingApproval(task);
  return task.status === statusFilter;
}

/**
 * 看板分组：返回任务应落入的泳道 status。
 * 待审批任务只进「待审批」泳道、不再落入真实 status 泳道——
 * 一卡两列会让看板各列计数之和大于任务总数，审批完成后卡片“跳列”也更直观。
 */
export function laneOfTask(task: TaskItem): string {
  if (isPendingApproval(task)) return PENDING_APPROVAL_LANE;
  return task.status;
}

/**
 * 「N 待审批」共享标记：红色 Tag + 警示图标，与执行历史行内既有标记同款视觉。
 * 传 onApprove 时表现为可点击：stopPropagation 防止冒泡触发卡片/行的普通选中，
 * 点击后由调用方跳转任务详情执行历史 Tab（063 需求 4B：一步到位到审批入口）。
 */
export function PendingApprovalTag({
  count,
  onApprove,
}: {
  count: number;
  onApprove?: () => void;
}): ReactNode {
  if (count <= 0) return null;
  const handleClick = (e: MouseEvent) => {
    // 阻断冒泡：卡片/行本体点击是「进详情概览」，待审批点击是「进详情执行历史」，语义不同。
    e.stopPropagation();
    onApprove?.();
  };
  return (
    <Tag
      color="red"
      style={{ fontWeight: 600, margin: 0, cursor: onApprove ? 'pointer' : undefined }}
      onClick={onApprove ? handleClick : undefined}
      data-testid="pending-approval-tag"
    >
      <ExclamationCircleOutlined /> {count} 待审批
    </Tag>
  );
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

// StatusTag 与 laneColorForStatus 已删除（PR #1073 评审修复）：
// StatusTag 全仓无消费方（TodoCenterCard/TodoListView 各有自己的局部状态标签实现），
// laneColorForStatus 唯一调用方就是 StatusTag，连带删除。看板列头取色走 TASK_LANES 自身。
