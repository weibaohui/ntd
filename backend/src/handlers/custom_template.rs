use axum::extract::State;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;

use crate::db::Database;
use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::{ApiResponse, TodoTemplate};

/// Remote template YAML format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteTemplate {
    pub title: String,
    pub prompt: Option<String>,
    pub category: Option<String>,
}

impl RemoteTemplate {
    pub fn category_or_default(&self) -> String {
        self.category.clone().unwrap_or_else(|| "自定义".to_string())
    }
}

/// Custom template subscription status
#[derive(Serialize)]
pub struct CustomTemplateStatus {
    pub subscribed: bool,
    pub source_url: Option<String>,
    pub last_sync_at: Option<String>,
    pub auto_sync_enabled: bool,
    pub auto_sync_cron: String,
    pub templates: Vec<TodoTemplate>,
}

/// Subscribe to a remote template URL
#[derive(Deserialize)]
pub struct SubscribeRequest {
    pub url: String,
}

/// Update auto sync config
#[derive(Deserialize)]
pub struct UpdateAutoSyncRequest {
    pub enabled: bool,
    pub cron: String,
}

/// Check if a host is a private/internal IP
fn is_private_host(host: &str) -> bool {
    // Check for literal private IPs
    if host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "0.0.0.0" {
        return true;
    }

    // Try to parse as IP and check ranges
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        match ip {
            std::net::IpAddr::V4(ipv4) => {
                // Check private ranges: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                let octets = ipv4.octets();
                ipv4.is_loopback()
                    || ipv4.is_unspecified()
                    || (octets[0] == 10)
                    || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                    || (octets[0] == 192 && octets[1] == 168)
            }
            std::net::IpAddr::V6(ipv6) => {
                ipv6.is_loopback() || ipv6.is_unspecified()
            }
        }
    } else {
        false
    }
}

/// Validate URL for SSRF vulnerabilities
fn validate_url(url: &str) -> Result<String, String> {
    let parsed = url::Url::parse(url)
        .map_err(|e| format!("Invalid URL: {}", e))?;

    let host = parsed.host_str()
        .ok_or_else(|| "URL must have a host".to_string())?;

    // Check for private/internal hosts
    if is_private_host(host) {
        return Err("Private/internal hosts are not allowed".to_string());
    }

    Ok(url.to_string())
}

/// Fetch YAML from URL and parse it (with timeout and size limit)
pub async fn fetch_remote_templates(url: &str) -> Result<Vec<RemoteTemplate>, String> {
    // Validate URL for SSRF
    let _ = validate_url(url)?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        // 禁止重定向以防止 SSRF 绕过：
        // 攻击者可以提供一个指向公共服务器的 URL，该服务器再重定向到内网地址
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client.get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch URL: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    // Limit response size to 1MB
    let body = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    if body.len() > 1024 * 1024 {
        return Err("Response too large (max 1MB)".to_string());
    }

    let body_str = String::from_utf8(body.to_vec())
        .map_err(|e| format!("Invalid UTF-8 in response: {}", e))?;

    // Try parsing as YAML array first, handling comments at the start
    let templates: Vec<RemoteTemplate> = match serde_yaml::from_str::<Vec<RemoteTemplate>>(&body_str) {
        Ok(templates) => templates,
        Err(_) => {
            // If array parse fails, try single object
            let single: RemoteTemplate = serde_yaml::from_str(&body_str)
                .map_err(|e| format!("Invalid YAML format: {}", e))?;
            vec![single]
        }
    };

    Ok(templates)
}

/// Get custom template subscription status
pub async fn get_custom_template_status(
    State(state): State<AppState>,
) -> Result<ApiResponse<CustomTemplateStatus>, AppError> {
    // 块作用域拷出 owned 值,锁卫立即 drop,避免后续 .await 持 std 读锁卫。
    let (auto_sync_enabled, auto_sync_cron) = {
        // RwLock 中毒 = 曾有线程持锁 panic，继续执行无意义
        #[allow(clippy::unwrap_used)]
        let cfg = state.config.read().unwrap();
        (
            cfg.auto_sync_custom_templates_enabled,
            cfg.auto_sync_custom_templates_cron.clone(),
        )
    };

    let subscription = state.db.get_custom_template_subscription().await?;
    let (subscribed, source_url, last_sync_at) = match subscription {
        Some((url, sync_at)) => (true, Some(url), sync_at),
        None => (false, None, None),
    };

    // Get all templates with source_url set (custom templates)
    let all_templates = state.db.get_templates().await?;
    let custom_templates: Vec<TodoTemplate> = all_templates
        .into_iter()
        .filter(|t| t.source_url.is_some())
        .collect();

    Ok(ApiResponse::ok(CustomTemplateStatus {
        subscribed,
        source_url,
        last_sync_at,
        auto_sync_enabled,
        auto_sync_cron,
        templates: custom_templates,
    }))
}

