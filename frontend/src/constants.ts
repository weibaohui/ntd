// =============================================================================
// Shared application constants — single source of truth for values used across
// multiple layers (frontend components, backend config). Keep in sync with
// backend/src/config.rs::DEFAULT_EXECUTION_TIMEOUT_SECS.
// =============================================================================

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
export const STATUS_COLORS = {
  /** 待执行 (pending) */
  pending: '#06b6d4',
  /** 执行中 (running) */
  running: '#f59e0b',
  /** 已完成 (completed/success) */
  success: '#22c55e',
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

/** Log 类型颜色 */
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
};

/** 默认执行器名称 */
export const DEFAULT_EXECUTOR = 'claudecode';

// =============================================================================
// Layout & UI constants — single source of truth for layout values.
// =============================================================================

/** 布局断点 */
export const BREAKPOINTS = {
  /** 宽屏断点：超过此宽度显示双栏布局 */
  wide: 1440,
  /** 移动端断点：低于此宽度启用移动端适配 */
  mobile: 768,
} as const;

/** 侧边栏宽度 */
export const SIDEBAR_WIDTH = {
  /** 桌面端侧边栏宽度 */
  desktop: 350,
  /** 移动端侧边栏占满宽度 */
  mobile: '100%',
} as const;

/** 执行面板高度 */
export const EXECUTION_PANEL = {
  /** 展开时高度 */
  expanded: 280,
  /** 折叠时高度 */
  collapsed: 40,
} as const;

/** 触摸滑动阈值 */
export const SWIPE = {
  /** 滑动距离阈值（px） */
  threshold: 50,
  /** 滑动最大时间（ms） */
  maxTime: 300,
} as const;

/** 定时器间隔 */
export const TIMER = {
  /** 执行状态刷新间隔（ms） */
  tickInterval: 1000,
  /** 完成任务自动移除延迟（ms） */
  autoRemoveDelay: 5000,
} as const;

/** 文本截断长度 */
export const TEXT_TRUNCATE = {
  /** Todo 标题截断长度 */
  todoTitle: 60,
  /** Todo 摘要截断长度 */
  todoSummary: 20,
  /** 工具输入预览截断长度 */
  toolInput: 50,
  /** 工具输入字段值截断长度 */
  toolInputField: 15,
  /** 日志内容截断长度 */
  logContent: 2000,
} as const;

/** 导出限制 */
export const EXPORT = {
  /** 导出时最大获取日志条数 */
  maxLogs: 100000,
} as const;

/** 复制反馈延迟 */
export const COPY_FEEDBACK_DELAY = 2000;
