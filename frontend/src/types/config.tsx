// ─── Config types ───────────────────────────────────────────

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
  /** 黑板更新防抖周期（秒），默认 600 秒 */
  blackboard_debounce_secs?: number;
  /** 黑板更新防抖条数阈值，达到此条数立即触发 */
  blackboard_debounce_count?: number;
    /** Wiki 索引页面维护提示词模板（占位符 {{page_summaries}}、{{pending_record_ids}}）*/
    wiki_index_prompt?: string;
    /** Wiki 主题页面生成提示词模板（占位符 {{operations_json}}、{{page_contents}}）*/
    wiki_page_prompt?: string;
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
