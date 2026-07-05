//! 静态文件服务模块：提供嵌入式前端资源的 HTTP 服务。
//!
//! 支持 Vite 构建产物的智能缓存策略：
//! - 带 hash 的资源（如 index-AbCd1234.js）使用 immutable 长缓存
//! - 其他资源使用 no-cache，确保更新及时生效

use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};

use crate::Assets;
use crate::models::ApiResponse;

use super::errors::AppError;

/// 首页 handler：返回嵌入的 index.html。
pub async fn index_handler() -> Result<Html<String>, AppError> {
    let content = Assets::get("index.html")
        .ok_or_else(|| AppError::Internal("index.html not found in embedded assets".to_string()))?;
    Ok(Html(String::from_utf8_lossy(&content.data).to_string()))
}

/// 静态资源 handler：根据路径返回对应的嵌入资源。
pub async fn static_handler(Path(path): Path<String>) -> Response {
    let path = path.trim_start_matches('/');
    let full_path = if path.is_empty() {
        "index.html".to_string()
    } else {
        format!("assets/{}", path)
    };

    match Assets::get(&full_path) {
        Some(content) => {
            let mime_str = guess_mime(path);
            let cache_control = cache_control_for(path, mime_str);
            let mime_value = match header::HeaderValue::from_str(mime_str) {
                Ok(v) => v,
                Err(_) => {
                    tracing::warn!(
                        "invalid mime derived for {}: {}; fallback to octet-stream",
                        path,
                        mime_str
                    );
                    header::HeaderValue::from_static("application/octet-stream")
                }
            };
            let cache_value = header::HeaderValue::from_static(cache_control);
            ([
                (header::CONTENT_TYPE, mime_value),
                (header::CACHE_CONTROL, cache_value),
            ], content.data.to_vec()).into_response()
        }
        None => match Assets::get("index.html") {
            Some(content) => {
                Html(String::from_utf8_lossy(&content.data).to_string()).into_response()
            }
            None => (StatusCode::NOT_FOUND, "Not found").into_response(),
        },
    }
}

/// 根据文件路径推断 MIME 类型。
fn guess_mime(path: &str) -> &'static str {
    mime_guess::from_path(path)
        .first_raw()
        .unwrap_or("application/octet-stream")
}

/// 根据路径与 MIME 返回合适的 `Cache-Control` 头。
fn cache_control_for(path: &str, mime: &str) -> &'static str {
    if is_vite_hashed_asset(path) && is_cacheable_mime(mime) {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    }
}

/// 是否是 Vite 风格的带 hash 资源名（`<name>-<hash>.<ext>`）。
fn is_vite_hashed_asset(path: &str) -> bool {
    let Some((base, ext)) = path.rsplit_once('.') else {
        return false;
    };
    if !is_vite_hashed_extension(ext) {
        return false;
    }
    let Some((_name, hash)) = base.rsplit_once('-') else {
        return false;
    };
    hash.len() >= 6 && hash.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Vite 在生产构建中会带 hash 的扩展名集合。
fn is_vite_hashed_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "js" | "mjs" | "css" | "woff" | "woff2" | "ttf" | "eot" | "svg" | "png" | "jpg"
            | "jpeg" | "gif" | "webp" | "ico" | "json" | "map" | "wasm"
    )
}

/// 该 MIME 是否适合下发 immutable 长缓存。
fn is_cacheable_mime(mime: &str) -> bool {
    matches!(
        mime,
        "text/javascript"
            | "application/javascript"
            | "text/css"
            | "font/woff2"
            | "font/woff"
            | "application/font-woff"
            | "font/ttf"
            | "application/vnd.ms-fontobject"
    )
}

#[derive(serde::Serialize)]
struct VersionResponse {
    version: String,
    git_sha: String,
    git_describe: String,
}

/// 健康检查 handler。
pub async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, axum::Json(serde_json::json!({"status": "ok"})))
}

