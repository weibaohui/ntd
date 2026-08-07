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
    // npm view 是同步子进程（含网络往返），直接在 async 路径调用会占住 tokio worker；
    // 挪进 spawn_blocking 交给阻塞线程池（091 性能优化）。
    let join = tokio::task::spawn_blocking(|| {
        std::process::Command::new("npm")
            .args(["view", "@weibaohui/ntd", "version"])
            .output()
    })
    .await;
    match join {
        // 任务正常返回且 npm 退出成功：取 stdout 首段即版本号。
        Ok(Ok(out)) if out.status.success() => {
            let latest = String::from_utf8_lossy(&out.stdout).trim().to_string();
            ApiResponse::ok(serde_json::json!({ "latest": latest }))
        }
        // npm 退出了但 exit code 非成功：透传 stderr 给前端做提示。
        Ok(Ok(out)) => {
            let err_msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
            tracing::warn!("npm view failed: {}", err_msg);
            ApiResponse::ok(serde_json::json!({ "latest": null, "error": err_msg }))
        }
        // npm 二进制未启动等 IO 错误：latest 置空，前端降级为不提示更新。
        Ok(Err(e)) => {
            tracing::warn!("Failed to run npm view: {}", e);
            ApiResponse::ok(serde_json::json!({ "latest": null, "error": e.to_string() }))
        }
        // spawn_blocking 任务 panic / 被取消：同样降级，不让版本检查拖垮接口。
        Err(e) => {
            tracing::warn!("npm view task panicked: {}", e);
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

/// npm 升级前置准备的结果：通过 spawn_blocking 跑完所有同步子进程后，把后续重部署 spawn
/// 所需的两个值带回 async 路径。
struct UpgradePrep {
    ntd_cmd: String,
    marker_cleanup_path: String,
}

/// 把「取 npm prefix → npm install -g → 找 ntd 二进制 → 安全校验 → 写标记」整段同步阻塞逻辑
/// 收敛成一个函数，供 spawn_blocking 调用。npm install 是长子进程（网络下载 + 解包），直接在
/// async 路径调用会占住 tokio worker（4 核机 4 并发即冻死运行时），故整体挪进阻塞线程池
/// （091 性能优化）。任一步失败返回错误消息，调用方据此短路。
fn prepare_upgrade_blocking() -> Result<UpgradePrep, String> {
    let prefix = crate::npm_utils::get_npm_global_prefix();
    let npm_result = std::process::Command::new("npm")
        .args(["install", "-g", &format!("--prefix={}", prefix), "@weibaohui/ntd@latest"])
        .output();

    // npm 子进程未启动等 IO 错误：直接上报，不进入后续校验。
    let out = match &npm_result {
        Ok(out) => {
            tracing::info!(
                "npm upgrade stdout: {}, stderr: {}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            out
        }
        Err(e) => {
            tracing::error!("Failed to run npm: {}", e);
            return Err(format!("npm upgrade failed: {}", e));
        }
    };
    // npm 退出了但 exit code 非成功：把 stderr 透传给前端做诊断。
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(if stderr.is_empty() {
            "npm upgrade failed".to_string()
        } else {
            format!("npm upgrade failed: {}", stderr.trim())
        });
    }

    // 定位新装的 ntd 二进制并校验路径合法性，防 npm prefix 被污染注入 shell 元字符。
    let ntd_cmd = crate::npm_utils::find_ntd_binary(&prefix);
    if ntd_cmd == "ntd" {
        tracing::error!("Self-update: ntd binary not found");
        return Err("无法更新：未找到 ntd 可执行文件路径".to_string());
    }
    if !crate::daemon::common::is_safe_ntd_path(&ntd_cmd) {
        tracing::error!(
            "Refusing self-update: ntd path {:?} contains characters outside [A-Za-z0-9/_.-]",
            ntd_cmd
        );
        return Err("无法更新：ntd 路径包含非法字符（可能 npm prefix 被污染）".to_string());
    }

    // 写更新标记：daemon 重启后据此判断「本次重启由升级触发」，从而做收尾。
    std::fs::write(ntd_update_marker_path(), "").ok();
    Ok(UpgradePrep {
        ntd_cmd,
        marker_cleanup_path: ntd_update_marker_cleanup_path(),
    })
}

/// 平台相关的「分离式重部署」。各分支都用 .spawn() fork 后立即返回（systemd-run / sh -c /
/// cmd 三选一），不阻塞调用方，因此可安全留在 async handler 内。
fn spawn_platform_redeploy(ntd_cmd: &str, marker_cleanup_path: &str) {
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
                spawn_redeploy_sh_fallback(ntd_cmd, marker_cleanup_path, &fallback_log);
            }
        }
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        spawn_redeploy_sh_fallback(ntd_cmd, marker_cleanup_path, "/tmp/ntd-upgrade.log");
    }
    #[cfg(windows)]
    {
        let quoted = crate::daemon::common::shell_quote_single(ntd_cmd);
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
}

/// 执行 npm 升级并采用分离式自更新方案重新部署 daemon 服务。
pub async fn version_upgrade_handler() -> impl IntoResponse {
    // prepare_upgrade_blocking 内含 npm install 长子进程，挪进 spawn_blocking 避免占住
    // tokio worker（091 性能优化）；其返回的错误消息直接转成 ApiResponse 短路返回。
    let prep = match tokio::task::spawn_blocking(prepare_upgrade_blocking).await {
        Ok(Ok(p)) => p,
        Ok(Err(msg)) => return ApiResponse::err(1, &msg),
        Err(e) => {
            tracing::error!("Self-update: upgrade task panicked: {}", e);
            return ApiResponse::err(1, &format!("npm upgrade failed: {}", e));
        }
    };
    // 分离式重部署：fork 后立即返回，不阻塞 handler。
    spawn_platform_redeploy(&prep.ntd_cmd, &prep.marker_cleanup_path);
    tracing::info!("Self-update: npm upgraded, forked child process. ntd path: {}", prep.ntd_cmd);
    let response = ApiResponse::ok(serde_json::json!({
        "status": "upgrade_started",
        "message": "升级流程已启动，服务即将重启",
    }));
    // 先把响应发出去再退出主进程：sleep 500ms 等 handler 返回，避免响应半途丢失。
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
