import type { ReactNode } from 'react';
import { FaSquare } from 'react-icons/fa';
import type { TodoHookItem } from '../utils/database/hooks';

export interface Todo {
  id: number;
  title: string;
  prompt: string;
  status: 'pending' | 'running' | 'completed' | 'failed';
  created_at: string;
  updated_at: string;
  deleted_at: string | null;
  tag_ids: number[];
  executor?: string;
  scheduler_enabled?: boolean;
  scheduler_config?: string | null;
  scheduler_timezone?: string | null;
  scheduler_next_run_at?: string | null;
  task_id?: string | null;
  workspace?: string | null;
  worktree_enabled?: boolean;
  hooks?: TodoHookItem[];
}

export interface Tag {
  id: number;
  name: string;
  color: string;
  created_at: string;
}

export interface TodoTag {
  todo_id: number;
  tag_id: number;
}

export interface TodoTemplate {
  id: number;
  title: string;
  prompt: string | null;
  category: string;
  sort_order: number;
  is_system: boolean;
  source_url?: string | null;
  last_sync_at?: string | null;
  created_at: string | null;
  updated_at: string | null;
}

export interface CustomTemplateStatus {
  subscribed: boolean;
  source_url: string | null;
  last_sync_at: string | null;
  auto_sync_enabled: boolean;
  auto_sync_cron: string;
  templates: TodoTemplate[];
}