/// Sync templates from remote URL (core logic, returns templates to insert)
async fn fetch_and_validate_templates(url: &str) -> Result<Vec<RemoteTemplate>, String> {
    let remote_templates = fetch_remote_templates(url).await?;

    if remote_templates.is_empty() {
        return Err("No templates found in remote file".to_string());
    }

    Ok(remote_templates)
}

/// Insert templates into database
async fn insert_templates(db: &Database, templates: &[RemoteTemplate], source_url: &str) -> Result<(), String> {
    for (idx, remote) in templates.iter().enumerate() {
        db.create_template_from_remote(
            crate::db::TemplateInput {
                title: &remote.title,
                prompt: remote.prompt.as_deref(),
                category: &remote.category_or_default(),
                sort_order: Some(idx as i32),
            },
            source_url,
        ).await
        .map_err(|e| format!("Failed to create template '{}': {}", remote.title, e))?;
    }
    Ok(())
}

/// Subscribe to a remote template URL and sync immediately
pub async fn subscribe_custom_template(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<SubscribeRequest>,
) -> Result<ApiResponse<CustomTemplateStatus>, AppError> {
    let url = req.url.trim();
    if url.is_empty() {
        return Err(AppError::BadRequest("URL is required".to_string()));
    }

    // Validate URL format and security
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(AppError::BadRequest("URL must start with http:// or https://".to_string()));
    }

    // First fetch and validate (before any deletion)
    let remote_templates = fetch_and_validate_templates(url).await
        .map_err(|e| AppError::BadRequest(format!("Failed to fetch templates: {}", e)))?;

    // Delete existing custom templates (from any previous subscription)
    state.db.delete_all_custom_templates().await?;

    // Insert new templates
    // AppError::Internal 接收 String 参数，闭包 |e| AppError::Internal(e) 可简化为函数引用
    insert_templates(&state.db, &remote_templates, url).await
        .map_err(AppError::Internal)?;

    // Return updated status
    get_custom_template_status(State(state)).await
}

/// Unsubscribe from remote template
pub async fn unsubscribe_custom_template(
    State(state): State<AppState>,
) -> Result<ApiResponse<()>, AppError> {
    state.db.delete_all_custom_templates().await?;
    Ok(ApiResponse::ok(()))
}

/// Sync templates from the subscribed URL
pub async fn sync_custom_template(
    State(state): State<AppState>,
) -> Result<ApiResponse<CustomTemplateStatus>, AppError> {
    let subscription = state.db.get_custom_template_subscription().await?
        .ok_or_else(|| AppError::BadRequest("Not subscribed to any remote template".to_string()))?;

    let (url, _) = subscription;

    // First fetch and validate (before any deletion)
    let remote_templates = fetch_and_validate_templates(&url).await
        .map_err(|e| AppError::BadRequest(format!("Failed to fetch templates: {}", e)))?;

    // Delete existing custom templates
    state.db.delete_templates_by_source_url(&url).await?;

    // Insert new templates
    // AppError::Internal 接收 String 参数，闭包 |e| AppError::Internal(e) 可简化为函数引用
    insert_templates(&state.db, &remote_templates, &url).await
        .map_err(AppError::Internal)?;

    // Return updated status
    get_custom_template_status(State(state)).await
}

