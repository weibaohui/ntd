//! 内置资源同步相关 API 处理器
//!
//! 提供从远程 Git 仓库同步专家、事项模板、Skills 等资源的能力。
//! 所有资源（experts、todos、skills）共用同一个仓库、同一个同步机制。

use axum::extract::{Query, State};
use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};
use crate::config::BundledSourceConfig;

use crate::handlers::{AppError, AppState};
use crate::models::ApiResponse;
use crate::git_sync;

/// 子目录类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Subdir {
    /// 全部资源
    #[default]
    All,
    /// 专家
    Experts,
    /// 事项模板
    Todos,
    /// Skills
    Skills,
}

impl Subdir {
    fn as_str(self) -> &'static str {
        match self {
            Subdir::All => "all",
            Subdir::Experts => "experts",
            Subdir::Todos => "todos",
            Subdir::Skills => "skills",
        }
    }
}

/// 同步请求参数
#[derive(Debug, Deserialize)]
pub struct SyncRequest {
    /// 同步策略：keep_local（保留本地）、overwrite（覆盖）、manual（手动处理冲突）
    #[serde(default = "default_sync_strategy")]
    pub strategy: String,
    /// 同步的子目录：all/experts/todos/skills，默认 all
    #[serde(default)]
    pub subdir: Subdir,
}

fn default_sync_strategy() -> String {
    "keep_local".to_string()
}

/// 同步响应
#[derive(Debug, Serialize)]
pub struct SyncResponse {
    /// 是否成功
    pub success: bool,
    /// 消息描述
    pub message: String,
    /// 是否是首次克隆
    pub is_first_clone: bool,
    /// 是否有更新
    pub has_updates: bool,
    /// 更新的文件数
    pub changed_files: usize,
    /// 同步的子目录
    pub subdir: String,
}

/// 状态查询参数
#[derive(Debug, Deserialize)]
pub struct StatusQuery {
    /// 子目录
    #[serde(default)]
    pub subdir: Subdir,
}

/// 状态响应
#[derive(Debug, Serialize)]
pub struct StatusResponse {
    /// 远程仓库地址
    pub remote_url: String,
    /// 当前分支
    pub branch: String,
    /// 本地路径
    pub local_path: String,
    /// 当前同步策略
    pub sync_strategy: String,
    /// 自动同步是否启用
    pub auto_sync_enabled: bool,
    /// 本地是否存在
    pub local_exists: bool,
    /// 本地 commit
    pub local_commit: Option<String>,
    /// 远程 commit
    pub remote_commit: Option<String>,
    /// 是否需要更新
    pub needs_update: Option<bool>,
    /// 上次同步时间
    pub last_sync_at: Option<String>,
    /// 子目录路径
    pub subdir: String,
    /// 子目录是否存在
    pub subdir_exists: bool,
    /// 子目录下文件数
    pub subdir_file_count: usize,
}

/// 手动触发同步
///
/// POST /api/bundled/sync
pub async fn sync_bundled(
    State(state): State<AppState>,
    Json(req): Json<SyncRequest>,
) -> Result<ApiResponse<SyncResponse>, AppError> {
    let cfg = state.config_clone();
    let bundled_config = &cfg.bundled_source;

    let repo_path = match git_sync::bundled_dir(&bundled_config.local_path) {
        Some(p) => p,
        None => return Err(AppError::BadRequest("无法获取 home 目录".to_string())),
    };

    let strategy = git_sync::SyncStrategy::from(req.strategy.as_str());
    let subdir = req.subdir;

    let result = if !repo_path.exists() || !repo_path.join(".git").exists() {
        if repo_path.exists() {
            tracing::info!("本地目录存在但不是 git 仓库，清理后重新克隆");
            if let Err(e) = tokio::fs::remove_dir_all(&repo_path).await {
                tracing::error!("清理旧目录失败: {}", e);
                return Err(AppError::Internal(format!("清理旧目录失败: {}", e)));
            }
        } else {
            tracing::info!("本地目录不存在，执行首次克隆");
        }
        git_sync::clone_repo(&bundled_config.url, &repo_path, &bundled_config.branch).await
    } else {
        tracing::info!("本地仓库已存在，执行同步更新");
        git_sync::sync_repo(&repo_path, "origin", &bundled_config.branch, strategy).await
    };

    match result {
        Ok(r) => {
            update_last_sync_time(&state).await;
            
            // 同步完成后，如果子目录是 todos，触发数据库导入
            if subdir == Subdir::Todos || subdir == Subdir::All {
                if let Err(e) = import_todo_templates_from_bundled(&state).await {
                    tracing::error!("从 bundled 导入事项模板失败: {}", e);
                }
            }
            
            // 同步完成后，如果子目录是 experts，触发专家重新加载
            if subdir == Subdir::Experts || subdir == Subdir::All {
                if let Err(e) = reload_experts_from_bundled(&state).await {
                    tracing::error!("从 bundled 重新加载专家失败: {}", e);
                }
            }
            
            Ok(ApiResponse::ok(SyncResponse {
                success: r.success,
                message: r.message,
                is_first_clone: r.is_first_clone,
                has_updates: r.has_updates,
                changed_files: r.changed_files,
                subdir: subdir.as_str().to_string(),
            }))
        }
        Err(e) => {
            tracing::error!("同步失败: {}", e);
            Err(AppError::Internal(format!("同步失败: {}", e)))
        }
    }
}

