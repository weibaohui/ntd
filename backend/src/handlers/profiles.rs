//! Profile 管理 API Handlers。
//!
//! 提供 Profile 的增删改查和应用（apply/switch）接口。
//! 所有 Profile 数据存储在 `~/.ntd/profiles.yaml` 中，通过 `ProfilesConfig` 加载。
//!
//! # API 路由
//!
//! | 方法 | 路径 | 说明 |
//! |------|------|------|
//! | GET | `/api/v1/profiles` | 获取所有 profile 摘要列表 |
//! | GET | `/api/v1/profiles/current` | 获取当前 profile 详情 |
//! | POST | `/api/v1/profiles` | 创建新 profile |
//! | PUT | `/api/v1/profiles/{name}` | 更新指定 profile |
//! | DELETE | `/api/v1/profiles/{name}` | 删除指定 profile |
//! | POST | `/api/v1/profiles/{name}/apply` | 应用（切换）指定 profile |

use axum::{
    Router,
    extract::State,
    routing::{get, post, put},
};

use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::ApiResponse;
use crate::profiles::{
    ApplyProfileResult, CreateProfileRequest, ExecutorProfile,
    ProfileGeneratorRegistry, ProfileSummary, ProfilesConfig, UpdateProfileRequest,
};

// ============================================================================
// 路由
// ============================================================================

/// 挂载所有的 profile 路由。
pub fn profile_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/profiles", get(list_profiles).post(create_profile))
        .route("/api/v1/profiles/current", get(get_current_profile))
        .route("/api/v1/profiles/{name}", put(update_profile).delete(delete_profile))
        .route("/api/v1/profiles/{name}/apply", post(apply_profile))
}

// ============================================================================
// Handlers
// ============================================================================

