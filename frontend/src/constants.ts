// =============================================================================
// Shared application constants — single source of truth for values used across
// multiple layers (frontend components, backend config). Keep in sync with
// backend/src/config.rs::DEFAULT_EXECUTION_TIMEOUT_SECS.
// =============================================================================

import { getDefaultExecutor } from '@/utils/executors';

/** Default execution timeout in seconds (1 hour). Used as fallback when no config is set. */
export const DEFAULT_EXECUTION_TIMEOUT_SECS = 3600;

/** Maximum execution timeout in minutes (7 days = 10080 min).
 * Derived from backend config::MAX_EXECUTION_TIMEOUT_SECS (604800 seconds).
 * If you change one side, update the other: MAX_EXECUTION_TIMEOUT_SECS / 60 = 10080. */
export const MAX_EXECUTION_TIMEOUT_MINUTES = 10080;

// =============================================================================
// Status colors — single source of truth for status-related colors.
// =============================================================================

/** Todo 执行状态颜色 */
// `completed` 与 `success` 指向同一颜色：历史 API/前端组件有两套命名约定
// （后端 status 字段为 'success'，前端 DashboardStats.completed_todos 用 'completed'），
// 这里并列两个键以保持两边调用方都能直接拿到颜色，避免再次出现双源 STATUS_COLORS。
export const STATUS_COLORS = {
  /** 待执行 (pending) */
  pending: '#06b6d4',
  /** 执行中 (running) */
  running: '#f59e0b',
  /** 已完成 (success) — 后端 status 字段名 */
  success: '#22c55e',
  /** 已完成 (completed) — 前端 DashboardStats 字段名（与 success 同色） */
  completed: '#22c55e',
  /** 失败 (failed/error) */
  failed: '#ef4444',
  /** 定时任务 (cron/scheduled) */
  scheduled: '#8b5cf6',
  /** 评审中 (reviewing) */
  reviewing: '#06b6d4',
  /** 评审通过 (review passed) */
  reviewPassed: '#10b981',
  /** 评审失败 (review failed) */
  reviewFailed: '#ef4444',
  /** 评审中断 (review interrupted) */
  reviewInterrupted: '#f59e0b',
  /** Hook 触发 */
  hook: '#a855f7',
} as const;

/** Log 类型颜色（浅色主题，供 Dashboard 等不区分主题的组件使用） */
// 使用 Record<string, string> 而非 as const，因为 LOG_TYPE_COLORS 需要通过
// 动态字符串键访问（如 LOG_TYPE_COLORS[log.type]，log.type 是运行时字符串）。
export const LOG_TYPE_COLORS: Record<string, string> = {
  info: '#6b7280',
  stdout: '#3b82f6',
  stderr: '#ef4444',
  error: '#ef4444',
  tool_use: '#8b5cf6',
  tool_call: '#8b5cf6',
  tool_result: '#10b981',
  assistant: '#0ea5e9',
  user: '#f59e0b',
  system: '#6b7280',
  thinking: '#a855f7',
  result: '#22c55e',
  step_start: '#06b6d4',
  step_finish: '#06b6d4',
  tokens: '#6b7280',
  // 兼容 helpers.ts 迁移的旧键
  text: '#4ade80',
  tool: '#fbbf24',
};

/** Log 类型颜色 — 浅色主题专用（ExecutionPanel 等需要区分主题的组件使用） */
export const LOG_TYPE_COLORS_LIGHT: Record<string, string> = {
  info: '#3b82f6',
  text: '#22c55e',
  tool: '#f59e0b',
  step_start: '#8b5cf6',
  step_finish: '#14b8a6',
  stdout: '#64748b',
  stderr: '#ef4444',
  error: '#dc2626',
  system: '#6b7280',
  assistant: '#7c3aed',
  user: '#0891b2',
  result: '#22c55e',
  thinking: '#f97316',
};

/** Log 类型颜色 — 深色主题专用（Catppuccin Mocha 风格） */
export const LOG_TYPE_COLORS_DARK: Record<string, string> = {
  info: '#89b4fa',
  text: '#a6e3a1',
  tool: '#f9e2af',
  step_start: '#cba6f7',
  step_finish: '#94e2d5',
  stdout: '#cdd6f4',
  stderr: '#f38ba8',
  error: '#f38ba8',
  system: '#6c7086',
  assistant: '#cba6f7',
  user: '#89dceb',
  result: '#a6e3a1',
  thinking: '#fab387',
};

/** Log 类型显示标签 */
export const LOG_TYPE_LABELS: Record<string, string> = {
  info: 'INFO', text: 'TEXT', tool: 'TOOL', tool_use: 'TOOL', tool_call: 'TOOL',
  tool_result: 'RESULT', step_start: 'START', step_finish: 'END', stdout: 'OUT',
  stderr: 'ERR', error: 'ERROR', system: 'SYS', assistant: 'ASST', user: 'USER',
  result: 'RESULT', thinking: 'THINK', tokens: 'INFO',
};

