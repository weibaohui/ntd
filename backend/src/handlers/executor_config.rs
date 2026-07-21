use axum::extract::{Path, State};
use std::path::PathBuf;
use std::time::Duration;

use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::{ApiResponse, ExecutorConfig, ExecutorDetectResult, ExecutorTestResult, UpdateExecutorRequest, ExecutorBatchDetectResult, ExecutorDetectInfo, ExecutorPathResolveResult};

pub async fn list_executors(State(state): State<AppState>) -> Result<ApiResponse<Vec<ExecutorConfig>>, AppError> {
    let executors = state.db.get_executors().await.map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(ApiResponse::ok(executors))
}

pub async fn update_executor(
    State(state): State<AppState>,
    Path(name): Path<String>,
    ApiJson(req): ApiJson<UpdateExecutorRequest>,
) -> Result<ApiResponse<ExecutorConfig>, AppError> {
    state.db.update_executor(
        &name,
        req.path.as_deref(),
        req.enabled,
        req.display_name.as_deref(),
        req.session_dir.as_deref(),
        req.default_model.as_deref(),
    ).await.map_err(|e| AppError::Internal(e.to_string()))?;

    // Re-read updated executor
    let ec = state.db.get_executor_by_name(&name).await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or(AppError::NotFound)?;

    // Update registry based on enabled state
    if ec.enabled {
        state.executor_registry.register_by_name(&ec.name, &ec.path).await;
    } else if let Some(et) = crate::adapters::parse_executor_type(&ec.name) {
        state.executor_registry.unregister(et).await;
    }

    Ok(ApiResponse::ok(ec))
}

pub async fn detect_executor(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<ApiResponse<ExecutorDetectResult>, AppError> {
    let ec = state.db.get_executor_by_name(&name).await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or(AppError::NotFound)?;

    // ec.path 非空时直接 move，无需 clone——后续不再使用 ec.path
    let path = if ec.path.is_empty() { name.clone() } else { ec.path };
    let (found, resolved) = detect_binary(&path);

    Ok(ApiResponse::ok(ExecutorDetectResult {
        binary_found: found,
        path_resolved: resolved,
    }))
}

pub async fn test_executor(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<ApiResponse<ExecutorTestResult>, AppError> {
    let ec = state.db.get_executor_by_name(&name).await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or(AppError::NotFound)?;

    let path = if ec.path.is_empty() { name.clone() } else { ec.path.clone() };
    let (found, resolved) = detect_binary(&path);

    if !found {
        return Ok(ApiResponse::ok(ExecutorTestResult {
            test_passed: false,
            output: None,
            error: Some(format!("Binary not found: {}", path)),
        }));
    }

    // 使用 resolved 路径（detect_binary 已展开 ~ 等），避免原始路径含 ~ 导致 OS 找不到
    let exec_path = resolved.unwrap_or_else(|| path.clone());

    // Try running --version with a short timeout
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::process::Command::new(&exec_path)
            .arg("--version")
            .output(),
    ).await;

    match output {
        Ok(Ok(out)) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let combined = if stdout.is_empty() { stderr } else { stdout };
            Ok(ApiResponse::ok(ExecutorTestResult {
                test_passed: out.status.success(),
                output: Some(combined.trim().to_string()),
                error: None,
            }))
        }
        Ok(Err(e)) => Ok(ApiResponse::ok(ExecutorTestResult {
            test_passed: false,
            output: None,
            error: Some(format!("Failed to execute: {}", e)),
        })),
        Err(_) => Ok(ApiResponse::ok(ExecutorTestResult {
            test_passed: false,
            output: None,
            error: Some("Execution timed out (10s)".to_string()),
        })),
    }
}

/// Check if a binary exists at the given path or in PATH.
fn detect_binary(path: &str) -> (bool, Option<String>) {
    // Expand ~ to home directory
    let expanded = if path.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            path.replacen('~', &home.to_string_lossy(), 1)
        } else {
            path.to_string()
        }
    } else {
        path.to_string()
    };

    let p = PathBuf::from(&expanded);

    // If it looks like an absolute or relative path (contains separator)
    if p.is_absolute() || expanded.contains('/') || expanded.contains('\\') {
        if p.exists() {
            return (true, Some(p.to_string_lossy().to_string()));
        }
        return (false, None);
    }

    // Bare command name — look up in PATH using `which` equivalent
    match which::which(&expanded) {
        Ok(resolved) => (true, Some(resolved.to_string_lossy().to_string())),
        Err(_) => (false, None),
    }
}

