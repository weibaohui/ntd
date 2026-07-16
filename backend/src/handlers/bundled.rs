//! 内置资源同步相关 API 处理器
//!
//! 提供从远程 Git 仓库同步专家、事项模板、Skills 等资源的能力。
//! 所有资源（experts、todos、skills）共用同一个仓库、同一个同步机制。

use axum::extract::{Path, Query, State};
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

/// 执行一次完整的内置资源同步：git clone/pull +（按 subdir）导入事项模板 + 重载专家 + 写 last_sync_at。
///
/// 抽自 sync_bundled handler，供 HTTP 接口与启动检查任务共用，避免逻辑分叉。
/// 返回 git 层 SyncResult，调用方据此组装 HTTP 响应或日志。
pub(crate) async fn run_bundled_sync(
    state: &AppState,
    subdir: Subdir,
    strategy: git_sync::SyncStrategy,
) -> Result<git_sync::SyncResult, String> {
    // 先快照出 owned bundled 配置并立即释放读锁卫，后续 .await 不持锁、future 保持 Send
    let bundled_config = state.config_snapshot(|c| c.bundled_source.clone());

    let repo_path = git_sync::bundled_dir(&bundled_config.local_path)
        .ok_or_else(|| "无法获取 home 目录".to_string())?;

    // 本地缺失或非合法 git 仓库 → 克隆；已存在 → 按策略同步更新
    let result = if !repo_path.exists() || !repo_path.join(".git").exists() {
        if repo_path.exists() {
            // 目录存在但不是 git 仓库，清理后重新克隆，避免脏目录卡住 sync
            tracing::info!("本地目录存在但不是 git 仓库，清理后重新克隆");
            tokio::fs::remove_dir_all(&repo_path)
                .await
                .map_err(|e| format!("清理旧目录失败: {}", e))?;
        } else {
            tracing::info!("本地目录不存在，执行首次克隆");
        }
        git_sync::clone_repo(&bundled_config.url, &repo_path, &bundled_config.branch).await
    } else {
        tracing::info!("本地仓库已存在，执行同步更新");
        git_sync::sync_repo(&repo_path, "origin", &bundled_config.branch, strategy).await
    };

    let r = result.map_err(|e| e.to_string())?;

    // 刷新 last_sync_at：供启动检查的冷却判断使用
    update_last_sync_time(state).await;

    // todos / all：把 bundled/todos 下的模板导入数据库（失败不阻断，仅记日志）
    if subdir == Subdir::Todos || subdir == Subdir::All {
        if let Err(e) = import_todo_templates_from_bundled(state).await {
            tracing::error!("从 bundled 导入事项模板失败: {}", e);
        }
    }

    // experts / all：重载专家索引（bundled 系统 + 用户自定义）
    if subdir == Subdir::Experts || subdir == Subdir::All {
        if let Err(e) = reload_experts_from_bundled(state).await {
            tracing::error!("从 bundled 重新加载专家失败: {}", e);
        }
    }

    // skills / all：扫描 bundled/skills 目录并记录日志（无需导入数据库，前端按需扫描）
    if subdir == Subdir::Skills || subdir == Subdir::All {
        let skills_dir = git_sync::bundled_dir("bundled")
            .map(|p| p.join("skills"));
        if let Some(dir) = &skills_dir {
            if dir.exists() {
                let count = std::fs::read_dir(dir)
                    .map(|entries| entries.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()).count())
                    .unwrap_or(0);
                tracing::info!("bundled/skills 目录就绪，包含 {} 个子目录", count);
            } else {
                tracing::info!("bundled/skills 目录不存在，跳过");
            }
        }
    }

    Ok(r)
}

