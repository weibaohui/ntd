//! Usage statistics service
//!
//! Reads usage data from various AI code editor JSONL session files.
//! Follows ccusage's approach of reading raw session files directly.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{Datelike, DateTime, TimeZone};
use tokio::io::AsyncReadExt;
use tokio::fs;

use crate::db::Database;

/// Represents aggregated usage statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsageStat {
    pub date: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub extra_total_tokens: i64,
    pub total_cost: f64,
    pub credits: Option<f64>,
    pub message_count: Option<i64>,
    pub models_used: Vec<String>,
    pub project: Option<String>,
    pub last_activity: Option<String>,
    pub stats_type: String,
}

/// Model breakdown for a specific model
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelBreakdown {
    pub date: String,
    pub model_name: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub extra_total_tokens: i64,
    pub cost: f64,
}

/// Complete usage report with breakdowns
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsageReport {
    pub daily: Vec<UsageStat>,
    pub weekly: Vec<UsageStat>,
    pub monthly: Vec<UsageStat>,
}

/// Raw usage entry parsed from a JSONL session file
#[derive(Debug, Clone)]
pub struct RawUsageEntry {
    pub timestamp: i64,
    pub date: String,
    pub session_id: String,
    pub project_path: String,
    pub model: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub extra_total_tokens: u64,
    pub cost: f64,
}

/// Statistics collector
#[derive(Default)]
struct TokenAccumulator {
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
    extra_total_tokens: u64,
    cost: f64,
    models: HashMap<String, ModelAccumulator>,
}

impl TokenAccumulator {
    fn add_entry(&mut self, entry: &RawUsageEntry) {
        self.input_tokens += entry.input_tokens;
        self.output_tokens += entry.output_tokens;
        self.cache_creation_tokens += entry.cache_creation_tokens;
        self.cache_read_tokens += entry.cache_read_tokens;
        self.extra_total_tokens += entry.extra_total_tokens;
        self.cost += entry.cost;

        if let Some(ref model) = entry.model {
            let acc = self.models.entry(model.clone()).or_default();
            acc.input_tokens += entry.input_tokens;
            acc.output_tokens += entry.output_tokens;
            acc.cache_creation_tokens += entry.cache_creation_tokens;
            acc.cache_read_tokens += entry.cache_read_tokens;
            acc.extra_total_tokens += entry.extra_total_tokens;
            acc.cost += entry.cost;
        }
    }

    fn into_model_breakdowns(self) -> Vec<ModelBreakdown> {
        self.models
            .into_iter()
            .map(|(model_name, acc)| ModelBreakdown {
                date: String::new(),
                model_name,
                input_tokens: acc.input_tokens as i64,
                output_tokens: acc.output_tokens as i64,
                cache_creation_tokens: acc.cache_creation_tokens as i64,
                cache_read_tokens: acc.cache_read_tokens as i64,
                extra_total_tokens: acc.extra_total_tokens as i64,
                cost: acc.cost,
            })
            .collect()
    }
}

#[derive(Default)]
struct ModelAccumulator {
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
    extra_total_tokens: u64,
    cost: f64,
}

/// Usage statistics service
pub struct UsageStatsService {
    db: Arc<Database>,
}

impl UsageStatsService {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Collect all usage entries from all editors
    pub(crate) async fn collect_all_entries(&self) -> Vec<RawUsageEntry> {
        let mut all_entries = Vec::new();

        // Claude Code - read from JSONL session files
        let claude_entries = self.load_claude_jsonl_entries().await;
        all_entries.extend(claude_entries);

        // Codex - read from session files
        let codex_entries = self.load_codex_jsonl_entries().await;
        all_entries.extend(codex_entries);

        // OpenCode - read from session files
        let opencode_entries = self.load_opencode_jsonl_entries().await;
        all_entries.extend(opencode_entries);

        // Zhanlu (Issue #673) - read from session files in ~/.local/share/zhanlu/storage
        let zhanlu_entries = self.load_zhanlu_jsonl_entries().await;
        all_entries.extend(zhanlu_entries);

        // Kimi - read from wire.jsonl files
        let kimi_entries = self.load_kimi_jsonl_entries().await;
        all_entries.extend(kimi_entries);

        all_entries.sort_by_key(|e| e.timestamp);
        all_entries
    }

