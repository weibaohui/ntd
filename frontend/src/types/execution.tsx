// ─── Execution types ────────────────────────────────────────
//
// 纯类型定义文件：运行时常量（EXECUTORS 等）已迁至 utils/executors.ts。
// 本文件通过 re-export 保持 import { EXECUTORS } from '@/types' 兼容。

import type { ReactNode } from 'react';
import type { TodoItem } from './todo';

// Re-export runtime values for backward compatibility
export {
  EXECUTORS,
  EXECUTOR_COLORS,
  getExecutorColor,
  getExecutorOption,
  EXECUTORS_FOR_PICKER,
  RESUMABLE_EXECUTORS,
  DEFAULT_EXECUTOR,
  RESUMABLE_EXECUTOR_OPTIONS,
  supportsResume,
  executorConfigToOption,
  loadDefaultExecutor,
  getDefaultExecutor,
  setDefaultExecutorCache,
} from '@/utils/executors';

export interface LogEntry {
  timestamp: string;
  type: 'info' | 'stdout' | 'stderr' | 'error' | 'text' | 'tool' | 'tool_use' | 'tool_call' | 'tool_result' | 'step_start' | 'step_finish' | 'result' | 'assistant' | 'user' | 'system' | 'thinking' | 'tokens';
  content: string;
  // issue #648: 工具调用上下文，从后端 ParsedLogEntry 的 metadata 透出
  // （详见 backend/src/db/execution.rs 中的 metadata 序列化）。
  toolName?: string;
  toolInputJson?: string;
  /** 工具返回的原始文本（仅在 log_type === 'tool_result' 时填充） */
  toolResult?: string;
  /** 关联 ID；前后端尚未透出 id 时退化为顺序配对 */
  toolCallId?: string;
  isError?: boolean;
}

/**
 * issue #648: 从日志中提取出的"命令+返回"对。
 *
 * 之所以用独立类型而不是直接渲染 LogEntry，是因为：
 * - 一个 Bash 调用跨两条日志（tool_use + tool_result）；
 * - 不同执行器协议差异巨大，需要在 view 层做归一化。
 */
export interface CommandEntry {
  id: string;
  toolName: string;
  command: string;
  args?: string;
  output?: string;
  success: boolean;
  exitCode?: number;
  durationMs?: number;
  timestamp: string;
}

export interface ChatMessage {
  role: 'user' | 'assistant' | 'system' | 'tool' | 'thinking' | 'result';
  content: string;
  timestamp?: string;
  toolName?: string;
  toolInput?: string;
  toolResult?: string;
  isCollapsed?: boolean;
}

export interface ExecutionUsage {
  input_tokens: number;
  output_tokens: number;
  cache_read_input_tokens: number | null;
  cache_creation_input_tokens: number | null;
  total_cost_usd: number | null;
  duration_ms: number | null;
}

export interface ExecutionStats {
  tool_calls: number;
  conversation_turns: number;
  thinking_count: number;
}

/**
 * 多 Agent 协作中单个子 agent 的元数据。
 * 与后端 backend/src/agent_progress.rs::AgentRun 一一对应。
 * **只含元数据**（名称/角色/状态/启动时间），不含 prompt/result 原文——
 * 原文在 execution_logs 里，LogDrawer 的 Agent Tab 按 tool_name 扫日志展示。
 */
export interface AgentRun {
  name: string;
  role?: string;
  status: string;
  started_at?: string;
}

export interface ExecutionRecord {
  id: number;
  todo_id: number;
  status: 'running' | 'success' | 'failed';
  command: string;
  stdout: string;
  stderr: string;
  result: string | null;
  started_at: string;
  finished_at: string | null;
  usage: ExecutionUsage | null;
  executor: string | null;
  model: string | null;
  trigger_type: string;
  pid: number | null;
  task_id?: string | null;
  session_id?: string | null;
  todo_progress?: string | null;
  /** 多 Agent 协作的子 agent 元数据（JSON 字符串，AgentRun[]）。后端完成态写入，前端自行 parse。 */
  agent_runs?: string | null;
  execution_stats?: ExecutionStats | null;
  resume_message?: string | null;
  source_todo_id?: number | null;
  source_todo_title?: string | null;
  source_hook_id?: number | null;
  /** User-provided score for this execution's result (0-100). */
  rating?: number | null;
  /** For review instance records: the original execution record that was reviewed. */
  source_execution_record_id?: number | null;
  /** For the original execution record: status of the most recent auto-review. */
  last_review_status?: 'pending' | 'success' | 'failed' | 'interrupted' | null;
  /** ISO timestamp when the most recent auto-review finished. */
  last_reviewed_at?: string | null;
  /** issue #643/#645: 本次执行使用的 git worktree 目录路径；未启用 worktree 时为 null。 */
  worktree_path?: string | null;
}

export interface ExecutionSummary {
  todo_id: number;
  total_executions: number;
  success_count: number;
  failed_count: number;
  running_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cache_read_tokens: number;
  total_cache_creation_tokens: number;
  total_cost_usd: number | null;
}

export interface ExecutionRecordsPage {
  records: ExecutionRecord[];
  total: number;
  page: number;
  limit: number;
}

export interface ExecutionLogsPage {
  logs: LogEntry[];
  total: number;
  page: number;
  per_page: number;
}

export interface ExecuteResult {
  success: boolean;
  stdout: string;
  stderr: string;
  logs: LogEntry[];
}

export interface RunningTask {
  taskId: string;
  todoId: number;
  todoTitle: string;
  executor: string;
  logs: LogEntry[];
  status: 'running' | 'finished';
  success?: boolean;
  result?: string | null;
  startedAt: string;
  finishedAt?: string;
  todoProgress?: TodoItem[];
  executionStats?: ExecutionStats;
}

// ─── Executor type ──────────────────────────────────────────
// ExecutorOption 类型定义保留在此（纯类型），运行时常量在 utils/executors.ts。

export interface ExecutorOption {
  value: string;
  label: string;
  color: string;
  icon: ReactNode;
  /** 是否支持继续对话（resumable）。默认 false。 */
  resumable?: boolean;
}

// ─── Running Board types ────────────────────────────────────

export interface ScheduledTodo {
  id: number;
  title: string;
  prompt: string;
  status: string;
  executor: string | null;
  scheduler_enabled: boolean;
  scheduler_config: string | null;
  scheduler_timezone: string | null;
  scheduler_next_run_at: string | null;
  tag_ids: number[];
  workspace_id: number | null;
  updated_at: string;
}

export interface RunningBoardData {
  records: ExecutionRecord[];
  scheduled_todos: ScheduledTodo[];
  total: number;
  page: number;
  limit: number;
}

export type RunningBoardColumn = 'scheduled' | 'running' | 'completed' | 'reviewing' | 'review_passed' | 'failed';
