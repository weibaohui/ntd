//! 内置资源同步相关 API 处理器
//!
//! 提供从远程 Git 仓库同步专家、事项模板、Skills 等资源的能力。
//! 所有资源（experts、todos、skills）共用同一个仓库、同一个同步机制。

use axum::extract::{Path, Query, State};
use axum::Json;
use axum::Router;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use crate::config::BundledSourceConfig;

use crate::handlers::{AppError, AppState};
use crate::models::ApiResponse;
use crate::git_sync;
// 复用 skills 模块的工具：
// - SkillFileContentResponse：{ path, content } 响应结构，避免在本文件重复定义同构类型
// - resolve_skill_path_for_read：对 skill name 做字符串层安全校验，与 get_skill_file 同源，
//   拦截 `..`/绝对路径等，防止 name 经 URL 编码逃出 skills 根目录（详见 read_bundled_skill_file）
use crate::handlers::skills::{resolve_skill_path_for_read, SkillFileContentResponse};

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

// ---------------------------------------------------------------------------
// Skills 市场内存缓存
// ---------------------------------------------------------------------------

/// Skills 市场元数据缓存（启动时扫描一次，后续直接读内存）
///
/// 每次请求 `/api/bundled/skills` 都重新扫描磁盘会导致 10-20 秒延迟，
/// 用户体验很差。缓存后在内存中直接返回，延迟降到毫秒级。
///
/// 缓存在以下时机失效/刷新：
/// - `run_bundled_sync` 执行 git pull 后
/// - `install_bundled_skill` 安装新技能后
///
/// 使用 `parking_lot::RwLock` 而非 `tokio::sync::RwLock`：
/// 读多写少场景下 RwLock性能更好，且缓存更新发生在同步任务中（不在 async 上下文中）。
#[derive(Default)]
pub struct SkillsMarketCache {
    /// 缓存的技能列表（按名称排序）
    skills: RwLock<Vec<BundledSkillMeta>>,
    /// 缓存的来源分类信息
    sources: RwLock<HashMap<String, SkillSourceMeta>>,
    /// 缓存是否已初始化
    initialized: RwLock<bool>,
}

impl SkillsMarketCache {
    /// 从缓存读取，如果未初始化则返回 None（调用方应触发异步预热）
    pub fn get(&self) -> Option<BundledSkillsResponse> {
        if !*self.initialized.read() {
            return None;
        }
        let skills = self.skills.read().clone();
        let sources = self.sources.read().clone();
        let total = skills.len();
        // 缓存层返回全量快照，page/page_size 用占位值；
        // 真正的分页切片由 list_bundled_skills 里的 apply_pagination 完成。
        Some(BundledSkillsResponse {
            skills,
            sources,
            total,
            page: 1,
            page_size: 20,
        })
    }

    /// 更新缓存内容
    ///
    /// # 参数
    /// - `skills`: 技能列表（应已按名称排序）
    /// - `sources`: 来源分类信息
    pub fn update(&self, skills: Vec<BundledSkillMeta>, sources: HashMap<String, SkillSourceMeta>) {
        let mut skills_guard = self.skills.write();
        let mut sources_guard = self.sources.write();
        let mut initialized_guard = self.initialized.write();

        *skills_guard = skills;
        *sources_guard = sources;
        *initialized_guard = true;
    }

    /// 标记缓存为未初始化（强制重新扫描）
    pub fn invalidate(&self) {
        let mut initialized = self.initialized.write();
        *initialized = false;
    }

    /// 检查缓存是否已初始化
    pub fn is_initialized(&self) -> bool {
        *self.initialized.read()
    }
}

/// 异步预热缓存（在后台线程扫描磁盘）
///
/// 在 `spawn_blocking` 中执行磁盘 IO，避免阻塞 tokio worker 线程。
pub async fn warm_up_skills_cache(local_path: String) {
    let result = tokio::task::spawn_blocking(move || -> Option<(Vec<BundledSkillMeta>, HashMap<String, SkillSourceMeta>)> {
        // 获取 skills 目录路径
        let skills_dir = match git_sync::bundled_dir(&local_path) {
            Some(p) => p.join("skills"),
            None => return None,
        };

        if !skills_dir.exists() {
            return None;
        }

        // 递归扫描所有包含 SKILL.md 的目录
        let mut skills = Vec::new();
        collect_bundled_skills_recursive(&skills_dir, &skills_dir, &mut skills);

        // 按名称排序，保证输出顺序稳定
        skills.sort_by_key(|a| a.name.to_lowercase());

        // 读取每个来源目录的 metadata.json
        let sources = collect_skill_sources(&skills_dir);

        // 为每个 skill 关联来源元数据
        for skill in &mut skills {
            if let Some(meta) = sources.get(&skill.source) {
                skill.source_meta = Some(meta.clone());
            }
        }

        Some((skills, sources))
    })
    .await;

    if let Ok(Some((skills, sources))) = result {
        // 获取全局缓存实例并更新
        if let Some(cache) = get_global_skills_cache() {
            cache.update(skills, sources);
            tracing::debug!("Skills market cache warmed up successfully");
        }
    }
}

// 全局缓存实例（通过 Arc::new 共享到 AppState）
// 使用 Option<Arc<SkillsMarketCache>> 延迟初始化，避免循环初始化问题
use std::sync::OnceLock;
static GLOBAL_SKILLS_CACHE: OnceLock<Arc<SkillsMarketCache>> = OnceLock::new();

/// 获取全局 Skills 缓存实例
fn get_global_skills_cache() -> Option<&'static Arc<SkillsMarketCache>> {
    GLOBAL_SKILLS_CACHE.get()
}

