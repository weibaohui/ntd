//! Provider 池 + Profile 管理 API Handlers。
//!
//! # 路由
//!
//! | 方法 | 路径 | 说明 |
//! |------|------|------|
//! | GET | `/api/v1/providers` | Provider 列表 |
//! | GET | `/api/v1/providers/{name}` | Provider 详情（含 api_key） |
//! | POST | `/api/v1/providers` | 创建 Provider |
//! | PUT | `/api/v1/providers/{name}` | 更新 Provider |
//! | DELETE | `/api/v1/providers/{name}` | 删除 Provider |
//! | GET | `/api/v1/profiles` | Profile 列表 |
//! | GET | `/api/v1/profiles/current` | 当前 Profile 详情 |
//! | POST | `/api/v1/profiles` | 创建 Profile |
//! | PUT | `/api/v1/profiles/{name}` | 更新 Profile |
//! | DELETE | `/api/v1/profiles/{name}` | 删除 Profile |
//! | POST | `/api/v1/profiles/{name}/apply` | 应用（切换）Profile |

use axum::{
    Router,
    extract::State,
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};

use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::ApiResponse;
use crate::profiles::{
    ApplyProfileResult, CreateProfileRequest, CreateProviderRequest, ExecutorProfile,
    ExecutorRef, ProfileSummary, ProfilesConfig, Provider, ProviderDetail, ProviderSummary,
    UpdateProfileRequest, UpdateProviderRequest,
};
use crate::profiles::generators::{ExecutorConfigDef, resolve_provider, ProfileGeneratorRegistry, all_executor_configs};
// ============================================================================
// 路由
// ============================================================================

pub fn profile_routes() -> Router<AppState> {
    Router::new()
        // Provider CRUD
        .route("/api/v1/providers", get(list_providers).post(create_provider))
        .route("/api/v1/providers/supported-executors", get(list_executor_configs))
        .route("/api/v1/providers/{name}", get(get_provider).put(update_provider).delete(delete_provider))
        .route("/api/v1/providers/{name}/preview", post(preview_provider_to_executors))
        .route("/api/v1/providers/{name}/apply", post(apply_provider_to_executors))
        // Profile CRUD + apply
        .route("/api/v1/profiles", get(list_profiles).post(create_profile))
        .route("/api/v1/profiles/current", get(get_current_profile))
        .route("/api/v1/profiles/{name}", put(update_profile).delete(delete_profile))
        .route("/api/v1/profiles/{name}/apply", post(apply_profile))
}

// ============================================================================
// Provider Handlers
// ============================================================================

async fn list_providers(
    State(_state): State<AppState>,
) -> Result<ApiResponse<Vec<ProviderSummary>>, AppError> {
    let cfg = load()?;
    let summaries: Vec<ProviderSummary> = cfg.providers.into_iter().map(|(name, p)| ProviderSummary {
        name,
        display_name: p.name,
        base_url: p.base_url,
        protocol: p.protocol,
        model_count: p.models.len(),
    }).collect();
    Ok(ApiResponse::ok(summaries))
}

async fn create_provider(
    State(_state): State<AppState>,
    ApiJson(req): ApiJson<CreateProviderRequest>,
) -> Result<ApiResponse<ProviderSummary>, AppError> {
    validate_profile_name(&req.name)?;
    let mut cfg = load()?;
    if cfg.providers.contains_key(&req.name) {
        return Err(AppError::BadRequest(format!("Provider '{}' already exists", req.name)));
    }
    let protocol = req.protocol;
    let base_url = req.base_url.clone();
    let provider = Provider {
        name: req.display_name,
        api_key: req.api_key,
        base_url: req.base_url,
        protocol,
        models: req.models,
    };
    let model_count = provider.models.len();
    let name = req.name;
    let display_name = provider.name.clone();
    cfg.providers.insert(name.clone(), provider);
    save(&cfg)?;
    Ok(ApiResponse::ok(ProviderSummary {
        name,
        display_name,
        base_url,
        protocol,
        model_count,
    }))
}

/// 获取所有执行器的配置定义（配置文件路径、是否有生成器）。
async fn list_executor_configs(
    State(_state): State<AppState>,
) -> Result<ApiResponse<Vec<ExecutorConfigDef>>, AppError> {
    Ok(ApiResponse::ok(all_executor_configs()))
}

