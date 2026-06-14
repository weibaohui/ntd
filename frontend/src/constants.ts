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
// 使用 Record<string, string> 而非 as const，因为 LOG_TYPE_COLORS 需要通过
// 动态字符串键访问（如 LOG_TYPE_COLORS[log.type]，log.type 是运行时字符串）。
// STATUS_COLORS 使用 as const 是因为所有使用处都是已知键（如 .running）。
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

/** 执行面板高度 */
export const EXECUTION_PANEL = {
  /** 展开时高度。280px 能展示 3-4 条执行记录，过高会挤压上方 Todo 列表可视区。 */
  expanded: 280,
  /** 折叠时高度。40px 刚好容纳 header 行（含展开按钮和状态文本）。 */
  collapsed: 40,
} as const;

/** 触摸滑动阈值 */
export const SWIPE = {
  /** 滑动距离阈值（px）。50px 是人手指在屏幕上做"滑动删除"手势的合理最小距离，
   *  低于此值视为误触而不触发滑动操作。 */
  threshold: 50,
  /** 滑动最大时间（ms）。300ms 是快速滑动手势的典型时长，超过此值视为
   *  慢速拖动而非滑动操作。 */
  maxTime: 300,
} as const;

/** 定时器间隔 */
export const TIMER = {
  /** 执行状态刷新间隔（ms）。1000ms 是轮询的合理平衡：低于此值增加服务端负载
   *  而无明显 UX 改善（人类感知延迟阈值约 100-200ms 但执行状态变化本身是分钟级的）。 */
  tickInterval: 1000,
  /** 完成任务自动移除延迟（ms）。5000ms 给用户足够时间看到"已完成"状态后
   *  再自动移除，避免突兀消失。 */
  autoRemoveDelay: 5000,
} as const;

/** 文本截断长度 */
export const TEXT_TRUNCATE = {
  /** Todo 标题截断长度。60 字符约等于一行中文字符宽度，
   *  在 350px 侧边栏内能完整显示而不换行。 */
  todoTitle: 60,
  /** Todo 摘要截断长度。20 字符在卡片中作为副标题预览足够。 */
  todoSummary: 20,
  /** 工具输入预览截断长度。50 字符支持显示完整路径或命令。 */
  toolInput: 50,
  /** 工具输入字段值截断长度。15 字符适合显示单个参数值。 */
  toolInputField: 15,
  /** 日志内容截断长度。2000 字符确保抽屉中日志渲染不阻塞 UI，
   *  超长日志在后端已分片存储，前端只展示前 2000 字预览。 */
  logContent: 2000,
} as const;

/** 导出限制 */
export const EXPORT = {
  /** 导出时最大获取日志条数。100000 条在 SQLite 单表查询中能在 1-2 秒内完成，
   *  超过此量级说明日志规模已不适合前端直接导出，应走数据库备份流程。 */
  maxLogs: 100000,
} as const;

/** 复制反馈延迟 */
export const COPY_FEEDBACK_DELAY = 2000;