export interface LogEntry {
  timestamp: string;
  type: 'info' | 'stdout' | 'stderr' | 'error' | 'text' | 'tool' | 'tool_use' | 'tool_call' | 'tool_result' | 'step_start' | 'step_finish' | 'result' | 'assistant' | 'user' | 'system' | 'thinking' | 'tokens';
  content: string;
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

export interface TodoItem {
  id?: string;
  content: string;
  status: 'pending' | 'in_progress' | 'completed';
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

export interface ExecutorCount {
  executor: string;
  count: number;
  execution_count: number;
  success_count: number;
  failed_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cost_usd: number;
}

export interface TagCount {
  tag_id: number;
  tag_name: string;
  tag_color: string;
  count: number;
  execution_count: number;
  success_count: number;
  failed_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cost_usd: number;
}

export interface ModelCount {
  model: string;
  count: number;
  execution_count: number;
  success_count: number;
  failed_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cache_read_tokens: number;
  total_cache_creation_tokens: number;
  total_cost_usd: number;
}

export interface DailyExecution {
  date: string;
  success: number;
  failed: number;
}

export interface DailyTokenStats {
  date: string;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
  total_cost_usd: number;
}

export interface DashboardStats {
  total_todos: number;
  pending_todos: number;
  running_todos: number;
  completed_todos: number;
  failed_todos: number;
  total_tags: number;
  scheduled_todos: number;
  total_executions: number;
  success_executions: number;
  failed_executions: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cache_read_tokens: number;
  total_cache_creation_tokens: number;
  total_cost_usd: number;
  avg_duration_ms: number;
  executor_distribution: ExecutorCount[];
  tag_distribution: TagCount[];
  model_distribution: ModelCount[];
  daily_executions: DailyExecution[];
  daily_token_stats: DailyTokenStats[];
  recent_executions: ExecutionRecord[];
  trigger_type_distribution: TriggerTypeCount[];
  executor_duration_stats: ExecutorDuration[];
  model_cache_stats: ModelCacheStat[];
  // 增强指标卡片字段
  today_executions?: number;
  executions_change?: number;
  success_rate_change?: number;
  cost_change?: number;
  active_days?: number;
  streak_days?: number;
  peak_daily_executions?: number;
  top_model?: string;
  top_model_tokens?: number;
  leaderboard?: LeaderboardItem[];
  // Skills metrics
  skills_stats?: SkillsStats;
  // Backup metrics
  backup_stats?: BackupStats;
}

export interface LeaderboardItem {
  rank: number;
  name: string;
  tokens: number;
  sessions: number;
  change?: number;
}

export interface TriggerTypeCount {
  trigger_type: string;
  count: number;
  success_count: number;
  failed_count: number;
}

export interface ExecutorDuration {
  executor: string;
  avg_duration_ms: number;
  execution_count: number;
}

export interface ModelCacheStat {
  model: string;
  total_input_tokens: number;
  total_cache_read_tokens: number;
  cache_hit_rate: number;
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

export interface ExecutorOption {
  value: string;
  label: string;
  color: string;
  icon: ReactNode;
}

export const EXECUTORS: ExecutorOption[] = [
  { value: 'claudecode', label: 'Claude',    color: '#e17055', icon: <FaSquare color="#e17055" size={14} /> },
  { value: 'codebuddy',  label: 'CodeBuddy', color: '#00b894', icon: <FaSquare color="#00b894" size={14} /> },
  { value: 'opencode',   label: 'Opencode',  color: '#fdcb6e', icon: <FaSquare color="#fdcb6e" size={14} /> },
  { value: 'joinai',     label: 'JoinAI',    color: '#6c5ce7', icon: <FaSquare color="#6c5ce7" size={14} /> },
  { value: 'atomcode',   label: 'AtomCode',  color: '#e84393', icon: <FaSquare color="#e84393" size={14} /> },
  { value: 'hermes',     label: 'Hermes',    color: '#0984e3', icon: <FaSquare color="#0984e3" size={14} /> },
  { value: 'kimi',       label: 'Kimi',      color: '#d63031', icon: <FaSquare color="#d63031" size={14} /> },
  { value: 'codex',      label: 'Codex',     color: '#488597', icon: <FaSquare color="#488597" size={14} /> },
  // `agents` 是只读 skill 来源（`~/.agents/skills`），不在「执行器管理」显示，
  // 但会出现在 Skills 总览/对比/同步里。这里加进 EXECUTORS 是为了 Tab 渲染。
  // 颜色选深灰 `#2d3436` 故意区别于其他 8 个暖色调（橙/绿/黄/紫/红/蓝/粉），
  // 视觉上传递「这个是只读的、跟其他可写执行器不同」的信号。
  { value: 'agents',     label: 'Agents',    color: '#2d3436', icon: <FaSquare color="#2d3436" size={14} /> },
];

export const EXECUTOR_COLORS: Record<string, string> = {
  claudecode: '#e17055',
  codebuddy: '#00b894',
  opencode: '#fdcb6e',
  joinai: '#6c5ce7',
  atomcode: '#e84393',
  hermes: '#0984e3',
  kimi: '#d63031',
  codex: '#488597',
  agents: '#2d3436',
  // Aliases for backward compatibility with database names
  'claude_code': '#e17055', // alias for claudecode
  'claude': '#e17055',       // alias for claudecode
  'cbc': '#00b894',          // alias for codebuddy
  'atom': '#e84393',         // alias for atomcode
};

// Get executor color with alias support
export function getExecutorColor(name: string | undefined | null): string {
  if (!name) return '#999';
  return EXECUTOR_COLORS[name] || '#999';
}

export interface ExecutorConfig {
  id: number;
  name: string;
  path: string;
  enabled: boolean;
  display_name: string;
  session_dir: string;
  created_at: string | null;
  updated_at: string | null;
}

export function executorConfigToOption(ec: ExecutorConfig): ExecutorOption {
  const color = getExecutorColor(ec.name);
  return {
    value: ec.name,
    label: ec.display_name || ec.name,
    color,
    icon: <FaSquare color={color} size={14} />,
  };
}

export interface SlashCommandRule {
  slash_command: string;
  todo_id: number;
  enabled: boolean;
}

export interface Config {
  port: number;
  host: string;
  db_path: string;
  log_level: string;
  slash_command_rules?: SlashCommandRule[];
  default_response_todo_id?: number | null;
  history_message_max_age_secs?: number;
  max_concurrent_todos?: number;
  execution_timeout_secs?: number;
  scheduler_default_timezone?: string;
}

export const RESUMABLE_EXECUTORS = new Set(['claudecode', 'kimi', 'opencode', 'joinai', 'hermes']);

export function supportsResume(record: ExecutionRecord): boolean {
  return (
    record.status !== 'running' &&
    !!record.session_id &&
    !!record.executor &&
    RESUMABLE_EXECUTORS.has(record.executor.toLowerCase())
  );
}

export function getExecutorOption(value: string): ExecutorOption {
  return EXECUTORS.find(e => e.value === value.toLowerCase()) || EXECUTORS[0];
}

// Skills types
export interface SkillMeta {
  name: string;
  description: string;
  version: string | null;
  author: string | null;
  license: string | null;
  keywords: string[];
  file_count: number;
  total_size: number;
  modified_at: string | null;
}

export interface ExecutorSkills {
  executor: string;
  executor_label: string;
  skills_dir: string;
  skills_dir_exists: boolean;
  skills: SkillMeta[];
}

export interface SkillPresence {
  present: boolean;
  version: string | null;
  modified_at: string | null;
}

export interface SkillComparison {
  skill_name: string;
  description: string;
  executors: Record<string, SkillPresence>;
}

export interface SkillInvocation {
  id: number;
  skill_name: string;
  executor: string;
  todo_id: number;
  todo_title: string | null;
  invoked_at: string;
  status: string;
  duration_ms: number | null;
}

export interface PaginatedInvocations {
  items: SkillInvocation[];
  total: number;
  page: number;
  limit: number;
}

// Feishu History types
export interface FeishuHistoryMessage {
  id: number;
  message_id: string;
  chat_id: string;
  chat_type: string;
  sender_open_id: string;
  sender_nickname: string | null;
  sender_type: string | null;
  content: string | null;
  msg_type: string;
  is_history: boolean;
  processed: boolean;
  processed_todo_id: number | null;
  execution_record_id: number | null;
  created_at: string | null;
}

export interface FeishuHistoryMessagesPage {
  messages: FeishuHistoryMessage[];
  total: number;
  page: number;
  page_size: number;
}

export interface FeishuMessageStats {
  total_messages: number;
  processed: number;
  unprocessed: number;
  triggered_todos: number;
  unique_senders: number;
  last_24h_messages: number;
  unique_chats: number;
}

export interface RecentCompletedTodo {
  todo_id: number;
  title: string;
  prompt: string | null;
  executor: string | null;
  tag_ids: number[];
  completed_at: string;
  result: string | null;
  model: string | null;
  usage: ExecutionUsage | null;
  execution_status: string;
  trigger_type: string;
  record_id: number;
}

export interface FeishuHistoryChat {
  id: number;
  bot_id: number;
  chat_id: string;
  chat_name: string | null;
  enabled: boolean;
  last_fetch_time: string | null;
  polling_interval_secs: number;
  created_at: string | null;
}

// Skills statistics
export interface SkillsStats {
  total_invocations: number;
  success_invocations: number;
  failed_invocations: number;
  avg_duration_ms: number;
  invocations_today: number;
  top_skills: SkillTop[];
  executor_skills_count: ExecutorSkillCount[];
  daily_invocations: DailySkillInvocation[];
}

export interface SkillTop {
  skill_name: string;
  count: number;
  success_rate: number;
}

export interface ExecutorSkillCount {
  executor: string;
  skills_count: number;
}

export interface DailySkillInvocation {
  date: string;
  count: number;
  success: number;
}

// Backup statistics
export interface BackupStats {
  auto_backup_enabled: boolean;
  last_backup: string | null;
  auto_backup_cron: string;
  database: BackupCategoryStats;
  todo: BackupCategoryStats;
  skills: BackupCategoryStats;
  total_file_count: number;
  total_size: number;
  total_size_formatted: string;
  recent_backups: RecentBackup[];
}

export interface BackupCategoryStats {
  file_count: number;
  total_size: number;
  last_backup: string | null;
}

export interface RecentBackup {
  type: string;
  name: string;
  size: number;
  created_at: string;
}

// Usage Statistics types (from ccusage integration)
export interface UsageStat {
  date: string;
  input_tokens: number;
  output_tokens: number;
  cache_creation_tokens: number;
  cache_read_tokens: number;
  extra_total_tokens: number;
  total_cost: number;
  credits: number | null;
  message_count: number | null;
  models_used: string[];
  project: string | null;
  last_activity: string | null;
  stats_type: string;
}

export interface ModelBreakdown {
  date: string;
  model_name: string;
  input_tokens: number;
  output_tokens: number;
  cache_creation_tokens: number;
  cache_read_tokens: number;
  extra_total_tokens: number;
  cost: number;
}

export interface UsageStatsResponse {
  daily: UsageStat[];
  weekly: UsageStat[];
  monthly: UsageStat[];
  breakdowns: ModelBreakdown[];
}