/// 版本查询 handler：返回编译时嵌入的版本信息。
pub async fn version_handler() -> impl IntoResponse {
    let version = option_env!("NTD_VERSION").unwrap_or("unknown");
    let git_sha = option_env!("NTD_GIT_SHA").unwrap_or("unknown");
    let git_describe = option_env!("NTD_VERSION_FULL").unwrap_or("unknown");
    let response = VersionResponse {
        version: version.to_string(),
        git_sha: git_sha.to_string(),
        git_describe: git_describe.to_string(),
    };
    ApiResponse::ok(response)
}

/// 查询 npm 最新版本号，用于前端版本检查提示。
pub async fn version_latest_handler() -> impl IntoResponse {
    let output = std::process::Command::new("npm")
        .args(["view", "@weibaohui/nothing-todo", "version"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let latest = String::from_utf8_lossy(&out.stdout).trim().to_string();
            ApiResponse::ok(serde_json::json!({ "latest": latest }))
        }
        Ok(out) => {
            let err_msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
            tracing::warn!("npm view failed: {}", err_msg);
            ApiResponse::ok(serde_json::json!({ "latest": null, "error": err_msg }))
        }
        Err(e) => {
            tracing::warn!("Failed to run npm view: {}", e);
            ApiResponse::ok(serde_json::json!({ "latest": null, "error": e.to_string() }))
        }
    }
}

/// 返回 ntd.update 标记文件的路径（Unix 版）。
fn ntd_update_marker_path() -> String {
    "/tmp/ntd.update".to_string()
}

/// 返回子进程清理标记文件时使用的路径表达式。
fn ntd_update_marker_cleanup_path() -> String {
    #[cfg(unix)]
    { "/tmp/ntd.update".to_string() }
    #[cfg(windows)]
    { "%TEMP%\\ntd.update".to_string() }
}

/// sh -c 回退方案：在非 Linux 平台或 systemd-run 不可用时使用。
#[cfg(not(windows))]
fn spawn_redeploy_sh_fallback(ntd_cmd: &str, marker_cleanup_path: &str, log_path: &str) {
    let quoted = crate::daemon::common::shell_quote_single(ntd_cmd);
    std::process::Command::new("sh")
        .args(["-c", &format!(
            "(sleep 3; {quoted} daemon install --force; {quoted} daemon start; rm -f {marker}) >> {log} 2>&1 &",
            quoted = quoted,
            marker = marker_cleanup_path,
            log = log_path,
        )])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok();
}

