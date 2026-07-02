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