pub async fn detect_all_executors(
    State(state): State<AppState>,
) -> Result<ApiResponse<ExecutorBatchDetectResult>, AppError> {
    let executors = state.db.get_executors().await.map_err(|e| AppError::Internal(e.to_string()))?;

    let mut results: Vec<ExecutorDetectInfo> = Vec::new();
    let mut found_count = 0;

    for ec in executors {
        // If path is empty, detection fails (no valid path to detect)
        if ec.path.is_empty() {
            if ec.enabled {
                // Was enabled but path is empty - disable it
                state.db.update_executor(&ec.name, None, Some(false), None, None, None)
                    .await
                    .map_err(|e| AppError::Internal(e.to_string()))?;

                if let Some(et) = crate::adapters::parse_executor_type(&ec.name) {
                    state.executor_registry.unregister(et).await;
                }
            }

            results.push(ExecutorDetectInfo {
                name: ec.name,
                display_name: ec.display_name,
                binary_found: false,
                path_resolved: None,
                enabled: false,
            });
            continue;
        }

        let (found, resolved) = detect_binary(&ec.path);
        // Clone resolved for path_resolved field before moving
        let path_resolved = resolved.clone();

        // Update executor enabled state based on detection result
        let new_enabled = found;
        if ec.enabled != new_enabled {
            state.db.update_executor(&ec.name, None, Some(new_enabled), None, None, None)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;

            // Update registry based on new enabled state
            if new_enabled {
                // Use resolved path if available, otherwise clone original path
                let path_to_register = resolved.unwrap_or_else(|| ec.path.clone());
                state.executor_registry.register_by_name(&ec.name, &path_to_register).await;
            } else if let Some(et) = crate::adapters::parse_executor_type(&ec.name) {
                state.executor_registry.unregister(et).await;
            }
        }

        if found {
            found_count += 1;
        }

        results.push(ExecutorDetectInfo {
            name: ec.name,
            display_name: ec.display_name,
            binary_found: found,
            path_resolved,
            enabled: new_enabled,
        });
    }

    let total = results.len();
    Ok(ApiResponse::ok(ExecutorBatchDetectResult {
        results,
        total,
        found_count,
    }))
}

/// 用 `which` 查找执行器的真实路径，如果找到且与数据库中不同则更新数据库。
/// 前端在执行器路径无效时调用此接口进行自动修复。
pub async fn resolve_executor_path(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<ApiResponse<ExecutorPathResolveResult>, AppError> {
    let ec = state.db.get_executor_by_name(&name).await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or(AppError::NotFound)?;

    let old_path = if ec.path.is_empty() { name.clone() } else { ec.path.clone() };
    let (found, resolved) = detect_binary(&old_path);

    if !found {
        return Ok(ApiResponse::ok(ExecutorPathResolveResult {
            binary_found: false,
            path_resolved: None,
            path_updated: false,
            old_path: Some(ec.path.clone()),
            new_path: None,
        }));
    }

    // found=true 时 resolved 必定为 Some——detect_binary 返回 (true, Some(path))
    #[allow(clippy::unwrap_used)]
    let resolved = resolved.unwrap();
    let path_updated = ec.path != resolved;

    // 如果路径有变化，更新数据库
    if path_updated {
        state.db.update_executor(&name, Some(&resolved), None, None, None, None)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // 重新注册到 registry
        if ec.enabled {
            state.executor_registry.register_by_name(&name, &resolved).await;
        }
    }

    Ok(ApiResponse::ok(ExecutorPathResolveResult {
        binary_found: true,
        path_resolved: Some(resolved.clone()),
        path_updated,
        old_path: Some(ec.path.clone()),
        new_path: Some(resolved),
    }))
}

/// 获取系统默认执行器。
/// 如果没有设置默认执行器，返回 null（前端可回退到 claudecode）。
pub async fn get_default_executor(
    State(state): State<AppState>,
) -> Result<ApiResponse<Option<ExecutorConfig>>, AppError> {
    let executor = state.db.get_default_executor().await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(ApiResponse::ok(executor))
}

/// 设置指定执行器为系统默认执行器。
/// 会自动清除其他执行器的默认标记，确保只有一个默认执行器。
pub async fn set_default_executor(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<ApiResponse<ExecutorConfig>, AppError> {
    let executor = state.db.set_default_executor(&name).await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or(AppError::NotFound)?;
    Ok(ApiResponse::ok(executor))
}
