//! `script_check` 门禁：在 workspace 内运行指定脚本检查产物。
//!
//! 安全限制：
//! - 脚本路径必须是 workspace 目录内的已有文件（经路径安全校验）；
//! - 不允许直接执行 shell 命令。
//!
//! 配置格式：
//! ```json
//! {"script": "scripts/validate_prd.sh"}
//! ```

use std::path::Path;

use super::{GateContext, GateError, GateResult};

/// 评估脚本检查门禁。
pub async fn evaluate(ctx: &GateContext<'_>) -> Result<GateResult, GateError> {
    let script_path = ctx
        .config
        .script
        .as_deref()
        .ok_or_else(|| GateError::ConfigParse("script_check requires 'script' field".to_string()))?;

    // 脚本路径必须位于 workspace 内。
    let resolved = resolve_script_path(ctx.workspace_path, script_path)?;

    // 检查文件是否存在且可执行。
    if !resolved.exists() {
        return Ok(GateResult {
            gate_name: ctx.config.name.clone(),
            gate_type: "script_check".to_string(),
            passed: false,
            detail: Some(format!("校验脚本不存在: {}", script_path)),
        });
    }

    // 使用 tokio::process::Command 执行脚本。
    let output = tokio::process::Command::new(&resolved)
        .current_dir(ctx.workspace_path)
        .output()
        .await
        .map_err(|e| GateError::ScriptExecution(format!("执行脚本失败: {}", e)))?;

    let passed = output.status.success();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let mut detail = stdout;
    if !stderr.is_empty() {
        if !detail.is_empty() {
            detail.push('\n');
        }
        detail.push_str(&stderr);
    }

    Ok(GateResult {
        gate_name: ctx.config.name.clone(),
        gate_type: "script_check".to_string(),
        passed,
        detail: Some(if detail.is_empty() {
            if passed {
                "脚本执行成功".to_string()
            } else {
                format!("脚本返回退出码: {}", output.status.code().unwrap_or(-1))
            }
        } else {
            detail
        }),
    })
}

/// 校验脚本路径，防止目录遍历攻击。
fn resolve_script_path(workspace_path: &str, script_path: &str) -> Result<std::path::PathBuf, GateError> {
    let rel = Path::new(script_path);
    // 字符串级校验：拒绝绝对路径、父级引用。
    if rel.is_absolute() {
        return Err(GateError::ConfigParse(format!(
            "脚本路径不能是绝对路径: {}",
            script_path
        )));
    }
    if rel.components().any(|c| {
        matches!(c, std::path::Component::ParentDir | std::path::Component::Prefix(_))
    }) {
        return Err(GateError::ConfigParse(format!(
            "脚本路径不能包含父级引用: {}",
            script_path
        )));
    }
    Ok(Path::new(workspace_path).join(rel))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::services::process::GateDefinition;

    use super::*;

    #[test]
    fn test_resolve_script_path_rejects_absolute() {
        let err = resolve_script_path("/tmp/ws", "/etc/passwd").unwrap_err();
        assert!(err.to_string().contains("绝对路径"));
    }

    #[test]
    fn test_resolve_script_path_rejects_parent_traversal() {
        let err = resolve_script_path("/tmp/ws", "../escape.sh").unwrap_err();
        assert!(err.to_string().contains("父级引用"));
    }

    #[tokio::test]
    async fn test_script_check_script_missing() {
        let ctx = GateContext {
            step_execution_id: 1,
            config: GateDefinition {
                name: "Check".to_string(),
                gate_type: "script_check".to_string(),
                artifact: None,
                criteria_ref: None,
                min_score: None,
                script: Some("nonexistent.sh".to_string()),
            },
            skill_names: &[],
            artifacts: &[],
            execution_result: None,
            acceptance_criteria: None,
            workspace_path: "/tmp",
        };
        let result = evaluate(&ctx).await.unwrap();
        assert!(!result.passed);
    }

    #[tokio::test]
    async fn test_script_check_config_missing_script() {
        let ctx = GateContext {
            step_execution_id: 1,
            config: GateDefinition {
                name: "Check".to_string(),
                gate_type: "script_check".to_string(),
                artifact: None,
                criteria_ref: None,
                min_score: None,
                script: None,
            },
            skill_names: &[],
            artifacts: &[],
            execution_result: None,
            acceptance_criteria: None,
            workspace_path: "/tmp",
        };
        let result = evaluate(&ctx).await;
        assert!(result.is_err());
    }
}
