import type { ReactNode } from 'react';
import { FaSquare } from 'react-icons/fa';
import type { TodoItem } from './todo';

// ─── Execution types ────────────────────────────────────────

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

// ─── Executor types ──────────────────────────────────────────

export interface ExecutorOption {
  value: string;
  label: string;
  color: string;
  icon: ReactNode;
  /** 是否支持继续对话（resumable）。默认 false。 */
  resumable?: boolean;
}

export const EXECUTORS: ExecutorOption[] = [
  { value: 'claudecode', label: 'Claude',    color: '#e17055', icon: <FaSquare color="#e17055" size={14} />, resumable: true },
  { value: 'codebuddy',  label: 'CodeBuddy', color: '#00b894', icon: <FaSquare color="#00b894" size={14} /> },
  { value: 'opencode',   label: 'Opencode',  color: '#fdcb6e', icon: <FaSquare color="#fdcb6e" size={14} />, resumable: true },
  { value: 'mobilecoder', label: 'MobileCoder', color: '#6c5ce7', icon: <FaSquare color="#6c5ce7" size={14} />, resumable: true },
  { value: 'atomcode',   label: 'AtomCode',  color: '#e84393', icon: <FaSquare color="#e84393" size={14} /> },
  { value: 'hermes',     label: 'Hermes',    color: '#0984e3', icon: <FaSquare color="#0984e3" size={14} />, resumable: true },
  { value: 'kimi',       label: 'Kimi',      color: '#d63031', icon: <FaSquare color="#d63031" size={14} />, resumable: true },
  { value: 'codex',      label: 'Codex',     color: '#488597', icon: <FaSquare color="#488597" size={14} /> },
  { value: 'codewhale',  label: 'CodeWhale', color: '#00cec9', icon: <FaSquare color="#00cec9" size={14} />, resumable: true },
  { value: 'pi',        label: 'Pi',        color: '#8e44ad', icon: <FaSquare color="#8e44ad" size={14} />, resumable: true },
  { value: 'mimo',      label: 'MiMo',      color: '#ff6b6b', icon: <FaSquare color="#ff6b6b" size={14} />, resumable: true },
  // Issue #673: 新增 Zhanlu 执行器，与 Opencode 输出格式一致
  // 颜色与下方 EXECUTOR_COLORS.zhanlu 同步为 #0f766e，与 agents(#2d3436) 视觉可分。
  { value: 'zhanlu',    label: 'Zhanlu',    color: '#0f766e', icon: <FaSquare color="#0f766e" size={14} />, resumable: true },
  // `agents` is read-only skill source (`~/.agents/skills`), not shown in executor management.
  // Included here so it appears in Skills overview/sync tabs.
  { value: 'agents',     label: 'Agents',    color: '#2d3436', icon: <FaSquare color="#2d3436" size={14} /> },
];

export const EXECUTOR_COLORS: Record<string, string> = {
  claudecode: '#e17055',
  codebuddy: '#00b894',
  opencode: '#fdcb6e',
  mobilecoder: '#6c5ce7',
  atomcode: '#e84393',
  hermes: '#0984e3',
  kimi: '#d63031',
  codex: '#488597',
  codewhale: '#00cec9',
  pi: '#8e44ad',
  mimo: '#ff6b6b',
  // Issue #673 + PR #677 review H1：zhanlu 颜色与 agents 撞色（都是 #2d3436），
  // 改为深青 `#0f766e` 与 opencode 的 `#fdcb6e` / agents 的 `#2d3436` 视觉可分。
  zhanlu: '#0f766e',
  agents: '#2d3436',
  // Aliases for backward compatibility with database names
  'claude_code': '#e17055',
  'claude': '#e17055',
  'cbc': '#00b894',
  'atom': '#e84393',
};

// Get executor color with alias support
export function getExecutorColor(name: string | undefined | null): string {
  if (!name) return '#999';
  return EXECUTOR_COLORS[name] || '#999';
}

export function getExecutorOption(value: string): ExecutorOption {
  return EXECUTORS.find(e => e.value === value.toLowerCase()) || EXECUTORS[0];
}

/** 不包含 `agents` 的执行器列表，用于执行器选择 UI（agents 是只读 skill 来源，不是执行器）。 */
export const EXECUTORS_FOR_PICKER = EXECUTORS.filter(e => e.value !== 'agents');

/** 支持继续对话的执行器 value 集合。从 EXECUTORS 的 resumable 标志自动派生，无需手动维护。 */
export const RESUMABLE_EXECUTORS = new Set(EXECUTORS.filter(e => e.resumable).map(e => e.value));

/// 默认执行器
export const DEFAULT_EXECUTOR = 'claudecode';

/** 仅支持继续对话的执行器（用于选择 UI 的下拉数据源） */
export const RESUMABLE_EXECUTOR_OPTIONS = EXECUTORS.filter(e => RESUMABLE_EXECUTORS.has(e.value));

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
  workspace_path: string | null;
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

export function supportsResume(record: ExecutionRecord): boolean {
  return (
    record.status !== 'running' &&
    !!record.session_id &&
    !!record.executor &&
    RESUMABLE_EXECUTORS.has(record.executor.toLowerCase())
  );
}

// Defined here to avoid circular dependency (execution -> todo -> execution)
// executorConfigToOption needs ExecutorOption (execution) + ExecutorConfig (config)
export function executorConfigToOption(ec: { name: string; display_name: string }): ExecutorOption {
  const color = getExecutorColor(ec.name);
  return {
    value: ec.name,
    label: ec.display_name || ec.name,
    color,
    icon: <FaSquare color={color} size={14} />,
  };
}