async fn get_provider(
    State(_state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<ApiResponse<ProviderDetail>, AppError> {
    let cfg = load()?;
    let p = cfg.providers.get(&name).ok_or(AppError::NotFound)?;
    Ok(ApiResponse::ok(ProviderDetail {
        name: name.clone(),
        display_name: p.name.clone(),
        api_key: p.api_key.clone(),
        base_url: p.base_url.clone(),
        protocol: p.protocol,
        models: p.models.clone(),
    }))
}

async fn update_provider(
    State(_state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    ApiJson(req): ApiJson<UpdateProviderRequest>,
) -> Result<ApiResponse<ProviderSummary>, AppError> {
    let mut cfg = load()?;
    let provider = cfg.providers.get_mut(&name).ok_or(AppError::NotFound)?;
    if let Some(dn) = req.display_name { provider.name = dn; }
    if let Some(k) = req.api_key { provider.api_key = k; }
    if let Some(u) = req.base_url { provider.base_url = u; }
    if let Some(p) = req.protocol { provider.protocol = p; }
    if let Some(m) = req.models { provider.models = m; }
    let model_count = provider.models.len();
    let summary = ProviderSummary {
        name: name.clone(),
        display_name: provider.name.clone(),
        base_url: provider.base_url.clone(),
        protocol: provider.protocol,
        model_count,
    };
    save(&cfg)?;
    Ok(ApiResponse::ok(summary))
}

async fn delete_provider(
    State(_state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    let mut cfg = load()?;
    if cfg.providers.remove(&name).is_none() {
        return Err(AppError::NotFound);
    }
    save(&cfg)?;
    Ok(ApiResponse::ok(()))
}

/// 预览 Provider 应用到指定执行器的效果（不写盘）。
async fn preview_provider_to_executors(
    State(_state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    ApiJson(req): ApiJson<ApplyProviderRequest>,
) -> Result<ApiResponse<Vec<PreviewEntry>>, AppError> {
    let cfg = load()?;
    let provider = cfg.providers.get(&name).ok_or(AppError::NotFound)?;
    let registry = ProfileGeneratorRegistry::new();
    let mut entries = Vec::new();

    for (exec_name, model_name) in &req.executor_models {
        if let Some(generator) = registry.get(exec_name) {
            let exec_ref = crate::profiles::ExecutorRef { provider: name.clone(), model: model_name.clone() };
            let session_dir = crate::adapters::find_executor(exec_name).map(|def| def.session_dir).unwrap_or("");
            match generator.preview(&exec_ref, provider, session_dir) {
                Ok((path, content)) => entries.push(PreviewEntry { executor: exec_name.clone(), model: model_name.clone(), path, content }),
                Err(e) => entries.push(PreviewEntry { executor: exec_name.clone(), model: model_name.clone(), path: String::new(), content: format!("Error: {}", e) }),
            }
        }
    }

    Ok(ApiResponse::ok(entries))
}

/// 预览条目（含选中的模型）。
#[derive(Debug, Clone, Serialize)]
struct PreviewEntry {
    executor: String,
    model: String,
    path: String,
    content: String,
}

/// 应用 Provider 到指定执行器。
async fn apply_provider_to_executors(
    State(_state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    ApiJson(req): ApiJson<ApplyProviderRequest>,
) -> Result<ApiResponse<ApplyProviderResult>, AppError> {
    let cfg = load()?;
    let provider = cfg.providers.get(&name).ok_or(AppError::NotFound)?;
    let registry = crate::profiles::generators::ProfileGeneratorRegistry::new();
    let mut applied = Vec::new();
    let mut errors = Vec::new();

    for (exec_name, model_name) in &req.executor_models {
        if let Some(generator) = registry.get(exec_name) {
            let exec_ref = crate::profiles::ExecutorRef {
                provider: name.clone(),
                model: model_name.clone(),
            };
            let session_dir = crate::adapters::find_executor(exec_name)
                .map(|def| def.session_dir)
                .unwrap_or("");
            match generator.generate(&exec_ref, provider, session_dir) {
                Ok(()) => applied.push(format!("{} ({})", exec_name, model_name)),
                Err(e) => errors.push(format!("{}: {}", exec_name, e)),
            }
        } else {
            errors.push(format!("{}: no config generator available", exec_name));
        }
    }

    Ok(ApiResponse::ok(ApplyProviderResult { applied, errors }))
}

/// 应用 Provider 到指定执行器的响应。
#[derive(Debug, Clone, Serialize)]
struct ApplyProviderResult {
    applied: Vec<String>,
    errors: Vec<String>,
}

/// 应用 Provider 到执行器的请求体。
/// `executor_models` 的 key 为执行器名，value 为要使用的模型名。
#[derive(Debug, Clone, Deserialize)]
struct ApplyProviderRequest {
    #[serde(default)]
    executor_models: std::collections::HashMap<String, String>,
}

// ============================================================================
// Profile Handlers
// ============================================================================

async fn list_profiles(
    State(_state): State<AppState>,
) -> Result<ApiResponse<Vec<ProfileSummary>>, AppError> {
    let cfg = load()?;
    let current = cfg.current_profile.clone();
    let summaries: Vec<ProfileSummary> = cfg.profiles.into_iter().map(|(name, p)| {
        let is_current = name == current;
        ProfileSummary { name, display_name: p.name, description: p.description, executor_count: p.executors.len(), is_current }
    }).collect();
    Ok(ApiResponse::ok(summaries))
}

async fn get_current_profile(
    State(_state): State<AppState>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let cfg = load()?;
    let current_name = cfg.current_profile.clone();
    let profile = cfg.profiles.get(&current_name).ok_or(AppError::NotFound)?;
    let value = serde_json::json!({
        "name": current_name,
        "display_name": profile.name,
        "description": profile.description,
        "executors": profile.executors,
    });
    Ok(ApiResponse::ok(value))
}

async fn create_profile(
    State(_state): State<AppState>,
    ApiJson(req): ApiJson<CreateProfileRequest>,
) -> Result<ApiResponse<ProfileSummary>, AppError> {
    validate_profile_name(&req.name)?;
    let mut cfg = load()?;
    if cfg.profiles.contains_key(&req.name) {
        return Err(AppError::BadRequest(format!("Profile '{}' already exists", req.name)));
    }
    let profile_name = req.name;
    let profile = ExecutorProfile {
        name: req.display_name,
        description: req.description,
        executors: req.executors,
    };
    cfg.profiles.insert(profile_name.clone(), profile);
    save(&cfg)?;
    let is_current = cfg.current_profile == profile_name;
    let p = cfg.profiles.get(&profile_name).ok_or_else(|| AppError::Internal("profile not found after creation".to_string()))?;
    Ok(ApiResponse::ok(ProfileSummary {
        name: profile_name,
        display_name: p.name.clone(),
        description: p.description.clone(),
        executor_count: p.executors.len(),
        is_current,
    }))
}

async fn update_profile(
    State(_state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    ApiJson(req): ApiJson<UpdateProfileRequest>,
) -> Result<ApiResponse<ProfileSummary>, AppError> {
    let mut cfg = load()?;
    let profile = cfg.profiles.get_mut(&name).ok_or(AppError::NotFound)?;
    if let Some(dn) = req.display_name { profile.name = dn; }
    if let Some(desc) = req.description { profile.description = if desc.is_empty() { None } else { Some(desc) }; }
    for (exec_name, val) in req.executors {
        match val {
            Some(ref_) => { profile.executors.insert(exec_name, ref_); }
            None => { profile.executors.remove(&exec_name); }
        }
    }
    save(&cfg)?;
    let is_current = cfg.current_profile == name;
    let p = cfg.profiles.get(&name).ok_or_else(|| AppError::Internal("profile not found after operation".to_string()))?;
    Ok(ApiResponse::ok(ProfileSummary {
        name, display_name: p.name.clone(), description: p.description.clone(),
        executor_count: p.executors.len(), is_current,
    }))
}

async fn delete_profile(
    State(_state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    let mut cfg = load()?;
    if name == cfg.current_profile {
        return Err(AppError::BadRequest("Cannot delete the currently active profile. Switch to another profile first.".to_string()));
    }
    cfg.profiles.remove(&name).ok_or(AppError::NotFound)?;
    save(&cfg)?;
    Ok(ApiResponse::ok(()))
}

async fn apply_profile(
    State(_state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<ApiResponse<ApplyProfileResult>, AppError> {
    let mut cfg = load()?;
    let profile = cfg.profiles.get(&name).cloned().ok_or(AppError::NotFound)?;

    let registry = ProfileGeneratorRegistry::new();
    let mut applied = Vec::new();
    let mut skipped = Vec::new();
    let mut errors = Vec::new();

    for (exec_name, exec_ref) in &profile.executors {
        if let Some(generator) = registry.get(exec_name) {
            // 从 profiles.yaml 中解析 provider
            match resolve_provider(&cfg, exec_ref) {
                Ok((provider, model_name)) => {
                    // 构建一个与 ref 相同的临时 ref（model 取 resolved 值）
                    let ref_for_gen = ExecutorRef {
                        provider: exec_ref.provider.clone(),
                        model: model_name,
                    };
                    let session_dir = crate::adapters::find_executor(exec_name)
                        .map(|def| def.session_dir)
                        .unwrap_or("");
                    match generator.generate(&ref_for_gen, provider, session_dir) {
                        Ok(()) => applied.push(exec_name.clone()),
                        Err(e) => errors.push(format!("{}: {}", exec_name, e)),
                    }
                }
                Err(e) => errors.push(format!("{}: {}", exec_name, e)),
            }
        } else {
            skipped.push(exec_name.clone());
        }
    }

    cfg.current_profile = name.clone();
    save(&cfg)?;

    Ok(ApiResponse::ok(ApplyProfileResult {
        profile_name: name,
        profile_display_name: profile.name,
        applied_executors: applied,
        skipped_executors: skipped,
        errors,
    }))
}

// ============================================================================
// Helpers
// ============================================================================

fn load() -> Result<ProfilesConfig, AppError> {
    Ok(ProfilesConfig::load())
}

fn save(cfg: &ProfilesConfig) -> Result<(), AppError> {
    cfg.save().map_err(AppError::Internal)
}

fn validate_profile_name(name: &str) -> Result<(), AppError> {
    if name.is_empty() {
        return Err(AppError::BadRequest("Name cannot be empty".to_string()));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(AppError::BadRequest("Name can only contain letters, numbers, hyphens, and underscores".to_string()));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_profile_name_valid() {
        assert!(validate_profile_name("default").is_ok());
        assert!(validate_profile_name("my-provider").is_ok());
    }

    #[test]
    fn test_validate_profile_name_invalid() {
        assert!(validate_profile_name("").is_err());
        assert!(validate_profile_name("with space").is_err());
        assert!(validate_profile_name("中文").is_err());
    }
}