/// 获取同步状态
///
/// GET /api/bundled/status?subdir=experts
pub async fn get_bundled_status(
    State(state): State<AppState>,
    Query(query): Query<StatusQuery>,
) -> Result<ApiResponse<StatusResponse>, AppError> {
    let cfg = state.config_clone();
    let bundled_config = &cfg.bundled_source;

    let repo_path = match git_sync::bundled_dir(&bundled_config.local_path) {
        Some(p) => p,
        None => return Err(AppError::BadRequest("无法获取 home 目录".to_string())),
    };

    let local_exists = repo_path.exists();

    let local_commit = if local_exists {
        git_sync::get_current_commit(&repo_path).await.ok()
    } else {
        None
    };

    let remote_commit = if local_exists {
        git_sync::get_remote_commit(&repo_path, "origin", &bundled_config.branch).await.ok()
    } else {
        None
    };

    let needs_update = match (local_commit.as_ref(), remote_commit.as_ref()) {
        (Some(l), Some(r)) => Some(l != r),
        _ => None,
    };

    // 子目录信息
    let subdir_name = query.subdir.as_str();
    let subdir_path = if subdir_name == "all" {
        repo_path.clone()
    } else {
        repo_path.join(subdir_name)
    };
    let subdir_exists = subdir_path.exists();
    let subdir_file_count = if subdir_exists {
        count_files_in_dir(&subdir_path)
    } else {
        0
    };

    Ok(ApiResponse::ok(StatusResponse {
        remote_url: bundled_config.url.clone(),
        branch: bundled_config.branch.clone(),
        local_path: repo_path.to_string_lossy().to_string(),
        sync_strategy: "overwrite".to_string(),
        auto_sync_enabled: bundled_config.auto_sync_enabled,
        local_exists,
        local_commit,
        remote_commit,
        needs_update,
        last_sync_at: bundled_config.last_sync_at.clone(),
        subdir: subdir_name.to_string(),
        subdir_exists,
        subdir_file_count,
    }))
}

/// 统计目录下的文件数（递归）
fn count_files_in_dir(path: &std::path::Path) -> usize {
    if !path.exists() {
        return 0;
    }
    if path.is_file() {
        return 1;
    }
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            count += count_files_in_dir(&entry.path());
        }
    }
    count
}

/// 从 bundled/todos/ 导入事项模板到数据库
///
/// 扫描 bundled/todos/ 目录下的所有 yaml/json 文件，
/// 解析为事项模板并 upsert 到数据库（is_system = true）。
/// 文件名即为模板 ID。
async fn import_todo_templates_from_bundled(state: &AppState) -> Result<(), String> {
    let bundled_todos_dir = crate::git_sync::bundled_dir("bundled")
        .ok_or_else(|| "无法获取 home 目录".to_string())?
        .join("todos");

    if !bundled_todos_dir.exists() {
        tracing::info!("bundled/todos 目录不存在，跳过导入");
        return Ok(());
    }

    let entries = std::fs::read_dir(&bundled_todos_dir)
        .map_err(|e| format!("读取 bundled/todos 目录失败: {}", e))?;

    let mut imported_count = 0;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "yaml" && ext != "yml" && ext != "json" {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("读取文件失败 {}: {}", path.display(), e);
                continue;
            }
        };

        // 解析模板文件
        let template: Result<TemplateFile, String> = if ext == "json" {
            serde_json::from_str(&content).map_err(|e| e.to_string())
        } else {
            serde_yaml::from_str(&content).map_err(|e| e.to_string())
        };

        let template = match template {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("解析模板文件失败 {}: {}", path.display(), e);
                continue;
            }
        };

        // upsert 到数据库
        let template_id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown");
        let source_url = format!("bundled://todos/{}", path.file_name().and_then(|s| s.to_str()).unwrap_or(""));

        if let Err(e) = state.db.upsert_system_template(
            template_id,
            &template.title,
            template.prompt.as_deref(),
            &template.category,
            template.sort_order,
            &source_url,
            &now,
        ).await {
            tracing::warn!("保存模板 {} 失败: {}", template_id, e);
            continue;
        }

        imported_count += 1;
    }

    tracing::info!("从 bundled/todos 导入了 {} 个事项模板", imported_count);
    Ok(())
}