/// 注册全局 Skills 缓存实例（在 AppState 构造时调用）
pub fn register_skills_cache(cache: Arc<SkillsMarketCache>) {
    let _ = GLOBAL_SKILLS_CACHE.set(cache);
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
    /// 环境中是否安装了 git —— bundled 资源同步的前置依赖（同步靠系统 git CLI 完成）。
    /// 单独暴露给前端：让前端在「模板管理」页就展示「未检测到 Git + 一键安装」入口，
    /// 而不是等到用户点「立即同步」、收到一个笼统的 500 错误之后才知道根因。
    pub git_available: bool,
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
        // local_path 来自上层快照的 bundled 配置，保证「克隆落点」与「读取来源」一致
        if let Err(e) = import_todo_templates_from_bundled(state, &bundled_config.local_path).await {
            tracing::error!("从 bundled 导入事项模板失败: {}", e);
        }
    }

    // experts / all：重载专家索引（bundled 系统 + 用户自定义）
    if subdir == Subdir::Experts || subdir == Subdir::All {
        if let Err(e) = reload_experts_from_bundled(state).await {
            tracing::error!("从 bundled 重新加载专家失败: {}", e);
        }
    }

    // skills / all：扫描 bundled/skills 目录并刷新缓存
    if subdir == Subdir::Skills || subdir == Subdir::All {
        // 与上方克隆路径同源：skills 子目录在配置的 local_path 下，而不是硬编码 ~/.ntd/bundled，
        // 否则用户改了 local_path 后，这里会去错误目录数子目录。
        let skills_dir = git_sync::bundled_dir(&bundled_config.local_path)
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

        // 同步后刷新 Skills 缓存，确保用户下次访问时拿到最新数据
        warm_up_skills_cache(bundled_config.local_path.clone()).await;
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
        // 直接同步调用 is_git_available：which::which("git") 只做一次 PATH 目录扫描，
        // 耗时在微秒级，不值得为此 spawn_blocking 占一个阻塞线程池槽位。
        // 与启动检查（services/startup_check.rs）用的是同一个探测函数，单一事实来源。
        git_available: git_sync::is_git_available(),
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
async fn import_todo_templates_from_bundled(state: &AppState, local_path: &str) -> Result<(), String> {
    // todos 目录与 skills 同源：都在配置的 local_path 下（默认 "bundled"），而非硬编码，
    // 否则用户改了 local_path 后，git 克隆落到别处、这里却仍去 ~/.ntd/bundled/todos 读不到。
    let bundled_todos_dir = crate::git_sync::bundled_dir(local_path)
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
        // 来源分页：与技能分页职责分离，按「来源」本身切片，
        // 来源网格据此渲染，避免按技能切片后派生来源导致每页来源数量稀少
        .route("/api/bundled/skill-sources", axum::routing::get(list_bundled_skill_sources))
        .route("/api/bundled/skills/{name}/content", axum::routing::get(get_bundled_skill_content))
        // 读单文件内容：与 content 同命名空间，让市场页文件浏览器能预览 SKILL.md 以外的文件
        .route("/api/bundled/skills/{name}/file", axum::routing::get(get_bundled_skill_file))
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

/// 带技能计数的来源视图
///
/// 来源分页接口专用：在 SkillSourceMeta 基础上附加 `skill_count`，
/// 让前端来源网格能直接显示「该来源下有多少技能」，
/// 而不必先拉全部技能再在前端按 source 分组计数。
#[derive(Debug, Clone, Serialize)]
pub struct SkillSourceWithCount {
    /// 来源元数据
    pub meta: SkillSourceMeta,
    /// 该来源下的技能数（过滤前计数，与来源网格展示语义一致）
    pub skill_count: usize,
}

/// 来源分页列表响应
///
/// 与 BundledSkillsResponse 职责分离：
/// - BundledSkillsResponse 按「技能」切片，用于「全部技能」模式
/// - BundledSkillSourcesResponse 按「来源」切片，用于「按来源浏览」来源网格
#[derive(Debug, Serialize)]
pub struct BundledSkillSourcesResponse {
    /// 当前页的来源列表（已分页切片）
    pub sources: Vec<SkillSourceWithCount>,
    /// 来源总数（过滤前），前端 Pagination 据此渲染页码
    pub total: usize,
    /// 当前页码（从 1 开始）
    pub page: u32,
    /// 每页大小
    pub page_size: u32,
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
///
/// 强制分页：page / page_size 始终有值，skills 是该页切片，
/// total 是「过滤前」的全量计数。
#[derive(Debug, Serialize)]
pub struct BundledSkillsResponse {
    /// Skills 列表（已应用分页切片）
    pub skills: Vec<BundledSkillMeta>,
    /// 来源分类信息（key 为 source 名称）
    pub sources: std::collections::HashMap<String, SkillSourceMeta>,
    /// 总数：始终是「过滤前」的全量技能数，前端据此渲染分页器
    pub total: usize,
    /// 当前页码（从 1 开始）
    pub page: u32,
    /// 每页大小
    pub page_size: u32,
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

/// 技能列表分页查询参数
///
/// 强制分页：page / page_size 都缺省时也按默认值（page=1, page_size=20）切片，
/// 绝不返回全量数据，避免一次把上千张技能卡片塞进响应。
///
/// 过滤参数（source / keyword）下沉到后端：先按它们过滤，再分页。
/// 这样 total 就是「过滤后」的计数，前端 Pagination 与实际可见技能一一对应。
#[derive(Debug, Deserialize)]
pub struct ListSkillsQuery {
    /// 页码，从 1 开始；缺省默认 1
    #[serde(default = "default_page")]
    pub page: u32,
    /// 每页数量，缺省默认 20；上限 200，避免恶意大请求把内存打爆
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    /// 来源筛选：传具体 source 名时只返回该来源的技能；
    /// 缺省（None）表示不按来源过滤
    #[serde(default)]
    pub source: Option<String>,
    /// 关键字筛选：不区分大小写匹配 name / short_name / description / description_zh；
    /// 缺省（None）表示不按关键字过滤
    #[serde(default)]
    pub keyword: Option<String>,
}

/// 默认页码：第 1 页
fn default_page() -> u32 {
    1
}

/// 默认每页数量：20 条
fn default_page_size() -> u32 {
    20
}

/// GET /api/bundled/skills - 列出技能市场中的所有技能
///
/// 优先从内存缓存返回（毫秒级），缓存未初始化时触发异步预热并等待结果。
/// 支持嵌套目录结构（如 awesome-skills-zh/lark-doc/SKILL.md）。
///
/// 强制分页：`?page=&page_size=` 缺省时按 `page=1, page_size=20` 返回；
/// 任何情况下都不会返回全量数据。
pub async fn list_bundled_skills(
    State(state): State<AppState>,
    Query(query): Query<ListSkillsQuery>,
) -> Result<ApiResponse<BundledSkillsResponse>, AppError> {
    // 先尝试从缓存读取（毫秒级响应）
    if let Some(cache) = get_global_skills_cache() {
        if let Some(response) = cache.get() {
            // 缓存返回的是全量数据，按 query 强制切片
            return Ok(ApiResponse::ok(apply_pagination(response, &query)));
        }
    }

    // 缓存未命中：快照 local_path 并触发异步预热
    let local_path = state.config_snapshot(|c| c.bundled_source.local_path.clone());

    // 预热缓存（后台线程扫描），完成后缓存自动更新
    warm_up_skills_cache(local_path.clone()).await;

    // 再次尝试从缓存读取
    if let Some(cache) = get_global_skills_cache() {
        if let Some(response) = cache.get() {
            return Ok(ApiResponse::ok(apply_pagination(response, &query)));
        }
    }

    // 兜底：同步扫描磁盘（极少发生，仅在缓存预热尚未完成时）
    let local_path_owned = local_path;
    let result = tokio::task::spawn_blocking(move || {
        // 获取 skills 目录路径（仓库同步到 ~/.ntd/{local_path}/，skills 子目录在其中）
        let skills_dir = match git_sync::bundled_dir(&local_path_owned) {
            Some(p) => p.join("skills"),
            None => {
                // 目录不存在时返回空列表而非错误，让前端能正常渲染
                return BundledSkillsResponse {
                    skills: Vec::new(),
                    sources: std::collections::HashMap::new(),
                    total: 0,
                    page: 1,
                    page_size: 20,
                };
            }
        };

        // 目录不存在时返回空列表
        if !skills_dir.exists() {
            return BundledSkillsResponse {
                skills: Vec::new(),
                sources: std::collections::HashMap::new(),
                total: 0,
                page: 1,
                page_size: 20,
            };
        }

        // 递归扫描所有包含 SKILL.md 的目录
        let mut skills = Vec::new();
        collect_bundled_skills_recursive(&skills_dir, &skills_dir, &mut skills);

        // 按名称排序，保证输出顺序稳定；sort_by_key 等价于按小写名升序，clippy 偏好此写法
        skills.sort_by_key(|a| a.name.to_lowercase());

        // 读取每个来源目录的 metadata.json
        let sources = collect_skill_sources(&skills_dir);

        // 为每个 skill 关联来源元数据
        for skill in &mut skills {
            if let Some(meta) = sources.get(&skill.source) {
                skill.source_meta = Some(meta.clone());
            }
        }

        let total = skills.len();
        BundledSkillsResponse {
            skills,
            sources,
            total,
            page: 1,
            page_size: 20,
        }
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking join error: {}", e)))?;

    // 同样对兜底结果应用分页，再交给调用方
    Ok(ApiResponse::ok(apply_pagination(result, &query)))
}

/// 分页切片上限：超过该值的 page_size 会被强制压回，避免一次拉太多拖慢响应
const MAX_PAGE_SIZE: u32 = 200;

/// 在全量响应之上应用过滤 + 分页参数
///
/// 处理顺序（关键）：先按 source / keyword 过滤，再分页。
/// 只有这样 `total` 才是「过滤后」的计数，前端 Pagination 才能正确渲染页码。
///
/// 设计取舍：
/// - 缓存里存的是全量技能，过滤 + 分页都在内存里完成，避免每次都重新扫磁盘。
/// - 始终切片，不再有「不分页」分支；page_size 钳到 [1, MAX_PAGE_SIZE]。
/// - page 为 0 视为非法，统一回退到 1。
/// - page 超出范围时返回空列表而非报错，前端 Pagination 组件能正确处理 total。
fn apply_pagination(
    mut response: BundledSkillsResponse,
    query: &ListSkillsQuery,
) -> BundledSkillsResponse {
    // 1) 先过滤：把 source / keyword 不匹配的技能剔除
    apply_filter(&mut response, query);

    // 2) 再分页：page_size 钳到 [1, MAX_PAGE_SIZE]；page 为 0 视为非法，统一回退到 1
    let page_size = query.page_size.clamp(1, MAX_PAGE_SIZE);
    let page = if query.page == 0 { 1 } else { query.page };

    // total 是「过滤后」的计数，前端据此渲染分页器
    let total = response.skills.len();
    let start = (page as usize).saturating_sub(1) * page_size as usize;
    // start 越界时 drain 返回空，正好对应「翻到末页之后」的语义
    let end = start.saturating_add(page_size as usize).min(total);

    let paged: Vec<BundledSkillMeta> = response.skills.drain(start..end).collect();
    response.skills = paged;
    response.total = total;
    response.page = page;
    response.page_size = page_size;
    response
}

/// 按 source / keyword 过滤技能列表（原地修改）
///
/// - source：精确匹配 skill.source；None 表示不按来源过滤
/// - keyword：不区分大小写匹配 name / short_name / description / description_zh；
///   None 或空串表示不按关键字过滤
///
/// 过滤后 response.total 由调用方（apply_pagination）重算，
/// 这里只负责把 skills 收窄到「过滤后」的子集。
fn apply_filter(response: &mut BundledSkillsResponse, query: &ListSkillsQuery) {
    // 关键字预处理：trim 后转小写，空串视为不过滤
    let keyword = query
        .keyword
        .as_ref()
        .map(|k| k.trim().to_lowercase())
        .filter(|k| !k.is_empty());

    // 用 retain 原地过滤，避免中间 Vec 分配；
    // 两个条件都满足才保留，缺省的条件视为「通过」
    response.skills.retain(|skill| {
        // 来源过滤：query.source 缺省或匹配 skill.source 时通过
        let source_pass = query
            .source
            .as_ref()
            .map_or(true, |s| s == &skill.source);

        // 关键字过滤：query.keyword 缺省或匹配任一文本字段时通过
        let keyword_pass = keyword.as_ref().map_or(true, |kw| {
            skill.name.to_lowercase().contains(kw)
                || skill.short_name.to_lowercase().contains(kw)
                || skill.description.to_lowercase().contains(kw)
                || skill
                    .description_zh
                    .as_ref()
                    .is_some_and(|d| d.to_lowercase().contains(kw))
        });

        source_pass && keyword_pass
    });
}

/// 来源分页查询参数
///
/// 来源网格按「来源」翻页；keyword 用于「来源内搜索」场景，
/// 不传则返回全部来源（再分页）。
#[derive(Debug, Deserialize)]
pub struct ListSkillSourcesQuery {
    /// 页码，从 1 开始；缺省默认 1
    #[serde(default = "default_page")]
    pub page: u32,
    /// 每页数量，缺省默认 20；上限 200
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    /// 来源关键字筛选：不区分大小写匹配 name / display_name / description；
    /// 缺省（None）表示不按关键字过滤
    #[serde(default)]
    pub keyword: Option<String>,
}

/// GET /api/bundled/skill-sources - 列出技能来源（分页）
///
/// 与 `/api/bundled/skills` 职责分离：
/// - `/skills` 按「技能」切片，用于「全部技能」模式
/// - `/skill-sources` 按「来源」切片，用于「按来源浏览」来源网格
///
/// 返回每个来源的 `skill_count`（过滤前计数），前端来源卡片据此显示数量。
pub async fn list_bundled_skill_sources(
    State(state): State<AppState>,
    Query(query): Query<ListSkillSourcesQuery>,
) -> Result<ApiResponse<BundledSkillSourcesResponse>, AppError> {
    // 先尝试从缓存读取全量技能（缓存里 skills 已是全量）
    let cached: Option<BundledSkillsResponse> = if let Some(cache) = get_global_skills_cache() {
        cache.get()
    } else {
        None
    };

    // 缓存未命中：触发预热后再读一次
    let cached = match cached {
        Some(c) => Some(c),
        None => {
            let local_path = state.config_snapshot(|c| c.bundled_source.local_path.clone());
            warm_up_skills_cache(local_path.clone()).await;
            if let Some(cache) = get_global_skills_cache() {
                cache.get()
            } else {
                None
            }
        }
    };

    // 基于全量技能构造来源列表（含每个来源的技能数），再分页
    let response = match cached {
        Some(full) => build_sources_response(full, &query),
        // 缓存彻底不可用：返回空响应，让前端走「暂无技能来源」分支
        None => BundledSkillSourcesResponse {
            sources: Vec::new(),
            total: 0,
            page: 1,
            page_size: 20,
        },
    };

    Ok(ApiResponse::ok(response))
}

/// 基于全量技能响应构造来源分页响应
///
/// 步骤：
/// 1. 按 keyword 过滤来源（匹配 name / display_name / description）
/// 2. 按 source 字母序排序，保证分页顺序稳定
/// 3. 按页切片
///
/// `skill_count` 是「过滤前」每个来源的真实技能数，
/// 与来源网格展示「该来源下有多少技能」的语义一致。
fn build_sources_response(
    full: BundledSkillsResponse,
    query: &ListSkillSourcesQuery,
) -> BundledSkillSourcesResponse {
    // 关键字预处理：trim 后转小写，空串视为不过滤
    let keyword = query
        .keyword
        .as_ref()
        .map(|k| k.trim().to_lowercase())
        .filter(|k| !k.is_empty());

    // 先统计每个来源的技能数（用全量 skills，不被 keyword 影响）
    let mut count_by_source: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for skill in &full.skills {
        *count_by_source.entry(skill.source.clone()).or_insert(0) += 1;
    }

    // 把 sources HashMap 转成 Vec<SkillSourceWithCount>，并按 keyword 过滤
    let mut all_sources: Vec<SkillSourceWithCount> = full
        .sources
        .into_values()
        .map(|meta| {
            let skill_count = count_by_source.get(&meta.name).copied().unwrap_or(0);
            SkillSourceWithCount { meta, skill_count }
        })
        .filter(|src| {
            // keyword 缺省时全部保留
            keyword.as_ref().map_or(true, |kw| {
                src.meta.name.to_lowercase().contains(kw)
                    || src.meta.display_name.to_lowercase().contains(kw)
                    || src.meta.description.to_lowercase().contains(kw)
            })
        })
        .collect();

    // 按来源名排序，保证分页顺序稳定
    all_sources.sort_by(|a, b| a.meta.name.cmp(&b.meta.name));

    // total 是「过滤后」的来源数，前端 Pagination 据此渲染
    let total = all_sources.len();

    // 分页切片
    let page_size = query.page_size.clamp(1, MAX_PAGE_SIZE);
    let page = if query.page == 0 { 1 } else { query.page };
    let start = (page as usize).saturating_sub(1) * page_size as usize;
    let end = start.saturating_add(page_size as usize).min(total);
    let paged: Vec<SkillSourceWithCount> = all_sources.drain(start..end).collect();

    BundledSkillSourcesResponse {
        sources: paged,
        total,
        page,
        page_size,
    }
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
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<ApiResponse<BundledSkillContentResponse>, AppError> {
    // skill 名称中的 / 在 URL 路径中会被自动解析，无需额外解码
    // axum 的 Path 提取器会正确处理编码后的路径段
    let decoded_name = name;
    // 与 list_bundled_skills 同源：skills 根目录取自配置而非硬编码，
    // 保证「扫描出来的列表」与「能读到内容的目录」是同一个。
    let local_path = state.config_snapshot(|c| c.bundled_source.local_path.clone());

    let result = tokio::task::spawn_blocking(move || {
        // 定位 {local_path}/skills/{name}/SKILL.md
        let skill_dir = git_sync::bundled_dir(&local_path)
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

/// 单文件内容读取的 query 参数
#[derive(Debug, Deserialize)]
pub struct BundledSkillFileQuery {
    /// 目标文件相对技能目录的路径，如 `references/guide.md`；含子目录用 `/` 分隔
    pub path: String,
}

/// GET /api/bundled/skills/{name}/file - 读取 bundled 技能内某个文件的内容
///
/// 市场页文件浏览器预览非 SKILL.md 文件时调用。磁盘 IO 下放到 `read_bundled_skill_file`
/// 并整体塞进 `spawn_blocking`，避免 read_to_string 阻塞 tokio reactor worker。
pub async fn get_bundled_skill_file(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<BundledSkillFileQuery>,
) -> Result<ApiResponse<SkillFileContentResponse>, AppError> {
    // 快照 local_path 后立刻释放读锁，再 move 进阻塞任务，保证 future 为 Send
    let local_path = state.config_snapshot(|c| c.bundled_source.local_path.clone());
    // name 含 `/`（如 src1/lark-doc），由前端 encodeURIComponent 编码、axum Path 解码，
    // 与 get_bundled_skill_content 同一处理方式，这里无需二次解码
    let result = tokio::task::spawn_blocking(move || read_bundled_skill_file(&local_path, &name, &query.path))
        .await
        .map_err(|e| AppError::Internal(format!("spawn_blocking join error: {}", e)))??;
    Ok(ApiResponse::ok(result))
}

/// 在阻塞任务中读取 bundled 技能的单个文件内容。
///
/// skill_dir 的定位与 content 接口同源（`{local_path}/skills/{name}`），
/// 文件路径经 `resolve_file_under_skill` 做安全校验，防止 `..`/符号链接逃逸出技能目录。
fn read_bundled_skill_file(
    local_path: &str,
    name: &str,
    rel_path: &str,
) -> Result<SkillFileContentResponse, AppError> {
    // skills 根目录取自配置，与扫描/安装同源；自定义 local_path 时才能定位到正确目录
    let skills_root = git_sync::bundled_dir(local_path)
        .ok_or_else(|| AppError::Internal("无法获取 home 目录".to_string()))?
        .join("skills");

    // name 必须先校验再 join：axum 会对 URL 路径段内的 %2F 解码，攻击者能把 `..%2F..` 编码进
    // {name} 段、绕过路由层的 `..` 折叠。复用 get_skill_file 同款校验（字符串层拒绝空/绝对路径/
    // `..`/Windows 盘符），否则下方 resolve_file_under_skill 的前缀检查会和「已逃逸的 skill_dir」
    // 比较而彻底失效——配合干净的 rel_path 即可读取系统任意文件。
    // 校验器内部约定：不合法返回 BadRequest；技能不存在返回 NotFound（与 get_skill_file 一致）
    let skill_dir = resolve_skill_path_for_read(&skills_root, name)?;

    // resolve_skill_path_for_read 只判存在不判类型，补一道目录校验：
    // name 命中普通文件时（理论上 bundled skills 不会出现）也一并拦下
    if !skill_dir.is_dir() {
        return Err(AppError::NotFound);
    }

    // 安全校验返回 canonicalize 后的绝对路径；文件不存在时内部已转 NotFound
    let file_path = resolve_file_under_skill(&skill_dir, rel_path)?;
    // canonicalize 对目录也会成功，这里补一道 is_file，拦截「请求了一个目录」的情况
    if !file_path.is_file() {
        return Err(AppError::NotFound);
    }

    // skill 资源均为文本（md/json/js/ts 等），用 read_to_string 即可，与 get_skill_file 一致；
    // 真遇到二进制会因无效 UTF-8 报错，统一转 500，由前端预览区显示「无法加载」占位
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| AppError::Internal(format!("读取文件失败: {}", e)))?;

    Ok(SkillFileContentResponse {
        path: rel_path.to_string(),
        content,
    })
}

/// 校验 rel_path 位于 skill_dir 之内，返回 canonicalize 后的绝对文件路径。
///
/// 双重防护缺一不可：
/// - 字符串层先拒绝空/绝对路径/`..`，把绝大多数恶意输入挡在文件系统访问之前；
/// - canonicalize 后再做前缀检查，兜住「合法相对路径 + 符号链接」绕出到目录外的情形。
fn resolve_file_under_skill(
    skill_dir: &std::path::Path,
    rel_path: &str,
) -> Result<std::path::PathBuf, AppError> {
    let rel = std::path::Path::new(rel_path);
    // 空路径会让 join 退化为 skill_dir 本身，必须先拦
    if rel.as_os_str().is_empty() {
        return Err(AppError::BadRequest("文件路径不能为空".to_string()));
    }
    // 绝对路径会无视 skill_dir 直接指向任意位置，禁止
    if rel.is_absolute() {
        return Err(AppError::BadRequest("不允许使用绝对路径".to_string()));
    }
    // 拒绝 `..`（父级遍历）与 Windows 盘符前缀，二者都能逃出技能目录
    if rel.components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::Prefix(_))) {
        return Err(AppError::BadRequest("不允许的文件路径".to_string()));
    }

    let file_path = skill_dir.join(rel);
    // canonicalize 会解析符号链接并要求文件真实存在；不存在归 404，让前端走「文件没了」分支
    let file_canon = file_path.canonicalize().map_err(|_| AppError::NotFound)?;
    let dir_canon = skill_dir
        .canonicalize()
        .map_err(|e| AppError::Internal(format!("解析技能目录失败: {}", e)))?;
    // 最终保险：解析后的绝对路径必须仍落在技能目录之内（拦符号链接逃逸）
    if !file_canon.starts_with(&dir_canon) {
        return Err(AppError::BadRequest("文件路径超出技能目录".to_string()));
    }

    Ok(file_canon)
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
    State(state): State<AppState>,
    Json(req): Json<InstallSkillRequest>,
) -> Result<ApiResponse<InstallSkillResponse>, AppError> {
    // 校验参数
    if req.skill_name.is_empty() {
        return Err(AppError::BadRequest("skill_name 不能为空".to_string()));
    }
    if req.executor.is_empty() {
        return Err(AppError::BadRequest("executor 不能为空".to_string()));
    }

    // skills 根目录取自配置 bundled_source.local_path，与扫描/读取同源；
    // 否则自定义 local_path 时会从不存在的 ~/.ntd/bundled 安装，源目录永远找不到。
    let local_path = state.config_snapshot(|c| c.bundled_source.local_path.clone());
    // 获取源目录路径（仓库同步到 ~/.ntd/{local_path}/，skills 子目录在其中）
    let source_dir = git_sync::bundled_dir(&local_path)
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

    // 安装成功后刷新 Skills 缓存（确保下次访问时列表是最新的）
    warm_up_skills_cache(local_path).await;

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

// ===========================================================================
// 单元测试
// ---------------------------------------------------------------------------
// 三个公开 handler（list/get_content/install）是 spawn_blocking + 磁盘扫描的薄封装，
// 真正的逻辑都落在上面这些私有辅助函数里。这里按 CLAUDE.md「私有辅助函数如逻辑复杂
// 也建议测试」的要求，逐个覆盖辅助函数的正常/边界/错误路径。handler 本身依赖
// git_sync::bundled_dir（读取真实 ~/.ntd），属于集成测试范畴，不在本模块内重放。
// ===========================================================================
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    /// 把文本写入 dir 下的相对路径 rel（自动补齐父目录），省去每个用例重复 mkdir+write。
    /// 返回写入后的绝对路径，便于调用方继续断言。
    fn write_rel(dir: &std::path::Path, rel: &str, content: &str) -> std::path::PathBuf {
        let p = dir.join(rel);
        // 相对路径可能带多级目录，先确保父目录存在再写文件
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&p, content).expect("写入测试文件失败");
        p
    }

    // ---- extract_yaml_frontmatter：frontmatter 提取的边界全在这里 ----

    /// 标准结构：首行 `---` + 若干 yaml + 闭合 `---`，应只返回中间 yaml 段。
    #[test]
    fn test_extract_yaml_frontmatter_normal() {
        let content = "---\nname: demo\ndescription: hi\n---\n# body\n";
        assert_eq!(extract_yaml_frontmatter(content).as_deref(), Some("name: demo\ndescription: hi"));
    }

    /// 首行不是 `---` 视为没有 frontmatter，返回 None——区分「无 frontmatter 的纯 Markdown」。
    #[test]
    fn test_extract_yaml_frontmatter_missing_open_delimiter() {
        assert!(extract_yaml_frontmatter("# just a title\n").is_none());
    }

    /// 空字符串没有首行，first()? 提前返回 None。
    #[test]
    fn test_extract_yaml_frontmatter_empty_content() {
        assert!(extract_yaml_frontmatter("").is_none());
    }

    /// 只有起始 `---`、没有闭合行时不会死循环，把后续行原样返回（锁定当前契约）。
    #[test]
    fn test_extract_yaml_frontmatter_no_closing_delimiter() {
        assert_eq!(extract_yaml_frontmatter("---\nk: v\n").as_deref(), Some("k: v"));
    }

    // ---- parse_yaml_value：空/合法/非法三态 ----

    /// 空串走兜底分支，返回空 Mapping（而非 None），让后续 .get() 安全返回 None。
    #[test]
    fn test_parse_yaml_value_empty_returns_mapping() {
        let v = parse_yaml_value("");
        assert!(v.is_mapping(), "空输入应回退为空 Mapping");
        assert!(v.as_mapping().map(|m| m.is_empty()).unwrap_or(false));
    }

    /// 合法 yaml 能被正常解析成可索引的 mapping。
    #[test]
    fn test_parse_yaml_value_valid() {
        let v = parse_yaml_value("k: v");
        assert_eq!(v.get("k").and_then(|x| x.as_str()), Some("v"));
    }

    /// 非法 yaml（这里用 tab 缩进，YAML 规范明令禁止）走兜底，返回空 Mapping 而非 panic。
    #[test]
    fn test_parse_yaml_value_invalid_returns_empty_mapping() {
        let v = parse_yaml_value("a:\n\tb: c\n");
        assert!(v.as_mapping().map(|m| m.is_empty()).unwrap_or(false), "非法 yaml 应回退空 Mapping");
    }

    // ---- count_files_and_size：递归统计 ----

    /// 空目录计数为 0、大小为 0。
    #[test]
    fn test_count_files_and_size_empty_dir() {
        let dir = tempfile::tempdir().expect("创建 tempdir 失败");
        assert_eq!(count_files_and_size(dir.path()), (0, 0));
    }

    /// 递归累加文件数与字节数；嵌套子目录里的文件也要算进去。
    #[test]
    fn test_count_files_and_size_counts_recursively() {
        let dir = tempfile::tempdir().expect("创建 tempdir 失败");
        // 两个顶层文件 + 一个子目录里的文件，共 3 个文件
        write_rel(dir.path(), "a.txt", "aaaa");
        write_rel(dir.path(), "b.txt", "bb");
        write_rel(dir.path(), "sub/c.txt", "cccccc");

        let (count, size) = count_files_and_size(dir.path());
        assert_eq!(count, 3, "应递归统计到 3 个文件");
        assert_eq!(size, 4 + 2 + 6, "大小应为各文件字节数之和");
    }

    /// 不存在的目录 read_dir 失败，返回 (0,0) 而非报错——支撑 status 接口的容忍语义。
    #[test]
    fn test_count_files_and_size_nonexistent_returns_zero() {
        let dir = tempfile::tempdir().expect("创建 tempdir 失败");
        let missing = dir.path().join("no-such-dir");
        assert_eq!(count_files_and_size(&missing), (0, 0));
    }

    // ---- collect_files_list：相对路径文件清单 ----

    /// 扁平文件以相对 base_dir 的路径收集。
    #[test]
    fn test_collect_files_list_flat() {
        let dir = tempfile::tempdir().expect("创建 tempdir 失败");
        write_rel(dir.path(), "SKILL.md", "x");
        write_rel(dir.path(), "a.md", "yy");

        let mut paths: Vec<String> = collect_files_list(dir.path(), dir.path())
            .into_iter()
            .map(|f| f.path)
            .collect();
        paths.sort();
        assert_eq!(paths, vec!["SKILL.md".to_string(), "a.md".to_string()]);
    }

    /// 嵌套目录下的文件路径要保留中间层级（如 sub/inner.md），不能被压平。
    #[test]
    fn test_collect_files_list_nested() {
        let dir = tempfile::tempdir().expect("创建 tempdir 失败");
        write_rel(dir.path(), "sub/inner.md", "z");

        let files = collect_files_list(dir.path(), dir.path());
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "sub/inner.md");
        assert_eq!(files[0].size, 1);
    }

    // ---- copy_dir_all：整目录复制 ----

    /// 复制后结构一致、文件内容不丢；这是 install 接口的核心动作，必须可信赖。
    #[test]
    fn test_copy_dir_all_preserves_structure_and_content() {
        let src = tempfile::tempdir().expect("创建 src tempdir 失败");
        let dst = tempfile::tempdir().expect("创建 dst tempdir 失败");
        write_rel(src.path(), "SKILL.md", "body");
        write_rel(src.path(), "refs/note.md", "nested");

        let target = dst.path().join("copied");
        copy_dir_all(src.path(), &target).expect("复制应成功");

        // 校验结构：两处文件都在，内容与源一致
        assert_eq!(std::fs::read_to_string(target.join("SKILL.md")).unwrap(), "body");
        assert_eq!(std::fs::read_to_string(target.join("refs/note.md")).unwrap(), "nested");
    }

    // ---- collect_skill_sources：来源 metadata.json 解析 ----

    /// 写一份完整 metadata.json，应被解析进 map，且 name 被目录名覆盖（防手误）。
    #[test]
    fn test_collect_skill_sources_reads_metadata_and_overrides_name() {
        let dir = tempfile::tempdir().expect("创建 tempdir 失败");
        // json 里 name 故意写错，期望被目录名 src1 覆盖
        let json = r#"{
            "name": "wrong",
            "display_name": "Src One",
            "description": "d",
            "github_url": "https://example.com/x",
            "stars": 10,
            "license": "MIT",
            "author": "someone"
        }"#;
        write_rel(dir.path(), "src1/metadata.json", json);

        let sources = collect_skill_sources(dir.path());
        let meta = sources.get("src1").expect("应以目录名 src1 为 key");
        assert_eq!(meta.name, "src1", "name 应被目录名覆盖");
        assert_eq!(meta.display_name, "Src One");
        assert_eq!(meta.stars, 10);
    }

    /// 没有 metadata.json 的来源目录应被跳过，不出现在 map 里。
    #[test]
    fn test_collect_skill_sources_skips_dir_without_metadata() {
        let dir = tempfile::tempdir().expect("创建 tempdir 失败");
        write_rel(dir.path(), "src1/SKILL.md", "x");
        let sources = collect_skill_sources(dir.path());
        assert!(sources.is_empty(), "缺少 metadata.json 的目录不应进 sources");
    }

    /// skills_dir 下的普通文件（非目录）必须被忽略，避免误当来源解析。
    #[test]
    fn test_collect_skill_sources_ignores_plain_files() {
        let dir = tempfile::tempdir().expect("创建 tempdir 失败");
        write_rel(dir.path(), "README.md", "not a source");
        let sources = collect_skill_sources(dir.path());
        assert!(sources.is_empty(), "普通文件不应被当作来源");
    }

    // ---- parse_bundled_skill_meta：从 SKILL.md 抽元数据 ----

    /// 带完整 frontmatter 的技能：description_zh 优先填入 description，其余字段对齐解析。
    #[test]
    fn test_parse_bundled_skill_meta_parses_all_fields() {
        let base = tempfile::tempdir().expect("创建 base tempdir 失败");
        let skill_dir = write_rel(base.path(), "src1/lark-doc/SKILL.md", "---\nname: lark\ndescription: en\ndescription_zh: 中文描述\nversion: 1.2.0\nauthor: me\nlicense: MIT\n---\n# body\n");
        let meta = parse_bundled_skill_meta(skill_dir.parent().unwrap(), &skill_dir, base.path())
            .expect("合法 SKILL.md 应解析出 meta");

        // description_zh 存在时优先用作 description，让前端默认展示中文
        assert_eq!(meta.description, "中文描述");
        assert_eq!(meta.description_zh.as_deref(), Some("中文描述"));
        assert_eq!(meta.version.as_deref(), Some("1.2.0"));
        assert_eq!(meta.author.as_deref(), Some("me"));
        assert_eq!(meta.license.as_deref(), Some("MIT"));
        // 路径切分：source 取首段、short_name 取末段
        assert_eq!(meta.source, "src1");
        assert_eq!(meta.short_name, "lark-doc");
        assert_eq!(meta.name, "src1/lark-doc");
        // 刚写入的文件至少有 SKILL.md 一个文件、mtime 可读
        assert_eq!(meta.file_count, 1);
        assert!(meta.modified_at.is_some());
        // source_meta 在递归阶段恒为 None，由 handler 后续关联
        assert!(meta.source_meta.is_none());
    }

    /// 只有 description（无 description_zh）：回退用 description，description_zh 字段留 None。
    #[test]
    fn test_parse_bundled_skill_meta_description_fallback() {
        let base = tempfile::tempdir().expect("创建 base tempdir 失败");
        let skill_dir = write_rel(base.path(), "src1/a/SKILL.md", "---\ndescription: only-en\n---\n");
        let meta = parse_bundled_skill_meta(skill_dir.parent().unwrap(), &skill_dir, base.path())
            .expect("应解析出 meta");
        assert_eq!(meta.description, "only-en");
        assert!(meta.description_zh.is_none());
    }

    /// SKILL.md 没有 frontmatter（首行非 `---`）→ 返回 None，调用方据此跳过该目录。
    #[test]
    fn test_parse_bundled_skill_meta_no_frontmatter_returns_none() {
        let base = tempfile::tempdir().expect("创建 base tempdir 失败");
        let skill_dir = write_rel(base.path(), "src1/a/SKILL.md", "# just a title\nno frontmatter\n");
        let meta = parse_bundled_skill_meta(skill_dir.parent().unwrap(), &skill_dir, base.path());
        assert!(meta.is_none(), "无 frontmatter 应返回 None");
    }

    // ---- collect_bundled_skills_recursive：递归发现 skill ----

    /// 同时存在「来源下的 skill」和「来源/分类/下的 skill」两种嵌套，都应被发现。
    #[test]
    fn test_collect_bundled_skills_recursive_finds_nested_skills() {
        let base = tempfile::tempdir().expect("创建 base tempdir 失败");
        // src1 下直接挂一个 skill，再在 category 子目录里挂一个——覆盖两层结构
        write_rel(base.path(), "src1/skillA/SKILL.md", "---\nname: a\n---\n");
        write_rel(base.path(), "src1/category/skillB/SKILL.md", "---\nname: b\n---\n");

        let mut skills = Vec::new();
        collect_bundled_skills_recursive(base.path(), base.path(), &mut skills);

        // 文件系统遍历顺序不保证，排序后比对，避免用例在 CI 上随机失败
        let mut names: Vec<String> = skills.into_iter().map(|m| m.name).collect();
        names.sort();
        assert_eq!(
            names,
            vec!["src1/category/skillB".to_string(), "src1/skillA".to_string()]
        );
    }

    /// 整棵树都没有 SKILL.md 时返回空——递归终点的正确性。
    #[test]
    fn test_collect_bundled_skills_recursive_empty_when_no_skill_md() {
        let base = tempfile::tempdir().expect("创建 base tempdir 失败");
        write_rel(base.path(), "src1/metadata.json", "{}");
        write_rel(base.path(), "src1/notes.md", "not a skill");

        let mut skills = Vec::new();
        collect_bundled_skills_recursive(base.path(), base.path(), &mut skills);
        assert!(skills.is_empty(), "没有任何 SKILL.md 时结果应为空");
    }

    // ---- resolve_file_under_skill：文件路径安全校验（字符串层 + canonicalize 前缀层）----

    /// 技能目录内的合法文件应返回 canonicalize 后的绝对路径。
    #[test]
    fn test_resolve_file_under_skill_normal() {
        let dir = tempfile::tempdir().expect("创建 tempdir 失败");
        let skill_md = write_rel(dir.path(), "SKILL.md", "x");
        let skill_dir = skill_md.parent().expect("SKILL.md 应有父目录");

        let resolved = resolve_file_under_skill(skill_dir, "SKILL.md")
            .expect("技能目录内的合法文件应通过校验");
        // 返回值就是 canonical 路径，再 canonicalize 应幂等
        assert_eq!(resolved.canonicalize().unwrap(), resolved);
        assert!(resolved.ends_with("SKILL.md"));
    }

    /// 含子目录的相对路径（如 references/guide.md）也属合法，应放行。
    #[test]
    fn test_resolve_file_under_skill_allows_nested() {
        let dir = tempfile::tempdir().expect("创建 tempdir 失败");
        let nested = write_rel(dir.path(), "refs/guide.md", "y");

        let resolved = resolve_file_under_skill(dir.path(), "refs/guide.md")
            .expect("子目录内的文件应通过校验");
        assert_eq!(resolved, nested.canonicalize().unwrap());
    }

    /// 空路径会让 join 退化为读技能目录本身，必须在字符串层拒绝。
    #[test]
    fn test_resolve_file_under_skill_rejects_empty() {
        let dir = tempfile::tempdir().expect("创建 tempdir 失败");
        assert!(resolve_file_under_skill(dir.path(), "").is_err(), "空路径应被拒绝");
    }

    /// `..` 父级引用直接被字符串层拦下，根本不会触达文件系统。
    #[test]
    fn test_resolve_file_under_skill_rejects_parent_traversal() {
        let dir = tempfile::tempdir().expect("创建 tempdir 失败");
        assert!(resolve_file_under_skill(dir.path(), "../escape.md").is_err(), "含 .. 的路径应被拒绝");
    }

    /// `sub/../../escape` 这类先下钻再绕回上级的写法同样含 `..`，应被拒。
    #[test]
    fn test_resolve_file_under_skill_rejects_subdir_traversal() {
        let dir = tempfile::tempdir().expect("创建 tempdir 失败");
        write_rel(dir.path(), "sub/keep.md", "z");
        assert!(
            resolve_file_under_skill(dir.path(), "sub/../../escape.md").is_err(),
            "绕回上级的路径应被拒绝"
        );
    }

    /// 绝对路径无视 skill_dir，必须拒绝，防止读到任意系统文件。
    #[test]
    fn test_resolve_file_under_skill_rejects_absolute() {
        let dir = tempfile::tempdir().expect("创建 tempdir 失败");
        assert!(
            resolve_file_under_skill(dir.path(), "/etc/passwd").is_err(),
            "绝对路径应被拒绝"
        );
    }

    /// 技能目录内的符号链接指向外部文件：字符串层放行（无 `..`），但 canonicalize 解析后
    /// 落到技能目录之外，必须被前缀层拦截——这是字符串层兜不住的唯一逃逸路径。
    #[test]
    fn test_resolve_file_under_skill_rejects_symlink_escape() {
        let skill_root = tempfile::tempdir().expect("创建 skill tempdir 失败");
        let outside = tempfile::tempdir().expect("创建 outside tempdir 失败");
        let target = write_rel(outside.path(), "secret.md", "top secret");

        // 仅 unix 支持无权限创建符号链接，Windows 上该用例整体跳过
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, skill_root.path().join("link.md"))
                .expect("创建符号链接失败");
            assert!(
                resolve_file_under_skill(skill_root.path(), "link.md").is_err(),
                "指向外部的符号链接应被前缀检查拦截"
            );
        }
    }

    /// name 含 `..` 时必须在 join 之前被字符串层校验拦下——这是 bundled file 接口的核心安全契约。
    /// 若 name 能逃出 skills 根目录，攻击者可配合干净的 rel_path 读取系统任意文件
    /// （详见 read_bundled_skill_file 注释）。name 校验委托给 resolve_skill_path_for_read，
    /// 其字符串层拒绝已在 skills 模块独立单测覆盖；这里验证「本接口确实接入了该校验」。
    #[test]
    fn test_read_bundled_skill_file_rejects_traversal_in_name() {
        // local_path 取任意值即可：name 的字符串层校验发生在任何文件系统访问之前，
        // 不依赖该目录是否真实存在，因此本用例在任意环境（含 CI）都稳定
        let err = read_bundled_skill_file("any/local", "../escape", "passwd")
            .expect_err("含 .. 的 name 必须被拒绝，不得触达文件读取");
        // 校验失败统一为 BadRequest，与 get_skill_file 的 name 校验语义一致
        assert!(
            matches!(err, AppError::BadRequest(_)),
            "期望 BadRequest，实际: {:?}",
            err
        );
    }
}
