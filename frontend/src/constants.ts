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