/// 手动触发同步
///
/// POST /api/bundled/sync
pub async fn sync_bundled(
    State(state): State<AppState>,
    Json(req): Json<SyncRequest>,
) -> Result<ApiResponse<SyncResponse>, AppError> {
    let subdir = req.subdir;
    let strategy = git_sync::SyncStrategy::from(req.strategy.as_str());

    // 实际同步逻辑抽到 run_bundled_sync，handler 只负责参数解析与响应组装
    match run_bundled_sync(&state, subdir, strategy).await {
        Ok(r) => Ok(ApiResponse::ok(SyncResponse {
            success: r.success,
            message: r.message,
            is_first_clone: r.is_first_clone,
            has_updates: r.has_updates,
            changed_files: r.changed_files,
            subdir: subdir.as_str().to_string(),
        })),
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
        // 技能市场 API
        .route("/api/bundled/skills", axum::routing::get(list_bundled_skills))
        .route("/api/bundled/skills/{name}/content", axum::routing::get(get_bundled_skill_content))
        .route("/api/bundled/skills/install", axum::routing::post(install_bundled_skill))
}

// ---------------------------------------------------------------------------
// 技能市场 API
// ---------------------------------------------------------------------------

/// 技能来源元数据
///
/// 从 skills/{source}/metadata.json 读取的信息，
/// 描述一个来源仓库（如 mattpocock、anthropic、tiktok-video-skills）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSourceMeta {
    /// 来源标识（与目录名一致）
    pub name: String,
    /// 展示名称
    pub display_name: String,
    /// 来源描述
    pub description: String,
    /// GitHub 地址
    pub github_url: String,
    /// Star 数量
    pub stars: u64,
    /// 许可证
    pub license: Option<String>,
    /// 作者/组织
    pub author: Option<String>,
}

/// Bundled Skill 元数据
///
/// 从 ~/.ntd/bundled/skills/ 目录扫描得到的技能信息，
/// 用于前端「技能市场」展示。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundledSkillMeta {
    /// 完整路径名（如 awesome-skills-zh/lark-doc）
    pub name: String,
    /// 短名称（最后一段，如 lark-doc）
    pub short_name: String,
    /// 来源（第一段目录名，如 awesome-skills-zh）
    pub source: String,
    /// 来源元数据（如果存在 metadata.json）
    pub source_meta: Option<SkillSourceMeta>,
    /// 描述
    pub description: String,
    /// 中文描述（优先使用）
    pub description_zh: Option<String>,
    /// 版本号
    pub version: Option<String>,
    /// 作者
    pub author: Option<String>,
    /// 许可证
    pub license: Option<String>,
    /// 文件数
    pub file_count: u32,
    /// 总大小（字节）
    pub total_size: u64,
    /// 最后修改时间
    pub modified_at: Option<String>,
}

/// Bundled Skills 列表响应
#[derive(Debug, Serialize)]
pub struct BundledSkillsResponse {
    /// Skills 列表
    pub skills: Vec<BundledSkillMeta>,
    /// 来源分类信息（key 为 source 名称）
    pub sources: std::collections::HashMap<String, SkillSourceMeta>,
    /// 总数
    pub total: usize,
}

/// 安装技能请求
#[derive(Debug, Deserialize)]
pub struct InstallSkillRequest {
    /// 技能完整路径名（如 awesome-skills-zh/lark-doc）
    pub skill_name: String,
    /// 目标执行器（如 claudecode）
    pub executor: String,
}

/// Bundled Skill 内容响应
///
/// 返回 SKILL.md 的文本内容和目录下所有文件的列表，
/// 用于前端详情 Drawer 展示。
#[derive(Debug, Serialize)]
pub struct BundledSkillContentResponse {
    /// 技能名称
    pub skill_name: String,
    /// SKILL.md 文本内容
    pub content: String,
    /// 文件列表
    pub files: Vec<BundledSkillFile>,
}

/// Bundled Skill 文件信息
#[derive(Debug, Serialize)]
pub struct BundledSkillFile {
    /// 相对路径
    pub path: String,
    /// 文件大小（字节）
    pub size: u64,
}

/// 安装技能响应
#[derive(Debug, Serialize)]
pub struct InstallSkillResponse {
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: String,
    /// 目标路径
    pub target_path: String,
}