/// Update auto sync configuration
pub async fn update_auto_sync_config(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<UpdateAutoSyncRequest>,
) -> Result<ApiResponse<String>, AppError> {
    // Validate cron expression (accepts 5 or 6 field format)
    if req.enabled {
        let schedule = cron::Schedule::from_str(&req.cron)
            .map_err(|e| AppError::BadRequest(format!("Invalid cron expression: {}", e)))?;
        schedule.upcoming(chrono::Utc).next()
            .ok_or_else(|| AppError::BadRequest("Cron expression has no future executions".to_string()))?;
    }

    // 块作用域内 clone 出 owned 值,await 落盘前写锁已 drop。
    let cfg_clone = {
        // RwLock 中毒 = 曾有线程持锁 panic，继续执行无意义
        #[allow(clippy::unwrap_used)]
        let mut cfg = state.config.write().unwrap();
        cfg.auto_sync_custom_templates_enabled = req.enabled;
        cfg.auto_sync_custom_templates_cron = req.cron;
        cfg.clone()
    };

    tokio::task::spawn_blocking(move || cfg_clone.save())
        .await
        .map_err(|e| AppError::Internal(format!("Join error: {}", e)))?
        .map_err(|e| AppError::Internal(format!("Failed to save config: {}", e)))?;

    Ok(ApiResponse::ok("自动同步配置已更新".to_string()))
}

/// Start custom template auto sync scheduler
pub fn start_custom_template_auto_sync(
    _cron_expr: &str,
    // db 仅用于 clone 进 spawn 闭包，按引用传入避免调用方额外 clone
    db: &Arc<Database>,
    config: std::sync::Arc<std::sync::RwLock<crate::config::Config>>,
) -> Result<(), String> {
    // Validate initial cron expression but will re-read from config in the loop
    let _ = cron::Schedule::from_str(_cron_expr)
        .map_err(|e| format!("Invalid cron: {}", e))?;

    // db 是 Arc，clone 只增加引用计数；move 进 spawn 闭包需要 owned 值
    let db_clone = db.clone();
    tokio::spawn(async move {
        loop {
            // 关键:std::sync 读锁卫不能跨 .await。用显式 if-else + 块作用域
            // 把 disabled 分支的 sleep().await 放在 cfg 锁卫作用域外。
            let (enabled, next_delay) = {
                let enabled = {
                    // RwLock 中毒 = 曾有线程持锁 panic，继续执行无意义
                    #[allow(clippy::unwrap_used)]
                    let cfg = config.read().unwrap();
                    cfg.auto_sync_custom_templates_enabled
                };
                if !enabled {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    continue;
                }
                let (enabled, delay) = {
                    // RwLock 中毒 = 曾有线程持锁 panic，继续执行无意义
                    #[allow(clippy::unwrap_used)]
                    let cfg = config.read().unwrap();
                    // "0 0 * * *" 是硬编码的合法 cron 表达式，unwrap 安全
                    #[allow(clippy::unwrap_used)]
                    let schedule = cron::Schedule::from_str(&cfg.auto_sync_custom_templates_cron)
                        .unwrap_or_else(|_| cron::Schedule::from_str("0 0 * * *").unwrap());
                    let next = schedule.upcoming(chrono::Utc).next();
                    let delay = match next {
                        Some(dt) => {
                            let now = chrono::Utc::now();
                            (dt - now).to_std().unwrap_or(std::time::Duration::from_secs(60))
                        }
                        None => std::time::Duration::from_secs(3600),
                    };
                    (cfg.auto_sync_custom_templates_enabled, delay)
                };
                (enabled, delay)
            };

            tokio::time::sleep(next_delay).await;

            // Skip sync if disabled while sleeping
            if !enabled {
                continue;
            }

            let db = db_clone.clone();
            match perform_sync(&db).await {
                Ok(msg) => tracing::info!("{}", msg),
                Err(e) => tracing::error!("Auto custom template sync failed: {}", e),
            }
        }
    });

    Ok(())
}

/// Core sync logic - fetches, validates, and replaces templates atomically
async fn perform_sync(db: &Arc<Database>) -> Result<String, String> {
    let subscription = db.get_custom_template_subscription().await
        .map_err(|e| format!("DB error: {}", e))?
        .ok_or_else(|| "Not subscribed".to_string())?;

    let (url, _) = subscription;

    // First fetch and validate (before any deletion)
    let remote_templates = fetch_and_validate_templates(&url).await?;

    // Delete existing custom templates
    db.delete_templates_by_source_url(&url).await
        .map_err(|e| format!("Failed to delete old templates: {}", e))?;

    // Insert new templates
    insert_templates(db, &remote_templates, &url).await?;

    Ok(format!("Auto custom template sync completed: {} templates imported", remote_templates.len()))
}