/// bundled 模板文件格式
#[derive(Debug, serde::Deserialize)]
struct TemplateFile {
    title: String,
    prompt: Option<String>,
    #[serde(default = "default_category")]
    category: String,
    #[serde(default)]
    sort_order: Option<i32>,
}

fn default_category() -> String {
    "general".to_string()
}

/// 重新加载专家
async fn reload_experts_from_bundled(state: &AppState) -> Result<(), String> {
    state.expert_manager.clear();
    
    if let Some(bundled_dir) = crate::expert::bundled_experts_dir() {
        if bundled_dir.exists() {
            let result = crate::expert::load_experts_from_directory(
                &bundled_dir,
                &state.expert_manager,
                crate::expert::ExpertSource::System,
            );
            tracing::info!("从 bundled 加载专家: {} 个", result.loaded_count);
        }
    }
    
    if let Some(user_dir) = crate::expert::experts_dir() {
        if user_dir.exists() {
            let result = crate::expert::load_experts_from_directory(
                &user_dir,
                &state.expert_manager,
                crate::expert::ExpertSource::User,
            );
            tracing::info!("从用户目录加载专家: {} 个", result.loaded_count);
        }
    }
    
    Ok(())
}

/// 获取当前配置
///
/// GET /api/bundled/config
pub async fn get_bundled_config(
    State(state): State<AppState>,
) -> Result<ApiResponse<BundledSourceConfig>, AppError> {
    let config = state.config_snapshot(|c| c.bundled_source.clone());
    Ok(ApiResponse::ok(config))
}

/// 更新配置
///
/// PUT /api/bundled/config
#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    /// 远程仓库地址
    pub url: Option<String>,
    /// 目标分支
    pub branch: Option<String>,
    /// 是否启用自动同步
    pub auto_sync_enabled: Option<bool>,
    /// 自动同步 cron 表达式
    pub auto_sync_cron: Option<String>,
}

pub async fn update_bundled_config(
    State(state): State<AppState>,
    Json(req): Json<UpdateConfigRequest>,
) -> Result<ApiResponse<BundledSourceConfig>, AppError> {
    let cfg = state.config_write_clone(|c| {
        if let Some(url) = &req.url {
            c.bundled_source.url = url.clone();
        }
        if let Some(branch) = &req.branch {
            c.bundled_source.branch = branch.clone();
        }
        if let Some(auto_sync_enabled) = req.auto_sync_enabled {
            c.bundled_source.auto_sync_enabled = auto_sync_enabled;
        }
        if let Some(cron) = &req.auto_sync_cron {
            c.bundled_source.auto_sync_cron = cron.clone();
        }
        c.clone()
    });

    if let Err(e) = cfg.save() {
        tracing::error!("保存配置失败: {}", e);
        return Err(AppError::Internal(format!("保存配置失败: {}", e)));
    }

    let config = state.config_snapshot(|c| c.bundled_source.clone());
    Ok(ApiResponse::ok(config))
}

/// 更新上次同步时间
async fn update_last_sync_time(state: &AppState) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let _ = state.config_write_clone(|c| {
        c.bundled_source.last_sync_at = Some(now.clone());
        c.clone()
    }).save();
}

/// 路由定义
pub fn bundled_routes() -> Router<AppState> {
    Router::new()
        .route("/api/bundled/sync", axum::routing::post(sync_bundled))
        .route("/api/bundled/status", axum::routing::get(get_bundled_status))
        .route("/api/bundled/config", axum::routing::get(get_bundled_config))
        .route("/api/bundled/config", axum::routing::put(update_bundled_config))
}