/// GET /api/bundled/skills - 列出技能市场中的所有技能
///
/// 扫描 ~/.ntd/bundled/skills/ 目录，返回所有可安装的技能。
/// 支持嵌套目录结构（如 awesome-skills-zh/lark-doc/SKILL.md）。
pub async fn list_bundled_skills(
    State(_state): State<AppState>,
) -> Result<ApiResponse<BundledSkillsResponse>, AppError> {
    // 在 spawn_blocking 中执行磁盘扫描，避免阻塞 tokio worker
    let result = tokio::task::spawn_blocking(move || {
        // 获取 bundled/skills 目录路径（仓库同步到 ~/.ntd/bundled/，skills 子目录在其中）
        let skills_dir = match git_sync::bundled_dir("bundled") {
            Some(p) => p.join("skills"),
            None => {
                // 目录不存在时返回空列表而非错误，让前端能正常渲染
                return BundledSkillsResponse {
                    skills: Vec::new(),
                    sources: std::collections::HashMap::new(),
                    total: 0,
                };
            }
        };

        // 目录不存在时返回空列表
        if !skills_dir.exists() {
            return BundledSkillsResponse {
                skills: Vec::new(),
                sources: std::collections::HashMap::new(),
                total: 0,
            };
        }

        // 递归扫描所有包含 SKILL.md 的目录
        let mut skills = Vec::new();
        collect_bundled_skills_recursive(&skills_dir, &skills_dir, &mut skills);

        // 按名称排序，保证输出顺序稳定
        skills.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        // 读取每个来源目录的 metadata.json
        let sources = collect_skill_sources(&skills_dir);

        // 为每个 skill 关联来源元数据
        for skill in &mut skills {
            if let Some(meta) = sources.get(&skill.source) {
                skill.source_meta = Some(meta.clone());
            }
        }

        let total = skills.len();
        BundledSkillsResponse { skills, sources, total }
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking join error: {}", e)))?;

    Ok(ApiResponse::ok(result))
}

/// 递归收集 bundled skills
///
/// 扫描目录结构，找出所有包含 SKILL.md 的子目录，
/// 解析 YAML frontmatter 获取元数据。
fn collect_bundled_skills_recursive(
    base_dir: &std::path::Path,
    current_dir: &std::path::Path,
    skills: &mut Vec<BundledSkillMeta>,
) {
    // 读取目录，失败时静默返回
    let entries = match std::fs::read_dir(current_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // 跳过非目录
        if !path.is_dir() {
            continue;
        }

        // 检查是否存在 SKILL.md
        let skill_md = path.join("SKILL.md");
        if skill_md.exists() {
            // 找到 skill，解析元数据
            if let Some(meta) = parse_bundled_skill_meta(&path, &skill_md, base_dir) {
                skills.push(meta);
            }
        } else {
            // 没有 SKILL.md，继续递归子目录（可能是来源目录或分类目录）
            collect_bundled_skills_recursive(base_dir, &path, skills);
        }
    }
}

/// 解析 bundled skill 元数据
///
/// 从 SKILL.md 的 YAML frontmatter 中提取信息，
/// 并从目录路径解析来源和短名称。
fn parse_bundled_skill_meta(
    skill_dir: &std::path::Path,
    skill_md: &std::path::Path,
    base_dir: &std::path::Path,
) -> Option<BundledSkillMeta> {
    // 读取 SKILL.md 内容
    let content = std::fs::read_to_string(skill_md).ok()?;

    // 提取 YAML frontmatter
    let yaml_str = extract_yaml_frontmatter(&content)?;
    let yaml = parse_yaml_value(&yaml_str);

    // 从相对路径解析完整名称和来源
    let rel_path = skill_dir.strip_prefix(base_dir).ok()?;
    let name = rel_path.to_string_lossy().to_string();

    // 分割路径获取来源（第一段）和短名称（最后一段）
    let parts: Vec<&str> = name.split('/').collect();
    let source = parts.first().map_or("unknown", |v| *v).to_string();
    let short_name = parts.last().map_or(name.as_str(), |v| *v).to_string();

    // 从 YAML 提取字段
    let description = yaml
        .get("description_zh")
        .and_then(|v| v.as_str())
        .or_else(|| yaml.get("description").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();

    let description_zh = yaml.get("description_zh").and_then(|v| v.as_str()).map(String::from);

    let version = yaml.get("version").and_then(|v| v.as_str()).map(String::from);
    let author = yaml.get("author").and_then(|v| v.as_str()).map(String::from);
    let license = yaml.get("license").and_then(|v| v.as_str()).map(String::from);

    // 统计文件数和大小
    let (file_count, total_size) = count_files_and_size(skill_dir);

    // 获取修改时间
    let modified_at = std::fs::metadata(skill_md).ok().and_then(|m| {
        m.modified().ok().and_then(|t| {
            let secs = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
            chrono::DateTime::from_timestamp(secs as i64, 0)
                .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        })
    });

    Some(BundledSkillMeta {
        name,
        short_name,
        source,
        source_meta: None,
        description,
        description_zh,
        version,
        author,
        license,
        file_count,
        total_size,
        modified_at,
    })
}

/// 从内容中提取 YAML frontmatter
///
/// 查找 --- 分隔符之间的内容，返回 YAML 字符串。
fn extract_yaml_frontmatter(content: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();

    // 首行必须是 ---
    if lines.first()?.trim() != "---" {
        return None;
    }

    // 查找结束的 ---
    let mut yaml_lines = Vec::new();
    for line in lines.iter().skip(1) {
        if line.trim() == "---" {
            break;
        }
        yaml_lines.push(*line);
    }

    Some(yaml_lines.join("\n"))
}

/// 解析 YAML 字符串为 serde_yaml::Value
fn parse_yaml_value(yaml_str: &str) -> serde_yaml::Value {
    if yaml_str.is_empty() {
        return serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
    }
    serde_yaml::from_str(yaml_str).unwrap_or(serde_yaml::Value::Mapping(serde_yaml::Mapping::new()))
}

/// 统计目录下的文件数和总大小
fn count_files_and_size(dir: &std::path::Path) -> (u32, u64) {
    let mut count = 0u32;
    let mut size = 0u64;

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    count += 1;
                    size += metadata.len();
                } else if metadata.is_dir() {
                    let (c, s) = count_files_and_size(&entry.path());
                    count += c;
                    size += s;
                }
            }
        }
    }

    (count, size)
}

