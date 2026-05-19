use axum::extract::{Path, State};
use std::path::PathBuf;
use std::time::Duration;

use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::{ApiResponse, ExecutorConfig, ExecutorDetectResult, ExecutorTestResult, UpdateExecutorRequest, ExecutorBatchDetectResult, ExecutorDetectInfo};

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

    let path = if ec.path.is_empty() { name.clone() } else { ec.path.clone() };
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
    let (found, _) = detect_binary(&path);

    if !found {
        return Ok(ApiResponse::ok(ExecutorTestResult {
            test_passed: false,
            output: None,
            error: Some(format!("Binary not found: {}", path)),
        }));
    }

    // Try running --version with a short timeout
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::process::Command::new(&path)
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
                state.db.update_executor(&ec.name, None, Some(false), None, None)
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
            state.db.update_executor(&ec.name, None, Some(new_enabled), None, None)
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
