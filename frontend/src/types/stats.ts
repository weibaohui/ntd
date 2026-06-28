import type { ExecutionRecord, ExecutionUsage } from './execution';

// ─── Dashboard & Statistics types ────────────────────────────

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

export interface LeaderboardItem {
  rank: number;
  name: string;
  tokens: number;
  sessions: number;
  change?: number;
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
  // Enhanced metrics
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
  skills_stats?: SkillsStats;
  backup_stats?: BackupStats;
}

// ─── Skills types ────────────────────────────────────────────

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

// ─── Feishu types ────────────────────────────────────────────

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
  processed_id: number | null;
  processed_type: string | null;
  execution_record_id: number | null;
  created_at: string | null;
  workspace_id: number | null;
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

// ─── Skills Statistics ────────────────────────────────────────

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

// ─── Backup types ────────────────────────────────────────────

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

// ─── Usage Statistics (ccusage integration) ──────────────────

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
  /** User-provided score for the most recent execution record (0-100). */
  rating?: number | null;
}