    /// Load entries from Claude Code JSONL files (~/.claude/projects/*/*.jsonl)
    async fn load_claude_jsonl_entries(&self) -> Vec<RawUsageEntry> {
        let mut entries = Vec::new();

        let Some(home) = dirs::home_dir() else {
            return entries;
        };

        let projects_dir = home.join(".claude").join("projects");
        if !projects_dir.exists() {
            return entries;
        }

        self.load_claude_jsonl_from_dir(&projects_dir, &mut entries).await;
        entries
    }

    async fn load_claude_jsonl_from_dir(&self, dir: &PathBuf, entries: &mut Vec<RawUsageEntry>) {
        let mut stack = vec![dir.clone()];

        while let Some(current_dir) = stack.pop() {
            if let Ok(mut dir) = fs::read_dir(&current_dir).await {
                while let Ok(Some(dir_entry)) = dir.next_entry().await {
                    let path = dir_entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                        self.parse_claude_jsonl_file(&path, entries).await;
                    }
                }
            }
        }
    }

    async fn parse_claude_jsonl_file(&self, path: &PathBuf, entries: &mut Vec<RawUsageEntry>) {
        let projects_str = ".claude/projects/";

        // Extract project path from file path
        let project_path = path.to_string_lossy()
            .split(projects_str)
            .nth(1)
            .and_then(|s| s.split('/').next())
            .map(|s| {
                if s.starts_with('-') {
                    s.replace('-', "/")
                } else {
                    s.to_string()
                }
            })
            .unwrap_or_else(|| "unknown".to_string());

        // Extract session ID from filename
        let session_id = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let mut file = match fs::File::open(path).await {
            Ok(f) => f,
            Err(_) => return,
        };

        let mut contents = String::new();
        if file.read_to_string(&mut contents).await.is_err() {
            return;
        }

        for line in contents.lines() {
            if let Some(entry) = self.parse_claude_jsonl_line(line, &session_id, &project_path) {
                entries.push(entry);
            }
        }
    }

    fn parse_claude_jsonl_line(&self, line: &str, session_id: &str, project_path: &str) -> Option<RawUsageEntry> {
        let json: serde_json::Value = serde_json::from_str(line).ok()?;

        // Only process assistant messages with usage data
        let msg_type = json.get("type")?.as_str()?;
        if msg_type != "assistant" {
            return None;
        }

        let message = json.get("message")?;
        let usage = message.get("usage")?;

        let timestamp_str = json.get("timestamp")?.as_str()?;
        let timestamp = DateTime::parse_from_rfc3339(timestamp_str)
            .ok()?
            .timestamp_millis();

        let date = chrono::Utc.timestamp_millis_opt(timestamp)
            .single()?
            .format("%Y-%m-%d")
            .to_string();

        let model = message.get("model")
            .and_then(|m| m.as_str())
            .filter(|s| !s.starts_with("<synthetic>"))
            .map(|s| s.to_string());

        let input_tokens = usage.get("input_tokens")?.as_i64()? as u64;
        let output_tokens = usage.get("output_tokens")?.as_i64()? as u64;
        let cache_creation_tokens = usage.get("cache_creation_input_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as u64;
        let cache_read_tokens = usage.get("cache_read_input_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as u64;

        Some(RawUsageEntry {
            timestamp,
            date,
            session_id: session_id.to_string(),
            project_path: project_path.to_string(),
            model,
            input_tokens,
            output_tokens,
            cache_creation_tokens,
            cache_read_tokens,
            extra_total_tokens: 0,
            cost: 0.0,
        })
    }

    /// Load entries from Codex session files (~/.codex/sessions/*.jsonl)
    async fn load_codex_jsonl_entries(&self) -> Vec<RawUsageEntry> {
        let mut entries = Vec::new();

        let Some(home) = dirs::home_dir() else {
            return entries;
        };

        let sessions_dir = home.join(".codex").join("sessions");
        if !sessions_dir.exists() {
            return entries;
        }

        self.load_jsonl_files_from_dir(&sessions_dir, "codex", &mut entries).await;
        entries
    }

    /// Load entries from OpenCode session files
    async fn load_opencode_jsonl_entries(&self) -> Vec<RawUsageEntry> {
        let mut entries = Vec::new();

        let Some(home) = dirs::home_dir() else {
            return entries;
        };

        // Try ~/.local/share/opencode first
        let opencode_dir = home.join(".local").join("share").join("opencode");
        if opencode_dir.exists() {
            self.load_jsonl_files_from_dir(&opencode_dir, "opencode", &mut entries).await;
        }

        // Also try ~/.opencode
        let opencode_dir2 = home.join(".opencode");
        if opencode_dir2.exists() {
            self.load_jsonl_files_from_dir(&opencode_dir2, "opencode", &mut entries).await;
        }

        entries
    }

    /// Load entries from Zhanlu session files (Issue #673)
    ///
    /// Zhanlu 的 session 存储路径与 opencode 类似但路径前缀不同：
    /// Issue 中给出的默认 session 路径是 `~/.local/share/zhanlu/storage`。
    /// 这里直接复用 `load_jsonl_files_from_dir` 的解析逻辑，因为 Zhanlu 与 Opencode
    /// 输出一致（Issue #673 明确要求）。
    async fn load_zhanlu_jsonl_entries(&self) -> Vec<RawUsageEntry> {
        let mut entries = Vec::new();

        let Some(home) = dirs::home_dir() else {
            return entries;
        };

        // Zhanlu 默认 session 路径: ~/.local/share/zhanlu/storage
        let zhanlu_dir = home.join(".local").join("share").join("zhanlu").join("storage");
        if zhanlu_dir.exists() {
            self.load_jsonl_files_from_dir(&zhanlu_dir, "zhanlu", &mut entries).await;
        }

        entries
    }

    /// Load entries from Kimi wire.jsonl files (~/.kimi/sessions/*/wire.jsonl)
    async fn load_kimi_jsonl_entries(&self) -> Vec<RawUsageEntry> {
        let mut entries = Vec::new();

        let Some(home) = dirs::home_dir() else {
            return entries;
        };

        let kimi_dir = home.join(".kimi").join("sessions");
        if !kimi_dir.exists() {
            return entries;
        }

        self.load_kimi_wire_files(&kimi_dir, &mut entries).await;
        entries
    }

    async fn load_kimi_wire_files(&self, dir: &PathBuf, entries: &mut Vec<RawUsageEntry>) {
        let mut stack = vec![dir.clone()];

        while let Some(current_dir) = stack.pop() {
            if let Ok(mut dir) = fs::read_dir(&current_dir).await {
                while let Ok(Some(dir_entry)) = dir.next_entry().await {
                    let path = dir_entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if path.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n == "wire.jsonl")
                        .unwrap_or(false)
                    {
                        self.parse_kimi_wire_file(&path, entries).await;
                    }
                }
            }
        }
    }

    async fn parse_kimi_wire_file(&self, path: &PathBuf, entries: &mut Vec<RawUsageEntry>) {
        let session_id = path.parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let project_path = path.parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let mut file = match fs::File::open(path).await {
            Ok(f) => f,
            Err(_) => return,
        };

        let mut contents = String::new();
        if file.read_to_string(&mut contents).await.is_err() {
            return;
        }

        for line in contents.lines() {
            if let Some(entry) = self.parse_kimi_wire_line(line, &session_id, &project_path) {
                entries.push(entry);
            }
        }
    }

    fn parse_kimi_wire_line(&self, line: &str, session_id: &str, project_path: &str) -> Option<RawUsageEntry> {
        let json: serde_json::Value = serde_json::from_str(line).ok()?;

        let timestamp = json.get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp_millis())
            .unwrap_or(0);

        let date = chrono::Utc.timestamp_millis_opt(timestamp)
            .single()?
            .format("%Y-%m-%d")
            .to_string();

        // Try to find usage data - format varies by editor
        let usage = json.get("usage")
            .or_else(|| json.get("data").and_then(|d| d.get("usage")))?;

        let input_tokens = usage.get("input_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as u64;
        let output_tokens = usage.get("output_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as u64;

        let model = json.get("model")
            .or_else(|| json.get("data").and_then(|d| d.get("model")))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Some(RawUsageEntry {
            timestamp,
            date,
            session_id: session_id.to_string(),
            project_path: project_path.to_string(),
            model,
            input_tokens,
            output_tokens,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            extra_total_tokens: 0,
            cost: 0.0,
        })
    }

    async fn load_jsonl_files_from_dir(&self, dir: &PathBuf, _editor_name: &str, entries: &mut Vec<RawUsageEntry>) {
        let mut stack = vec![dir.clone()];

        while let Some(current_dir) = stack.pop() {
            if let Ok(mut dir) = fs::read_dir(&current_dir).await {
                while let Ok(Some(dir_entry)) = dir.next_entry().await {
                    let path = dir_entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                        self.parse_generic_jsonl_file(&path, entries).await;
                    }
                }
            }
        }
    }

    async fn parse_generic_jsonl_file(&self, path: &PathBuf, entries: &mut Vec<RawUsageEntry>) {
        let session_id = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let project_path = path.parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let mut file = match fs::File::open(path).await {
            Ok(f) => f,
            Err(_) => return,
        };

        let mut contents = String::new();
        if file.read_to_string(&mut contents).await.is_err() {
            return;
        }

        for line in contents.lines() {
            if let Some(entry) = self.parse_generic_jsonl_line(line, &session_id, &project_path) {
                entries.push(entry);
            }
        }
    }

    fn parse_generic_jsonl_line(&self, line: &str, session_id: &str, project_path: &str) -> Option<RawUsageEntry> {
        let json: serde_json::Value = serde_json::from_str(line).ok()?;

        // Look for usage data - format varies by editor
        let usage = json.get("usage")
            .or_else(|| json.get("data").and_then(|d| d.get("usage")))?;

        // Get timestamp
        let timestamp = json.get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp_millis())
            .or_else(|| {
                json.get("created_at")
                    .and_then(|v| v.as_str())
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.timestamp_millis())
            })
            .unwrap_or(0);

        let date = chrono::Utc.timestamp_millis_opt(timestamp)
            .single()?
            .format("%Y-%m-%d")
            .to_string();

        let model = json.get("model")
            .or_else(|| json.get("data").and_then(|d| d.get("model")))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let input_tokens = usage.get("input_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as u64;
        let output_tokens = usage.get("output_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as u64;
        let cache_read_tokens = usage.get("cache_read_input_tokens")
            .or_else(|| usage.get("cached_input_tokens"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as u64;
        let cache_creation_tokens = usage.get("cache_creation_input_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as u64;

        Some(RawUsageEntry {
            timestamp,
            date,
            session_id: session_id.to_string(),
            project_path: project_path.to_string(),
            model,
            input_tokens,
            output_tokens,
            cache_creation_tokens,
            cache_read_tokens,
            extra_total_tokens: 0,
            cost: 0.0,
        })
    }

    /// Aggregate entries by day, returning both stats and per-model breakdowns
    pub(crate) fn aggregate_by_day(entries: &[RawUsageEntry]) -> (Vec<UsageStat>, Vec<ModelBreakdown>) {
        let mut daily_map: HashMap<String, TokenAccumulator> = HashMap::new();

        for entry in entries {
            let acc = daily_map.entry(entry.date.clone()).or_default();
            acc.add_entry(entry);
        }

        let mut stats = Vec::new();
        let mut all_breakdowns = Vec::new();

        for (date, acc) in daily_map {
            let models_used: Vec<String> = acc.models.keys().cloned().collect();
            let input_tokens = acc.input_tokens as i64;
            let output_tokens = acc.output_tokens as i64;
            let cache_creation_tokens = acc.cache_creation_tokens as i64;
            let cache_read_tokens = acc.cache_read_tokens as i64;
            let extra_total_tokens = acc.extra_total_tokens as i64;
            let total_cost = acc.cost;
            let breakdowns = acc.into_model_breakdowns();
            for mut bd in breakdowns {
                bd.date = date.clone();
                all_breakdowns.push(bd);
            }
            stats.push(UsageStat {
                date: date.clone(),
                input_tokens,
                output_tokens,
                cache_creation_tokens,
                cache_read_tokens,
                extra_total_tokens,
                total_cost,
                credits: None,
                message_count: None,
                models_used,
                project: None,
                last_activity: None,
                stats_type: "daily".to_string(),
            });
        }

        (stats, all_breakdowns)
    }

    /// Aggregate daily stats into weekly
    fn aggregate_by_week(daily: &[UsageStat]) -> Vec<UsageStat> {
        let mut weekly_map: HashMap<String, TokenAccumulator> = HashMap::new();

        for stat in daily {
            let week_start = Self::get_week_start(&stat.date);
            let acc = weekly_map.entry(week_start).or_default();
            acc.input_tokens += stat.input_tokens as u64;
            acc.output_tokens += stat.output_tokens as u64;
            acc.cache_creation_tokens += stat.cache_creation_tokens as u64;
            acc.cache_read_tokens += stat.cache_read_tokens as u64;
            acc.extra_total_tokens += stat.extra_total_tokens as u64;
            acc.cost += stat.total_cost;
        }

        weekly_map
            .into_iter()
            .map(|(date, acc)| {
                let models_used: Vec<String> = acc.models.keys().cloned().collect();
                UsageStat {
                    date,
                    input_tokens: acc.input_tokens as i64,
                    output_tokens: acc.output_tokens as i64,
                    cache_creation_tokens: acc.cache_creation_tokens as i64,
                    cache_read_tokens: acc.cache_read_tokens as i64,
                    extra_total_tokens: acc.extra_total_tokens as i64,
                    total_cost: acc.cost,
                    credits: None,
                    message_count: None,
                    models_used,
                    project: None,
                    last_activity: None,
                    stats_type: "weekly".to_string(),
                }
            })
            .collect()
    }

    /// Aggregate daily stats into monthly
    fn aggregate_by_month(daily: &[UsageStat]) -> Vec<UsageStat> {
        let mut monthly_map: HashMap<String, TokenAccumulator> = HashMap::new();

        for stat in daily {
            // Safely extract YYYY-MM from date string
            let month = if stat.date.len() >= 7 {
                stat.date[..7].to_string()
            } else {
                // Fallback to using the full date or a placeholder
                stat.date.clone()
            };
            let acc = monthly_map.entry(month).or_default();
            acc.input_tokens += stat.input_tokens as u64;
            acc.output_tokens += stat.output_tokens as u64;
            acc.cache_creation_tokens += stat.cache_creation_tokens as u64;
            acc.cache_read_tokens += stat.cache_read_tokens as u64;
            acc.extra_total_tokens += stat.extra_total_tokens as u64;
            acc.cost += stat.total_cost;
        }

        monthly_map
            .into_iter()
            .map(|(date, acc)| {
                let models_used: Vec<String> = acc.models.keys().cloned().collect();
                UsageStat {
                    date,
                    input_tokens: acc.input_tokens as i64,
                    output_tokens: acc.output_tokens as i64,
                    cache_creation_tokens: acc.cache_creation_tokens as i64,
                    cache_read_tokens: acc.cache_read_tokens as i64,
                    extra_total_tokens: acc.extra_total_tokens as i64,
                    total_cost: acc.cost,
                    credits: None,
                    message_count: None,
                    models_used,
                    project: None,
                    last_activity: None,
                    stats_type: "monthly".to_string(),
                }
            })
            .collect()
    }

    fn get_week_start(date: &str) -> String {
        if let Ok(naive_date) = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") {
            let weekday = naive_date.weekday();
            let days_since_monday = weekday.num_days_from_monday();
            let monday = naive_date - chrono::Duration::days(days_since_monday as i64);
            monday.format("%Y-%m-%d").to_string()
        } else {
            date.to_string()
        }
    }

    /// Save aggregated stats to database
    pub async fn save_daily_stats(&self, stats: &[UsageStat], breakdowns: &[ModelBreakdown]) -> Result<(), String> {
        for stat in stats {
            // Check if we already have this date's stats
            let existing = self.db.get_latest_usage_stat(&stat.date, "daily").await
                .map_err(|e| e.to_string())?;

            if existing.is_some() {
                // Delete and re-insert (update)
                self.db.delete_usage_stats_by_date(&stat.date, "daily").await
                    .map_err(|e| e.to_string())?;
            }

            // Insert new stats
            let stat_id = self.db.create_usage_daily_stat(
                &stat.date,
                stat.project.as_deref(),
                None,
                stat.input_tokens,
                stat.output_tokens,
                stat.cache_creation_tokens,
                stat.cache_read_tokens,
                stat.extra_total_tokens,
                stat.total_cost,
                stat.credits,
                stat.message_count,
                &stat.models_used,
                stat.project.as_deref(),
                None,
                stat.last_activity.as_deref(),
                "daily",
            ).await.map_err(|e| e.to_string())?;

            // Insert model breakdowns for this date
            for bd in breakdowns.iter().filter(|b| b.date == stat.date) {
                self.db.create_usage_model_breakdown(
                    stat_id,
                    &bd.model_name,
                    bd.input_tokens,
                    bd.output_tokens,
                    bd.cache_creation_tokens,
                    bd.cache_read_tokens,
                    bd.extra_total_tokens,
                    bd.cost,
                ).await.map_err(|e| e.to_string())?;
            }
        }

        Ok(())
    }

    /// Generate and store today's real-time stats
    pub async fn update_today_stats(&self) -> Result<UsageReport, String> {
        let entries = self.collect_all_entries().await;

        if entries.is_empty() {
            return Ok(UsageReport {
                daily: vec![],
                weekly: vec![],
                monthly: vec![],
            });
        }

        // Get today's date
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        // Filter entries for today
        let today_entries: Vec<_> = entries.iter()
            .filter(|e| e.date == today)
            .cloned()
            .collect();

        // Aggregate today's entries
        let (daily, breakdowns) = if today_entries.is_empty() {
            (vec![], vec![])
        } else {
            let mut acc = TokenAccumulator::default();
            for entry in &today_entries {
                acc.add_entry(entry);
            }
            let models_used: Vec<String> = acc.models.keys().cloned().collect();
            let input_tokens = acc.input_tokens as i64;
            let output_tokens = acc.output_tokens as i64;
            let cache_creation_tokens = acc.cache_creation_tokens as i64;
            let cache_read_tokens = acc.cache_read_tokens as i64;
            let extra_total_tokens = acc.extra_total_tokens as i64;
            let total_cost = acc.cost;
            let mut bds = acc.into_model_breakdowns();
            for bd in &mut bds {
                bd.date = today.clone();
            }
            let stats = vec![UsageStat {
                date: today.clone(),
                input_tokens,
                output_tokens,
                cache_creation_tokens,
                cache_read_tokens,
                extra_total_tokens,
                total_cost,
                credits: None,
                message_count: None,
                models_used,
                project: None,
                last_activity: None,
                stats_type: "daily".to_string(),
            }];
            (stats, bds)
        };

        // Save today's stats
        if !daily.is_empty() {
            self.save_daily_stats(&daily, &breakdowns).await?;
        }

        Ok(UsageReport {
            daily,
            weekly: vec![],
            monthly: vec![],
        })
    }

    /// Refresh all usage statistics
    pub async fn refresh_all_stats(&self) -> Result<UsageReport, String> {
        // First, archive yesterday's stats
        self.archive_yesterday_stats().await?;

        // Then update today's stats
        self.update_today_stats().await?;

        // Return all stats from database
        let daily = self.get_stats("daily", None, None).await?;
        let weekly = self.get_stats("weekly", None, None).await?;
        let monthly = self.get_stats("monthly", None, None).await?;

        Ok(UsageReport {
            daily,
            weekly,
            monthly,
        })
    }

    /// Archive yesterday's usage stats
    pub async fn archive_yesterday_stats(&self) -> Result<(), String> {
        let entries = self.collect_all_entries().await;

        if entries.is_empty() {
            return Ok(());
        }

        // Get yesterday's date
        let yesterday = (chrono::Local::now() - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();

        // Filter entries for yesterday
        let yesterday_entries: Vec<_> = entries.iter()
            .filter(|e| e.date == yesterday)
            .cloned()
            .collect();

        if yesterday_entries.is_empty() {
            return Ok(());
        }

        // Aggregate by day
        let (daily_stats, breakdowns) = Self::aggregate_by_day(&yesterday_entries);

        // Save to database
        self.save_daily_stats(&daily_stats, &breakdowns).await?;

        Ok(())
    }

    /// Get historical stats from database
    pub async fn get_stats(
        &self,
        stats_type: &str,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<Vec<UsageStat>, String> {
        // For weekly/monthly, compute from daily data stored in DB
        if stats_type == "weekly" || stats_type == "monthly" {
            let daily_models = self.db.get_usage_stats("daily", since, until)
                .await
                .map_err(|e| e.to_string())?;

            if daily_models.is_empty() {
                return Ok(vec![]);
            }

            let daily: Vec<UsageStat> = daily_models.into_iter().map(|m| UsageStat {
                date: m.date,
                input_tokens: m.input_tokens,
                output_tokens: m.output_tokens,
                cache_creation_tokens: m.cache_creation_tokens,
                cache_read_tokens: m.cache_read_tokens,
                extra_total_tokens: m.extra_total_tokens,
                total_cost: m.total_cost,
                credits: m.credits,
                message_count: m.message_count,
                models_used: serde_json::from_str(&m.models_used).unwrap_or_default(),
                project: m.project,
                last_activity: m.last_activity,
                stats_type: m.stats_type,
            }).collect();

            return if stats_type == "weekly" {
                Ok(Self::aggregate_by_week(&daily))
            } else {
                Ok(Self::aggregate_by_month(&daily))
            };
        }

        // For daily, query directly from database
        let models = self.db.get_usage_stats(stats_type, since, until)
            .await
            .map_err(|e| e.to_string())?;

        Ok(models.into_iter().map(|m| UsageStat {
            date: m.date,
            input_tokens: m.input_tokens,
            output_tokens: m.output_tokens,
            cache_creation_tokens: m.cache_creation_tokens,
            cache_read_tokens: m.cache_read_tokens,
            extra_total_tokens: m.extra_total_tokens,
            total_cost: m.total_cost,
            credits: m.credits,
            message_count: m.message_count,
            models_used: serde_json::from_str(&m.models_used).unwrap_or_default(),
            project: m.project,
            last_activity: m.last_activity,
            stats_type: m.stats_type,
        }).collect())
    }
}

#[cfg(test)]
mod tests {
    //! 回归测试：Zhanlu (Issue #673) jsonl 走 `parse_generic_jsonl_line` 必须正确解析
    //! `usage` / `timestamp` / `model` / cache token 字段，与 Opencode 行为一致。
    //!
    //! 该测试针对 PR #677 review H2 —— 之前 zhanlu jsonl 复用 opencode 解析器但
    //! 没有专门的回归测试，运行时才能发现字段名是否真的 byte-for-byte 一致。
    use super::*;
    use std::sync::Arc;

    /// 构造一个 UsageStatsService 实例用于测试其 impl 方法。
    /// `parse_generic_jsonl_line` 是 &self 方法但不触碰 self.db，所以传任意 Database 即可。
    async fn make_service() -> UsageStatsService {
        let db = Arc::new(
            crate::db::Database::new(":memory:")
                .await
                .expect("内存 db 必须能创建"),
        );
        UsageStatsService::new(db)
    }

    /// Zhanlu (与 Opencode 一致) 风格 jsonl：顶层 `usage` + `timestamp` + `model`。
    /// 走 parse_generic_jsonl_line 后应填入对应字段。
    #[tokio::test]
    async fn test_parse_zhanlu_jsonl_step_with_usage() {
        let svc = make_service().await;
        let line = r#"{
            "type": "step_start",
            "sessionID": "ses_zhanlu_001",
            "timestamp": "2026-06-18T12:34:56Z",
            "model": "zhanlu-test-model",
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50,
                "cache_read_input_tokens": 20,
                "cache_creation_input_tokens": 10
            }
        }"#;
        let entry = svc
            .parse_generic_jsonl_line(line, "ses_zhanlu_001", "/tmp/zhanlu")
            .expect("zhanlu jsonl 应被 parse");

        assert_eq!(entry.session_id, "ses_zhanlu_001");
        assert_eq!(entry.project_path, "/tmp/zhanlu");
        assert_eq!(entry.model.as_deref(), Some("zhanlu-test-model"));
        assert_eq!(entry.input_tokens, 100);
        assert_eq!(entry.output_tokens, 50);
        assert_eq!(entry.cache_read_tokens, 20);
        assert_eq!(entry.cache_creation_tokens, 10);
        assert_eq!(entry.date, "2026-06-18");
    }

    /// Zhanlu 把 usage 嵌进 `data.usage`（与 Opencode 嵌套形态一致）也能被解析。
    /// 这是 parse_generic_jsonl_line 的 fallback 分支，必须能正确回退。
    #[tokio::test]
    async fn test_parse_zhanlu_jsonl_nested_data_usage() {
        let svc = make_service().await;
        let line = r#"{
            "type": "step_finish",
            "data": {
                "usage": {
                    "input_tokens": 200,
                    "output_tokens": 80,
                    "cache_creation_input_tokens": 5
                },
                "model": "nested-zhanlu-model"
            },
            "created_at": "2026-06-18T13:00:00Z"
        }"#;
        let entry = svc
            .parse_generic_jsonl_line(line, "ses_zhanlu_002", "/tmp/zhanlu")
            .expect("嵌套 data.usage 应被 parse");

        assert_eq!(entry.input_tokens, 200);
        assert_eq!(entry.output_tokens, 80);
        assert_eq!(entry.cache_creation_tokens, 5);
        assert_eq!(entry.cache_read_tokens, 0); // 字段缺失 → 0
        assert_eq!(entry.model.as_deref(), Some("nested-zhanlu-model"));
        assert_eq!(entry.date, "2026-06-18");
    }

    /// 没有 usage 字段的行（如纯文本 / metadata）应返回 None，不污染 entries。
    #[tokio::test]
    async fn test_parse_zhanlu_jsonl_skips_non_usage_lines() {
        let svc = make_service().await;
        let line = r#"{"type": "text", "text": "hello"}"#;
        assert!(svc
            .parse_generic_jsonl_line(line, "ses_zhanlu_003", "/tmp/zhanlu")
            .is_none());
    }
}