/// GET /api/bundled/skills/:name/content - 获取技能的 SKILL.md 内容和文件列表
///
/// 读取 bundled/skills/{name}/SKILL.md 的文本内容，
/// 并列出目录下所有文件，用于前端详情 Drawer 展示。
pub async fn get_bundled_skill_content(
    State(_state): State<AppState>,
    Path(name): Path<String>,
) -> Result<ApiResponse<BundledSkillContentResponse>, AppError> {
    // skill 名称中的 / 在 URL 路径中会被自动解析，无需额外解码
    // axum 的 Path 提取器会正确处理编码后的路径段
    let decoded_name = name;

    let result = tokio::task::spawn_blocking(move || {
        // 定位 bundled/skills/{name}/SKILL.md
        let skill_dir = git_sync::bundled_dir("bundled")
            .ok_or_else(|| AppError::Internal("无法获取 home 目录".to_string()))?
            .join("skills")
            .join(&decoded_name);

        if !skill_dir.exists() || !skill_dir.is_dir() {
            return Err(AppError::BadRequest(format!(
                "技能 '{}' 不存在",
                decoded_name
            )));
        }

        // 读取 SKILL.md 内容（不存在时返回空字符串）
        let skill_md_path = skill_dir.join("SKILL.md");
        let content = std::fs::read_to_string(&skill_md_path).unwrap_or_default();

        // 递归收集文件列表
        let files = collect_files_list(&skill_dir, &skill_dir);

        Ok(BundledSkillContentResponse {
            skill_name: decoded_name,
            content,
            files,
        })
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking join error: {}", e)))??;

    Ok(ApiResponse::ok(result))
}

/// 收集所有来源目录的 metadata.json
///
/// 扫描 skills/ 下的一级子目录，读取 metadata.json，
/// 返回以 source 名称为 key 的 HashMap。
fn collect_skill_sources(skills_dir: &std::path::Path) -> std::collections::HashMap<String, SkillSourceMeta> {
    let mut sources = std::collections::HashMap::new();

    if let Ok(entries) = std::fs::read_dir(skills_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let metadata_path = path.join("metadata.json");
            if !metadata_path.exists() {
                continue;
            }

            if let Ok(content) = std::fs::read_to_string(&metadata_path) {
                if let Ok(mut meta) = serde_json::from_str::<SkillSourceMeta>(&content) {
                    // 确保 name 字段与目录名一致（防止手误）
                    if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                        meta.name = dir_name.to_string();
                    }
                    sources.insert(meta.name.clone(), meta);
                }
            }
        }
    }

    sources
}