/// 执行 npm 升级并采用分离式自更新方案重新部署 daemon 服务。
pub async fn version_upgrade_handler() -> impl IntoResponse {
    let prefix = crate::npm_utils::get_npm_global_prefix();
    let npm_result = std::process::Command::new("npm")
        .args(["install", "-g", &format!("--prefix={}", prefix), "@weibaohui/nothing-todo@latest"])
        .output();

    match &npm_result {
        Ok(out) => {
            tracing::info!(
                "npm upgrade stdout: {}, stderr: {}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        Err(e) => {
            tracing::error!("Failed to run npm: {}", e);
            let err_resp: ApiResponse<serde_json::Value> = ApiResponse::err(1, &format!("npm upgrade failed: {}", e));
            return err_resp;
        }
    }

    if let Ok(out) = &npm_result {
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let err_msg = if stderr.is_empty() {
                "npm upgrade failed".to_string()
            } else {
                format!("npm upgrade failed: {}", stderr.trim())
            };
            let err_resp: ApiResponse<serde_json::Value> = ApiResponse::err(1, &err_msg);
            return err_resp;
        }
    }

    let ntd_cmd = crate::npm_utils::find_ntd_binary(&prefix);

    if ntd_cmd == "ntd" {
        tracing::error!("Self-update: ntd binary not found");
        let err_resp: ApiResponse<serde_json::Value> = ApiResponse::err(1, "无法更新：未找到 ntd 可执行文件路径");
        return err_resp;
    }
    if !crate::daemon::common::is_safe_ntd_path(&ntd_cmd) {
        tracing::error!("Refusing self-update: ntd path {:?} contains characters outside [A-Za-z0-9/_.-]", ntd_cmd);
        let err_resp: ApiResponse<serde_json::Value> = ApiResponse::err(1, "无法更新：ntd 路径包含非法字符（可能 npm prefix 被污染）");
        return err_resp;
    }

    std::fs::write(ntd_update_marker_path(), "").ok();
    let marker_cleanup_path = ntd_update_marker_cleanup_path();

    #[cfg(target_os = "linux")]
    {
        let script = format!(
            "sleep 3; {} daemon install --force; {} daemon start; rm -f {}",
            ntd_cmd, ntd_cmd, marker_cleanup_path,
        );
        match crate::daemon::spawn_detached_redeploy_nonblocking(&script) {
            Ok(()) => {
                tracing::info!("Self-update (Linux): systemd-run redeploy spawned. ntd path: {}", ntd_cmd);
            }
            Err(e) => {
                tracing::warn!("Self-update: systemd-run failed ({}), falling back to sh -c", e);
                let fallback_log = crate::daemon::redeploy_log_path().to_string_lossy().to_string();
                spawn_redeploy_sh_fallback(&ntd_cmd, &marker_cleanup_path, &fallback_log);
            }
        }
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        spawn_redeploy_sh_fallback(&ntd_cmd, &marker_cleanup_path, "/tmp/ntd-upgrade.log");
    }

    #[cfg(windows)]
    {
        let quoted = crate::daemon::common::shell_quote_single(&ntd_cmd);
        #[cfg(windows)]
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .args(["/C", &format!(
                "timeout /t 3 /nobreak >nul && {quoted} daemon install --force && {quoted} daemon start && del /f /q {marker}",
                quoted = quoted, marker = marker_cleanup_path,
            )])
            .creation_flags(0x08000000)
            .spawn()
            .ok();
    }

    tracing::info!("Self-update: npm upgraded, forked child process. ntd path: {}", ntd_cmd);
    let response = ApiResponse::ok(serde_json::json!({
        "status": "upgrade_started",
        "message": "升级流程已启动，服务即将重启",
    }));
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        tracing::info!("Self-update: main process exiting after response sent");
        std::process::exit(0);
    });
    response
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    #[test]
    fn guess_mime_recognises_common_static_types() {
        assert_eq!(guess_mime("foo.js"), "text/javascript");
        assert_eq!(guess_mime("foo.css"), "text/css");
        assert_eq!(guess_mime("foo.JS"), "text/javascript");
        assert_eq!(guess_mime("Makefile"), "application/octet-stream");
    }

    #[test]
    fn is_vite_hashed_asset_detects_typical_vite_hashes() {
        assert!(is_vite_hashed_asset("index-AbCd1234.js"));
        assert!(!is_vite_hashed_asset("foo-bar.js"));
        assert!(!is_vite_hashed_asset("index.html"));
    }

    #[test]
    fn is_vite_hashed_extension_matches_expected_set() {
        for ext in ["js", "mjs", "css", "woff", "woff2", "ttf", "eot", "svg", "png", "wasm"] {
            assert!(is_vite_hashed_extension(ext), "expected true for .{}", ext);
        }
        for ext in ["txt", "pdf", "zip"] {
            assert!(!is_vite_hashed_extension(ext), "expected false for .{}", ext);
        }
    }

    #[test]
    fn is_cacheable_mime_allows_js_css_and_fonts() {
        assert!(is_cacheable_mime("text/javascript"));
        assert!(is_cacheable_mime("font/woff2"));
        assert!(!is_cacheable_mime("image/png"));
    }

    #[test]
    fn cache_control_for_vite_hashed_js_gets_immutable() {
        assert_eq!(
            cache_control_for("index-AbCd1234.js", "text/javascript"),
            "public, max-age=31536000, immutable"
        );
        assert_eq!(cache_control_for("index.html", "text/html"), "no-cache");
    }
}