/** 任务状态标签 */
export const STATUS_LABELS: Record<string, string> = {
  pending: '待处理',
  running: '运行中',
  completed: '已完成',
  failed: '失败',
};

/** 触发类型标签 */
export const TRIGGER_LABELS: Record<string, string> = {
  manual: '手动',
  cron: '定时',
  slash_command: '命令',
  default_response: '默认回复',
};

/** 触发类型颜色（独立于 STATUS_COLORS，值碰巧相同但语义不同） */
export const TRIGGER_COLORS: Record<string, string> = {
  manual: '#3b82f6',
  cron: '#8b5cf6',
  slash_command: '#f59e0b',
  default_response: '#22c55e',
};

/** 评审结果颜色，与 STATUS_COLORS 解耦 */
export const REVIEW_RESULT_COLORS: Record<string, string> = {
  pending: '#06b6d4',
  success: '#10b981',
  failed: '#ef4444',
  interrupted: '#f59e0b',
};

// =============================================================================
// Layout & UI constants — single source of truth for layout values.
// =============================================================================

/** 布局断点 */
export const BREAKPOINTS = {
  /** 宽屏断点：超过此宽度显示双栏布局。1440px 是主流笔记本屏幕（14-16 寸）
   *  的常见分辨率宽度，能同时容纳侧边栏和内容区而不拥挤。 */
  wide: 1440,
  /** 移动端断点：低于此宽度启用移动端适配。768px 是 iPad mini 竖屏宽度，
   *  也是 Bootstrap/主流 CSS 框架的常用平板断点。 */
  mobile: 768,
} as const;

/** 侧边栏宽度 */
export const SIDEBAR_WIDTH = {
  /** 桌面端侧边栏宽度。350px 能容纳 TodoCard 的标题、状态、标签等元素，
   *  且为右侧内容区留出足够空间（在 1440px 宽屏下内容区约 1090px）。 */
  desktop: 350,
  /** 移动端侧边栏占满宽度 */
  mobile: '100%',
} as const;

/** 左侧主导航栏宽度（桌面端）。 */
export const LEFT_RAIL_WIDTH = {
  collapsed: 72,
  expanded: 240,
} as const;

/** 执行面板高度 */
export const EXECUTION_PANEL = {
  /** 展开时高度。280px 能展示 3-4 条执行记录，过高会挤压上方 Todo 列表可视区。 */
  expanded: 280,
  /** 折叠时高度。40px 刚好容纳 header 行（含展开按钮和状态文本）。 */
  collapsed: 40,
} as const;

/** 导出限制 */
export const EXPORT = {
  /** 导出时最大获取日志条数。100000 条在 SQLite 单表查询中能在 1-2 秒内完成，
   *  超过此量级说明日志规模已不适合前端直接导出，应走数据库备份流程。 */
  maxLogs: 100000,
} as const;

// =============================================================================
// Executor last-choice persistence — single key shared across all components
// that offer executor selection (ActionButton, TodoDrawer, QuickCaptureModal).
// When the user picks an executor in one place, the other two remember it too.
// =============================================================================

// =============================================================================
// Cross-component event names — single source of truth for custom event names
// used by window.dispatchEvent / addEventListener.
// =============================================================================

/** TodoDrawer 新建/保存事项后，通知 TodoList 和 TodoCenterCardView 刷新列表 */
export const TODO_LIST_REFRESH_EVENT = 'todoListRefresh';

// localStorage key 仅本文件内部用，跨组件记忆用户上次选择的执行器
const LAST_EXECUTOR_STORAGE_KEY = 'ntd_last_executor';

/** 从 localStorage 读出上次选择的执行器，不存在时回退到系统默认执行器 */
export function getLastExecutor(defaultExecutor?: string): string {
  try {
    const saved = localStorage.getItem(LAST_EXECUTOR_STORAGE_KEY);
    if (saved) return saved;
    // 没有保存值时，优先使用调用方传入的默认值，否则使用系统默认执行器
    return defaultExecutor || getSystemDefaultExecutor();
  } catch {
    // 隐私模式 / 配额满：静默吞掉，返回系统默认
    return defaultExecutor || getSystemDefaultExecutor();
  }
}

/** 获取系统默认执行器（从缓存读取，回退到 claudecode） */
function getSystemDefaultExecutor(): string {
  return getDefaultExecutor();
}

/** 将用户选择的执行器写入 localStorage */
export function setLastExecutor(executor: string) {
  try {
    localStorage.setItem(LAST_EXECUTOR_STORAGE_KEY, executor);
  } catch {
    // 写入失败不阻塞用户体验
  }
}
