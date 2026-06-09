use axum::extract::{Query, State};
use serde::Deserialize;
use std::str::FromStr;

use super::{AppError, AppState};
use crate::models::ApiResponse;
use crate::services::usage_stats::{ModelBreakdown, UsageReport, UsageStat, UsageStatsService};

#[derive(Debug, Deserialize)]
pub struct UsageStatsQuery {
    pub since: Option<String>,
    pub until: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct UsageStatsResponse {
    pub daily: Vec<UsageStat>,
    pub weekly: Vec<UsageStat>,
    pub monthly: Vec<UsageStat>,
    pub breakdowns: Vec<ModelBreakdown>,
}

impl From<UsageReport> for UsageStatsResponse {
    fn from(report: UsageReport) -> Self {
        Self {
            daily: report.daily,
            weekly: report.weekly,
            monthly: report.monthly,
            breakdowns: vec![],
        }
    }
}

pub async fn get_usage_stats(
    State(state): State<AppState>,
    Query(query): Query<UsageStatsQuery>,
) -> Result<ApiResponse<UsageStatsResponse>, AppError> {
    let service = UsageStatsService::new(state.db.clone());

    let daily = service
        .get_stats("daily", query.since.as_deref(), query.until.as_deref())
        .await
        .map_err(|e| AppError::Internal(e))?;

    let weekly = service
        .get_stats("weekly", query.since.as_deref(), query.until.as_deref())
        .await
        .map_err(|e| AppError::Internal(e))?;

    let monthly = service
        .get_stats("monthly", query.since.as_deref(), query.until.as_deref())
        .await
        .map_err(|e| AppError::Internal(e))?;

    // Get model breakdowns from database
    let db_breakdowns = state.db
        .get_usage_model_breakdowns_by_date_range("daily", query.since.as_deref(), query.until.as_deref())
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let breakdowns: Vec<ModelBreakdown> = db_breakdowns
        .into_iter()
        .map(|bd| ModelBreakdown {
            date: bd.date,
            model_name: bd.model_name,
            input_tokens: bd.input_tokens,
            output_tokens: bd.output_tokens,
            cache_creation_tokens: bd.cache_creation_tokens,
            cache_read_tokens: bd.cache_read_tokens,
            extra_total_tokens: bd.extra_total_tokens,
            cost: bd.cost,
        })
        .collect();

    Ok(ApiResponse::ok(UsageStatsResponse {
        daily,
        weekly,
        monthly,
        breakdowns,
    }))
}

pub async fn refresh_usage_stats(
    State(state): State<AppState>,
) -> Result<ApiResponse<UsageStatsResponse>, AppError> {
    let service = UsageStatsService::new(state.db.clone());

    let report = service
        .refresh_all_stats()
        .await
        .map_err(|e| AppError::Internal(e))?;

    Ok(ApiResponse::ok(report.into()))
}

#[derive(Debug, serde::Serialize)]
pub struct UsageStatsSettings {
    pub auto_usage_stats_enabled: bool,
    pub auto_usage_stats_cron: String,
}

pub async fn get_usage_stats_settings(
    State(state): State<AppState>,
) -> Result<ApiResponse<UsageStatsSettings>, AppError> {
    let cfg = state.config.read().await;
    Ok(ApiResponse::ok(UsageStatsSettings {
        auto_usage_stats_enabled: cfg.auto_usage_stats_enabled,
        auto_usage_stats_cron: cfg.auto_usage_stats_cron.clone(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct UpdateUsageStatsSettingsRequest {
    pub enabled: bool,
    pub cron: String,
}

pub async fn update_usage_stats_settings(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<UpdateUsageStatsSettingsRequest>,
) -> Result<ApiResponse<String>, AppError> {
    // Validate cron expression
    if req.enabled {
        let schedule = cron::Schedule::from_str(&req.cron)
            .map_err(|e| AppError::BadRequest(format!("Invalid cron expression: {}", e)))?;
        schedule.upcoming(chrono::Utc).next()
            .ok_or_else(|| AppError::BadRequest("Cron expression has no future executions".to_string()))?;
    }

    let mut cfg = state.config.write().await;
    cfg.auto_usage_stats_enabled = req.enabled;
    cfg.auto_usage_stats_cron = req.cron;
    cfg.normalize_paths();
    cfg.clamp_execution_timeout_secs();

    let cfg_clone = cfg.clone();
    tokio::task::spawn_blocking(move || cfg_clone.save())
        .await
        .map_err(|e| AppError::Internal(format!("Join error: {}", e)))?
        .map_err(|e| AppError::Internal(format!("Failed to save config: {}", e)))?;

    Ok(ApiResponse::ok("AI 使用统计配置已更新".to_string()))
}