/// 获取所有 profile 摘要列表。
async fn list_profiles(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<ProfileSummary>>, AppError> {
    // 读出配置文件
    let cfg = load_profiles_config(&state).map_err(AppError::Internal)?;

    let current = cfg.current_profile.clone();
    let summaries: Vec<ProfileSummary> = cfg
        .profiles
        .into_iter()
        .map(|(name, profile)| ProfileSummary {
            name: name.clone(),
            display_name: profile.name,
            description: profile.description,
            executor_count: profile.executors.len(),
            is_current: name == current,
        })
        .collect();

    Ok(ApiResponse::ok(summaries))
}

/// 获取当前 profile 的完整详情。
async fn get_current_profile(
    State(state): State<AppState>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let cfg = load_profiles_config(&state).map_err(AppError::Internal)?;
    let current_name = cfg.current_profile.clone();
    let profile = cfg
        .profiles
        .get(&current_name)
        .ok_or(AppError::NotFound)?;

    // 返回 profile 详情 + 名称标记
    let value = serde_json::json!({
        "name": current_name,
        "display_name": profile.name,
        "description": profile.description,
        "executors": profile.executors,
    });

    Ok(ApiResponse::ok(value))
}

/// 创建新 profile。
async fn create_profile(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<CreateProfileRequest>,
) -> Result<ApiResponse<ProfileSummary>, AppError> {
    // 校验 name 不重复
    validate_profile_name(&req.name)?;

    // 加载当前配置
    let mut cfg = load_profiles_config(&state).map_err(AppError::Internal)?;

    // 检查是否已存在同名 profile
    if cfg.profiles.contains_key(&req.name) {
        return Err(AppError::BadRequest(format!(
            "Profile '{}' already exists",
            req.name
        )));
    }

    // 构造新的 profile 条目
    let profile = ExecutorProfile {
        name: req.display_name,
        description: req.description,
        executors: req.executors,
    };
    let name = req.name;

    // 写回存储
    cfg.profiles.insert(name.clone(), profile);
    save_profiles_config(&state, &cfg).map_err(AppError::Internal)?;

    let is_current = cfg.current_profile == name;
    // profile 刚刚被操作，一定能取到；即便出 bug 也应返回 NotFound 而非 panic
    let profile_entry = cfg.profiles.get(&name).ok_or_else(|| AppError::Internal("profile not found after operation".to_string()))?;
    Ok(ApiResponse::ok(ProfileSummary {
        name,
        display_name: profile_entry.name.clone(),
        description: profile_entry.description.clone(),
        executor_count: profile_entry.executors.len(),
        is_current,
    }))
}

/// 更新指定 profile。
async fn update_profile(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    ApiJson(req): ApiJson<UpdateProfileRequest>,
) -> Result<ApiResponse<ProfileSummary>, AppError> {
    let mut cfg = load_profiles_config(&state).map_err(AppError::Internal)?;

    let profile = cfg
        .profiles
        .get_mut(&name)
        .ok_or(AppError::NotFound)?;

    if let Some(display_name) = req.display_name {
        profile.name = display_name;
    }
    if let Some(desc) = req.description {
        profile.description = if desc.is_empty() { None } else { Some(desc) };
    }

    // 逐个执行器更新配置
    for (exec_name, settings) in req.executors {
        match settings {
            Some(s) => {
                profile.executors.insert(exec_name, s);
            }
            None => {
                // None = 删除该执行器的配置
                profile.executors.remove(&exec_name);
            }
        }
    }

    save_profiles_config(&state, &cfg).map_err(AppError::Internal)?;

    let is_current = cfg.current_profile == name;
    // profile 刚刚被操作，一定能取到；即便出 bug 也应返回 NotFound 而非 panic
    let profile_entry = cfg.profiles.get(&name).ok_or_else(|| AppError::Internal("profile not found after operation".to_string()))?;
    Ok(ApiResponse::ok(ProfileSummary {
        name,
        display_name: profile_entry.name.clone(),
        description: profile_entry.description.clone(),
        executor_count: profile_entry.executors.len(),
        is_current,
    }))
}

/// 删除指定 profile（禁止删除当前激活的 profile）。
async fn delete_profile(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    let mut cfg = load_profiles_config(&state).map_err(AppError::Internal)?;

    if name == cfg.current_profile {
        return Err(AppError::BadRequest(
            "Cannot delete the currently active profile. Switch to another profile first.".to_string(),
        ));
    }

    if cfg.profiles.remove(&name).is_none() {
        return Err(AppError::NotFound);
    }

    save_profiles_config(&state, &cfg).map_err(AppError::Internal)?;

    Ok(ApiResponse::ok(()))
}

/// 应用（切换到）指定 profile。
///
/// 流程：
/// 1. 从 profiles.yaml 读取目标 profile
/// 2. 使用 ProfileGeneratorRegistry 为每个已配置的执行器生成并写入配置文件
/// 3. 更新 current_profile 标记
/// 4. 返回各执行器的应用结果（成功/跳过/失败）
async fn apply_profile(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<ApiResponse<ApplyProfileResult>, AppError> {
    let mut cfg = load_profiles_config(&state).map_err(AppError::Internal)?;

    let profile = cfg
        .profiles
        .get(&name)
        .ok_or(AppError::NotFound)?
        .clone();

    let registry = ProfileGeneratorRegistry::new();
    let mut applied = Vec::new();
    let mut skipped = Vec::new();
    let mut errors = Vec::new();

    // 遍历 profile 中包含的所有执行器配置
    for (exec_name, settings) in &profile.executors {
        if let Some(generator) = registry.get(exec_name) {
            // 从 ExecutorDef 获取 session_dir
            let session_dir = crate::adapters::find_executor(exec_name)
                .map(|def| def.session_dir)
                .unwrap_or("");

            match generator.generate(settings, session_dir) {
                Ok(()) => applied.push(exec_name.clone()),
                Err(e) => errors.push(format!("{}: {}", exec_name, e)),
            }
        } else {
            // 该执行器尚无生成器实现，跳过
            skipped.push(exec_name.clone());
        }
    }

    // 更新当前 profile 标记
    cfg.current_profile = name.clone();
    save_profiles_config(&state, &cfg).map_err(AppError::Internal)?;

    let result = ApplyProfileResult {
        profile_name: name,
        profile_display_name: profile.name,
        applied_executors: applied,
        skipped_executors: skipped,
        errors,
    };

    Ok(ApiResponse::ok(result))
}

// ============================================================================
// Helpers
// ============================================================================

/// 从 AppState 的 config 共享锁中获取 profiles.yaml 路径，加载 ProfilesConfig。
fn load_profiles_config(_state: &AppState) -> Result<ProfilesConfig, String> {
    // 直接从磁盘加载，与 Config::load() 对齐
    Ok(ProfilesConfig::load())
}

/// 保存 ProfilesConfig 到磁盘。
fn save_profiles_config(_state: &AppState, cfg: &ProfilesConfig) -> Result<(), String> {
    cfg.save()
}

/// 校验 profile name 的合法性：只能包含字母、数字、中划线、下划线。
fn validate_profile_name(name: &str) -> Result<(), AppError> {
    if name.is_empty() {
        return Err(AppError::BadRequest("Profile name cannot be empty".to_string()));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::BadRequest(
            "Profile name can only contain letters, numbers, hyphens, and underscores".to_string(),
        ));
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
        assert!(validate_profile_name("my-profile").is_ok());
        assert!(validate_profile_name("work_config").is_ok());
        assert!(validate_profile_name("a").is_ok());
    }

    #[test]
    fn test_validate_profile_name_invalid() {
        assert!(validate_profile_name("").is_err());
        assert!(validate_profile_name("with space").is_err());
        assert!(validate_profile_name("special!").is_err());
        assert!(validate_profile_name("中文").is_err());
    }
}