/// 递归收集目录下的文件列表
///
/// 返回相对路径和文件大小。
fn collect_files_list(base_dir: &std::path::Path, current_dir: &std::path::Path) -> Vec<BundledSkillFile> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(current_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    let rel_path = path.strip_prefix(base_dir)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string();
                    files.push(BundledSkillFile {
                        path: rel_path,
                        size: metadata.len(),
                    });
                } else if metadata.is_dir() {
                    files.extend(collect_files_list(base_dir, &path));
                }
            }
        }
    }
    files
}

/// POST /api/bundled/skills/install - 安装技能到执行器
///
/// 将 bundled/skills/{skill_name} 目录复制到目标执行器的 skills 目录。
/// 如果目标已存在同名技能，返回冲突提示（不自动覆盖）。
pub async fn install_bundled_skill(
    State(_state): State<AppState>,
    Json(req): Json<InstallSkillRequest>,
) -> Result<ApiResponse<InstallSkillResponse>, AppError> {
    // 校验参数
    if req.skill_name.is_empty() {
        return Err(AppError::BadRequest("skill_name 不能为空".to_string()));
    }
    if req.executor.is_empty() {
        return Err(AppError::BadRequest("executor 不能为空".to_string()));
    }

    // 获取源目录路径（仓库同步到 ~/.ntd/bundled/，skills 子目录在其中）
    let source_dir = git_sync::bundled_dir("bundled")
        .ok_or_else(|| AppError::Internal("无法获取 home 目录".to_string()))?
        .join("skills")
        .join(&req.skill_name);

    // 校验源目录存在
    if !source_dir.exists() || !source_dir.is_dir() {
        return Err(AppError::BadRequest(format!(
            "技能 '{}' 不存在于市场中",
            req.skill_name
        )));
    }

    // 获取目标执行器的 skills 目录
    let target_skills_dir = super::skills::executor_skills_dir_str(&req.executor)
        .ok_or_else(|| AppError::BadRequest(format!("未知执行器: {}", req.executor)))?;

    // 提取短名称作为目标目录名（转成 owned String 以便传入 spawn_blocking）
    let short_name = req.skill_name.rsplit('/').next().unwrap_or(&req.skill_name).to_string();
    let target_dir = target_skills_dir.join(&short_name);

    // 在 spawn_blocking 中执行复制操作
    let executor = req.executor.clone();
    let result = tokio::task::spawn_blocking(move || {
        // 检查目标是否已存在
        if target_dir.exists() {
            return Err(AppError::BadRequest(format!(
                "执行器 '{}' 已存在技能 '{}'，是否覆盖？",
                executor, short_name
            )));
        }

        // 确保目标目录的父目录存在
        std::fs::create_dir_all(&target_skills_dir)
            .map_err(|e| AppError::Internal(format!("创建目标目录失败: {}", e)))?;

        // 复制目录
        copy_dir_all(&source_dir, &target_dir)
            .map_err(|e| AppError::Internal(format!("复制技能失败: {}", e)))?;

        Ok(InstallSkillResponse {
            success: true,
            message: format!("已安装 {} 到 {}", short_name, executor),
            target_path: target_dir.to_string_lossy().to_string(),
        })
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking join error: {}", e)))??;

    Ok(ApiResponse::ok(result))
}

/// 复制目录及所有内容
fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
