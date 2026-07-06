//! Skills management handler.
//!
//! Discovers skills from executor directories, provides comparison, sync,
//! and execution tracking APIs.

use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use zip::write::FileOptions;
use zip::ZipArchive;

use crate::models::ExecutorType;
use crate::handlers::{AppError, AppState, ApiJson};
use crate::models::ApiResponse;

// ── Data types ──────────────────────────────────────────────────────────

/// Executor type name → skills directory mapping (string-based, shared with CLI).
///
/// 注意：`agents` 是**只读** skill 来源，没有 CLI，所以不出现在
/// `ExecutorType` 枚举里，但这里允许通过 `executor_skills_dir_str("agents")`
/// 拿到 `~/.agents/skills` 的路径用于扫描。
pub fn executor_skills_dir_str(et: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    match et {
        "claudecode" => Some(home.join(".claude").join("skills")),
        "hermes" => Some(home.join(".hermes").join("skills")),
        "codex" => Some(home.join(".codex").join("skills")),
        "codebuddy" => Some(home.join(".codebuddy").join("skills")),
        "opencode" => Some(home.join(".opencode").join("skills")),
        "atomcode" => Some(home.join(".atomcode").join("skills")),
        "kimi" => Some(home.join(".kimi").join("skills")),
        "mobilecoder" => Some(home.join(".mobile-coder").join("skills")),
        "pi" => Some(home.join(".pi").join("skills")),
        "mimo" => Some(home.join(".local/share/mimocode").join("skills")),
        // Zhanlu: Issue #673 新增执行器，session 路径为 ~/.local/share/zhanlu/storage，
        // skills 目录与 session 目录同根
        "zhanlu" => Some(home.join(".local/share/zhanlu").join("skills")),
        // agents 是只读 skill 来源：扫描但不参与执行器管理/Todo 执行
        "agents" => Some(home.join(".agents").join("skills")),
        _ => None,
    }
}

/// Executor type → skills directory mapping
///
/// 只是 ExecutorType 版本的薄包装；新代码应直接用 `executor_skills_dir_str`
/// 接收字符串参数（这样非 ExecutorType 来源如 `agents` 也能复用）。
fn executor_skills_dir(et: ExecutorType) -> Option<PathBuf> {
    // ExecutorType 必然有映射；这里直接 unwrap_or_default 也行，但
    // 保留 Option 让调用方决定空值时的行为
    executor_skills_dir_str(et.as_str())
}

/// 只读 skill 来源守卫：当前只有 `agents`（扫描 `~/.agents/skills`，无 CLI）。
///
/// 这些来源的 skill 可以看、可以导出、可以**作为同步源**复制到其他执行器，
/// 但**不能直接被删除或被导入覆盖**（避免误删外部工具维护的内容）。
///
/// 用 `matches!` 而不是等值比较：编译期保证名字写错时编译器提醒
/// （如果以后加新只读来源，往这里加一个 arm 即可）。
fn is_readonly_skill_source(name: &str) -> bool {
    matches!(name, "agents")
}

/// 进程内单调递增的临时目录 id 源：用于 import 临时目录等需要唯一名的场景。
///
/// 单靠 PID 不够（同一进程的并发请求 PID 相同），加 counter 才能保证并发不撞。
/// 64 位足够撑到天荒地老（每秒 1 亿次调用要 58 年才溢出）。
static NEXT_STAGING_ID: AtomicU64 = AtomicU64::new(0);

/// 取出下一个唯一的 staging 目录后缀
fn next_staging_id() -> u64 {
    NEXT_STAGING_ID.fetch_add(1, Ordering::Relaxed)
}

/// 把外部 `skill_name` 解析为「确实在 `base` 之下」的目录路径。
///
/// 防御：
/// - 绝对路径（如 `/etc`）直接拒
/// - 含 `..` 父级引用直接拒
/// - 含前缀（Windows `C:\\`）直接拒
/// - 解析后路径必须以 `base.canonicalize()` 为前缀
///
/// 与「直接 join + exists」的旧写法相比，这层校验避免：
/// - `/etc/passwd` 这种 escape 读取
/// - 符号链接绕过（canonicalize 后再 starts_with）
/// - 末尾 `/` 让 `split('/').next_back()` 得空串导致误删 skills 根
fn resolve_skill_path_under(base: &Path, skill_name: &str) -> Result<PathBuf, AppError> {
    // 第一道：纯字符串级校验，挡住最常见的恶意输入（不必走 IO 就能拒）
    let rel = Path::new(skill_name);
    if rel.as_os_str().is_empty() {
        return Err(AppError::BadRequest("Invalid skill name: empty".to_string()));
    }
    if rel.is_absolute() {
        return Err(AppError::BadRequest("Invalid skill name: absolute paths are not allowed".to_string()));
    }
    if rel.components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::Prefix(_))) {
        return Err(AppError::BadRequest("Invalid skill name: parent directory traversal is not allowed".to_string()));
    }

    // 第二道：IO 后兜底校验，挡住符号链接绕过等花招
    let base_canonical = base.canonicalize()
        .map_err(|e| AppError::Internal(format!("Failed to resolve base dir: {}", e)))?;
    let candidate = base.join(rel);
    let candidate_canonical = candidate.canonicalize()
        .map_err(|_| AppError::NotFound)?;  // 不存在就当 404

    if !candidate_canonical.starts_with(&base_canonical) {
        return Err(AppError::BadRequest("Invalid skill name: path escapes base directory".to_string()));
    }
    Ok(candidate_canonical)
}

/// 把外部 `skill_name` 解析为目录路径，用于**只读**操作（如获取内容、导出）。
///
/// 与 `resolve_skill_path_under` 的区别：
/// - 允许符号链接指向 skills 目录外的路径（如 `~/.claude/skills/xxx -> ~/.agents/skills/xxx`）
/// - 但仍拒绝绝对路径、`..` 父级引用等恶意输入
///
/// 这样用户可以通过符号链接访问其他位置的 skill，同时防止路径遍历攻击。
pub(crate) fn resolve_skill_path_for_read(base: &Path, skill_name: &str) -> Result<PathBuf, AppError> {
    // 第一道：纯字符串级校验（与 resolve_skill_path_under 相同）
    let rel = Path::new(skill_name);
    if rel.as_os_str().is_empty() {
        return Err(AppError::BadRequest("Invalid skill name: empty".to_string()));
    }
    if rel.is_absolute() {
        return Err(AppError::BadRequest("Invalid skill name: absolute paths are not allowed".to_string()));
    }
    if rel.components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::Prefix(_))) {
        return Err(AppError::BadRequest("Invalid skill name: parent directory traversal is not allowed".to_string()));
    }

    // 第二道：检查路径是否存在（不检查是否在 base 下，允许符号链接逃逸）
    let candidate = base.join(rel);
    if !candidate.exists() {
        return Err(AppError::NotFound);
    }

    Ok(candidate)
}

fn executor_label(et: ExecutorType) -> &'static str {
    match et {
        ExecutorType::Claudecode => "Claude Code",
        ExecutorType::Hermes => "Hermes",
        ExecutorType::Codex => "Codex",
        ExecutorType::Codebuddy => "CodeBuddy",
        ExecutorType::Opencode => "Opencode",
        ExecutorType::Atomcode => "AtomCode",
        ExecutorType::Kimi => "Kimi",
        ExecutorType::Mobilecoder => "MobileCoder",
        ExecutorType::Codewhale => "CodeWhale",
        ExecutorType::Pi => "Pi",
        ExecutorType::Mimo => "MiMo",
        ExecutorType::Zhanlu => "Zhanlu",
        ExecutorType::Kilo => "Kilo",
    }
}

// 保留 ALL_EXECUTORS 供其他可能用到的代码；新代码请用 ALL_SKILL_SOURCES
// 13 = 12 个旧执行器 + 新增的 Kilo
// 注意：加新执行器必须同时更新下面数组与本注释的计数，否则会出现 H1 同型错位。
#[allow(dead_code)]
const ALL_EXECUTORS: [ExecutorType; 13] = [
    ExecutorType::Claudecode,
    ExecutorType::Hermes,
    ExecutorType::Codex,
    ExecutorType::Codebuddy,
    ExecutorType::Opencode,
    ExecutorType::Atomcode,
    ExecutorType::Kimi,
    ExecutorType::Mobilecoder,
    ExecutorType::Codewhale,
    ExecutorType::Pi,
    ExecutorType::Mimo,
    ExecutorType::Zhanlu,
    ExecutorType::Kilo,
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub license: Option<String>,
    pub keywords: Vec<String>,
    pub file_count: u32,
    pub total_size: u64,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorSkills {
    pub executor: String,
    pub executor_label: String,
    pub skills_dir: String,
    pub skills_dir_exists: bool,
    pub skills: Vec<SkillMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillComparison {
    pub skill_name: String,
    pub description: String,
    pub executors: HashMap<String, SkillPresence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPresence {
    pub present: bool,
    pub version: Option<String>,
    pub modified_at: Option<String>,
}

/// 单个执行器的版本信息（用于版本更新检测）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionInfo {
    pub executor: String,
    pub executor_label: String,
    pub version: Option<String>,
    pub modified_at: Option<String>,
    pub is_latest: bool,
}

/// 版本更新检测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersionUpdate {
    pub skill_name: String,
    pub description: String,
    pub versions: Vec<SkillVersionInfo>,
    pub latest_version: Option<String>,
    pub latest_executor: String,
    pub has_update: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInvocation {
    pub id: i64,
    pub skill_name: String,
    pub executor: String,
    pub todo_id: i64,
    pub todo_title: Option<String>,
    pub invoked_at: String,
    pub status: String,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SyncRequest {
    pub source_executor: String,
    pub skill_name: String,
    pub target_executors: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct InvocationQuery {
    pub page: Option<i64>,
    pub limit: Option<i64>,
    pub skill_name: Option<String>,
    pub executor: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PaginatedInvocations {
    pub items: Vec<SkillInvocation>,
    pub total: i64,
    pub page: i64,
    pub limit: i64,
}

#[derive(Debug, Deserialize)]
pub struct RecordInvocationRequest {
    pub skill_name: String,
    pub executor: String,
    pub todo_id: i64,
    pub status: String,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteSkillQuery {
    pub executor: String,
    pub skill_name: String,
}

#[derive(Debug, Deserialize)]
pub struct SkillContentQuery {
    pub executor: String,
    pub skill_name: String,
}

#[derive(Debug, Deserialize)]
pub struct SkillExportQuery {
    pub executor: String,
    pub skill_name: String,
}

#[derive(Debug, Deserialize)]
pub struct SkillFileQuery {
    pub executor: String,
    pub skill_name: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    pub executor: String,
    pub skill_name: Option<String>,
    pub flatten: Option<bool>,
}

// ── Skill discovery ─────────────────────────────────────────────────────

fn parse_skill_yaml_header(content: &str) -> SkillMeta {
    let mut name = String::new();
    let mut description = String::new();
    let mut version = None;
    let mut author = None;
    let mut license = None;
    let mut keywords = Vec::new();
    let mut in_keywords_section = false;

    // Parse YAML front matter between --- markers
    if let Some(yaml_content) = extract_yaml_front_matter(content) {
        for line in yaml_content.lines() {
            if let Some(val) = line.strip_prefix("name:") {
                name = val.trim().trim_matches('"').to_string();
            } else if let Some(val) = line.strip_prefix("description:") {
                // description can be multi-line or quoted
                let val = val.trim();
                if val.starts_with('|') || val.starts_with('>') {
                    // skip multi-line for now, use first line
                } else {
                    description = val.trim_matches('"').to_string();
                }
            } else if let Some(val) = line.strip_prefix("version:") {
                version = Some(val.trim().trim_matches('"').to_string());
            } else if let Some(val) = line.strip_prefix("author:") {
                author = Some(val.trim().trim_matches('"').to_string());
            } else if let Some(val) = line.strip_prefix("license:") {
                license = Some(val.trim().trim_matches('"').to_string());
            } else if line.contains("keywords:") {
                in_keywords_section = true;
            } else if line.trim().is_empty() {
                in_keywords_section = false;
            } else if let Some(val) = line.strip_prefix("  - ") {
                if in_keywords_section {
                    keywords.push(val.trim_matches('"').to_string());
                }
            }
        }
    }

    // Fallback: if name is empty, try first heading
    if name.is_empty() {
        for line in content.lines() {
            if let Some(heading) = line.strip_prefix("# ") {
                name = heading.trim().to_string();
                break;
            }
        }
    }

    // Fallback: if description is empty, use first non-empty, non-front-matter line
    if description.is_empty() {
        let mut past_front = false;
        let mut dash_count = 0;
        for line in content.lines() {
            if line.trim() == "---" {
                dash_count += 1;
                if dash_count >= 2 {
                    past_front = true;
                }
                continue;
            }
            if past_front && !line.trim().is_empty() && !line.starts_with('#') {
                description = line.trim().chars().take(200).collect();
                break;
            }
        }
    }

    SkillMeta {
        name,
        description,
        version,
        author,
        license,
        keywords,
        file_count: 0,
        total_size: 0,
        modified_at: None,
    }
}

fn extract_yaml_front_matter(content: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.first()?.trim() != "---" {
        return None;
    }
    let mut end = 1;
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            end = i;
            break;
        }
    }
    Some(lines[1..end].join("\n"))
}

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

/// Recursively find skill directories containing SKILL.md.
/// Supports both flat (skill/SKILL.md) and nested (category/skill/SKILL.md) layouts.
fn collect_skills_recursive(base_dir: &std::path::Path, current_dir: &std::path::Path, skills: &mut Vec<SkillMeta>) {
    if let Ok(entries) = std::fs::read_dir(current_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skill_md = path.join("SKILL.md");
            if skill_md.exists() {
                let content = std::fs::read_to_string(&skill_md).unwrap_or_default();
                let mut meta = parse_skill_yaml_header(&content);

                if meta.name.is_empty() {
                    meta.name = path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                }

                // Use relative path from base as a category prefix for nested dirs
                if let Ok(rel) = path.strip_prefix(base_dir) {
                    let rel_str = rel.to_string_lossy().to_string();
                    // Only add prefix if nested (e.g. "devops/lark-cli" -> keep as name)
                    if rel_str.contains('/')
                        && meta.name == path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default() {
                            meta.name = rel_str;
                        }
                }

                let (file_count, total_size) = count_files_and_size(&path);
                meta.file_count = file_count;
                meta.total_size = total_size;

                if let Ok(metadata) = std::fs::metadata(&skill_md) {
                    meta.modified_at = metadata.modified().ok().map(|t| {
                        let secs = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                        chrono::DateTime::from_timestamp(secs as i64, 0)
                            .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
                            .unwrap_or_default()
                    });
                }

                skills.push(meta);
            } else {
                // No SKILL.md here — recurse deeper (may be a category folder)
                collect_skills_recursive(base_dir, &path, skills);
            }
        }
    }
}

/// 通用扫描：接受任意 executor 名字字符串（含 `agents` 这种只读来源）。
///
/// 把核心路径/扫描逻辑抽出来，原 `discover_skills_for_executor` 变为薄包装，
/// 这样只读 skill 来源（如 `agents`）也能复用同一份发现逻辑。
///
/// 行为：
/// - 输入：executor 名字（如 `"claudecode"` / `"agents"`）+ UI 显示标签
/// - 输出：该来源的 ExecutorSkills（路径、是否存在、扫描到的 skills）
///
/// 边界：name 不在 `executor_skills_dir_str` 映射里时，返回「目录不存在」占位
/// （不报错，因为前端可能传入未安装的执行器名）。
fn discover_skills_for(name: &str, label: &str) -> ExecutorSkills {
    // 拿 skills 目录；映射不到就当成「这个来源没配置」返回空结果
    let skills_dir = match executor_skills_dir_str(name) {
        Some(p) => p,
        None => {
            // 边界：未知的 executor 名字在生产里可能是脏数据，
            // 这里降级返回而不是 5xx，让前端 UI 友好展示
            return ExecutorSkills {
                executor: name.to_string(),
                executor_label: label.to_string(),
                skills_dir: String::new(),
                skills_dir_exists: false,
                skills: vec![],
            };
        }
    };

    // 提前 to_string 一次避免后续多次系统调用
    let dir_str = skills_dir.to_string_lossy().to_string();
    // exists 检查是必要的：collect_skills_recursive 不会自己返回 0，
    // 它对不存在的目录静默返回空 vec，前端就看不出"目录被删了" vs "目录没 skill"
    let exists = skills_dir.exists();

    // 只在目录存在时才递归扫描，避免对不存在的目录做无意义的 read_dir
    let mut skills = Vec::new();
    if exists {
        collect_skills_recursive(&skills_dir, &skills_dir, &mut skills);
    }

    // 大小写不敏感排序：UI Tab 内显示顺序稳定，
    // 否则 "Foo" 和 "bar" 会按 ASCII 顺序穿插，跨执行器对比时不一致
    skills.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    ExecutorSkills {
        executor: name.to_string(),
        executor_label: label.to_string(),
        skills_dir: dir_str,
        skills_dir_exists: exists,
        skills,
    }
}

// ── API handlers ────────────────────────────────────────────────────────

/// 参与 skill 扫描/对比的所有来源：9 个执行器 + 只读来源 `agents`。
///
/// 用字符串数组而非 `ExecutorType` 数组，方便容纳非 ExecutorType 来源。
/// **新增来源时**：
/// 1. 在 `executor_skills_dir_str` 加分支
/// 2. 在本数组加字符串
/// 3. 如果不是 ExecutorType，在 `executor_label_for_source` 加显示名
const ALL_SKILL_SOURCES: &[&str] = &[
    "claudecode", "codebuddy", "opencode", "atomcode",
    "hermes", "kimi", "mobilecoder", "codex",
    "pi", "mimo", "zhanlu",
    "agents",
];

/// 把 source 名字转成 UI 显示名。
///
/// 设计选择：先 `match` agents 这种特殊来源（避免 parse_executor_type 的成本），
/// 剩下的 fallthrough 到 `parse_executor_type` 走 ExecutorType 路径，
/// 找不到时返回空串（让 UI 退化显示原始 name）。
fn executor_label_for_source(name: &str) -> &'static str {
    match name {
        // 特殊来源走专门分支，避开 parse_executor_type 的解析开销
        "agents" => "Agents",
        other => {
            // 解析失败的回退：返回空串，调用方会兜底用 name 当 label
            if let Some(et) = crate::adapters::parse_executor_type(other) {
                executor_label(et)
            } else {
                ""
            }
        }
    }
}

/// GET /api/skills - List skills grouped by executor
///
/// GET /api/skills - List skills grouped by executor
///
/// 扫描 11 个 ExecutorType 之外，还扫 `~/.agents/skills`（只读 skill 来源）。
/// agents 不参与 Todo 执行，但能在 Skills 总览/对比/同步里看到并使用。
///
/// 实现选择：每个来源的目录 IO 放在 `spawn_blocking` 里跑，
/// 因为 read_dir 在大目录（hermes 146 个 skill）下可能阻塞 tokio worker。
pub async fn list_skills(
    State(_state): State<AppState>,
) -> Result<ApiResponse<Vec<ExecutorSkills>>, AppError> {
    // spawn_blocking：磁盘 IO 不能跑在 tokio reactor 上，否则会卡住其他请求
    let result = tokio::task::spawn_blocking(move || {
        // 顺序遍历 12 个来源：单次调用只 IO 一次，顺序 vs 并行收益不大，
        // 而且顺序能保证响应里 source 顺序稳定，方便前端按位置渲染 Tab
        ALL_SKILL_SOURCES
            .iter()
            .map(|name| discover_skills_for(name, executor_label_for_source(name)))
            .collect::<Vec<ExecutorSkills>>()
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking join error: {}", e)))?;
    Ok(ApiResponse::ok(result))
}

/// GET /api/skills/content - Get skill content (SKILL.md and metadata)
pub async fn get_skill_content(
    Query(query): Query<SkillContentQuery>,
) -> Result<ApiResponse<SkillContentResponse>, AppError> {
    // 既接受 ExecutorType，也接受只读来源（`agents`）
    let skills_dir = executor_skills_dir_str(&query.executor)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown executor: {}", query.executor)))?;

    // 用 resolve_skill_path_for_read 校验 skill_name
    // 对于只读操作，允许符号链接指向 skills 目录外的路径
    let skill_dir = resolve_skill_path_for_read(&skills_dir, &query.skill_name)?;

    let skill_name = query.skill_name.clone();
    let executor = query.executor.clone();
    let result = tokio::task::spawn_blocking(move || {
        let skill_md_path = skill_dir.join("SKILL.md");
        let content = if skill_md_path.exists() {
            std::fs::read_to_string(&skill_md_path).unwrap_or_default()
        } else {
            String::new()
        };

        let mut files = Vec::new();
        collect_skill_files(&skill_dir, &skill_dir, &mut files);

        SkillContentResponse {
            skill_name,
            executor,
            content,
            files,
        }
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking join error: {}", e)))?;

    Ok(ApiResponse::ok(result))
}

/// GET /api/skills/file - Get a single file's content within a skill
pub async fn get_skill_file(
    Query(query): Query<SkillFileQuery>,
) -> Result<ApiResponse<SkillFileContentResponse>, AppError> {
    let skills_dir = executor_skills_dir_str(&query.executor)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown executor: {}", query.executor)))?;

    let skill_dir = resolve_skill_path_for_read(&skills_dir, &query.skill_name)?;

    // 安全校验：防止路径遍历攻击
    let file_path = skill_dir.join(&query.path);
    let file_path_canonical = file_path.canonicalize()
        .map_err(|e| AppError::Internal(format!("Failed to resolve file path: {}", e)))?;
    let skill_dir_canonical = skill_dir.canonicalize()
        .map_err(|e| AppError::Internal(format!("Failed to resolve skill dir: {}", e)))?;
    if !file_path_canonical.starts_with(&skill_dir_canonical) {
        return Err(AppError::BadRequest("Invalid file path: escapes skill directory".to_string()));
    }

    if !file_path.exists() || !file_path.is_file() {
        return Err(AppError::NotFound);
    }

    let result = tokio::task::spawn_blocking(move || -> Result<SkillFileContentResponse, AppError> {
        let content = std::fs::read_to_string(&file_path)
            .map_err(|e| AppError::Internal(format!("Failed to read file: {}", e)))?;
        // query.path 是 String 类型，进入 spawn_blocking 闭包时 move 即可，无需 clone
        Ok(SkillFileContentResponse {
            path: query.path,
            content,
        })
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking join error: {}", e)))??;

    Ok(ApiResponse::ok(result))
}

/// DELETE /api/skills - Delete a skill from an executor
pub async fn delete_skill(
    Query(query): Query<DeleteSkillQuery>,
) -> Result<ApiResponse<String>, AppError> {
    // 只读 skill 来源（如 `agents`）禁止删除
    if is_readonly_skill_source(&query.executor) {
        return Err(AppError::BadRequest(format!(
            "Executor '{}' is a read-only skill source; cannot delete skills here",
            query.executor
        )));
    }
    let et = crate::adapters::parse_executor_type(&query.executor)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown executor: {}", query.executor)))?;

    let skills_dir = executor_skills_dir(et)
        .ok_or_else(|| AppError::BadRequest("No skills directory for this executor".to_string()))?;

    // Reject skill names with path separators or parent traversal
    if query.skill_name.contains('/') || query.skill_name.contains('\\') || query.skill_name.contains("..") {
        return Err(AppError::BadRequest("Invalid skill name: path separators and '..' are not allowed".to_string()));
    }

    let skill_dir = skills_dir.join(&query.skill_name);
    if !skill_dir.exists() || !skill_dir.is_dir() {
        return Err(AppError::NotFound);
    }

    // Verify the path is under the skills directory and is a direct child
    let skill_dir_canonical = skill_dir.canonicalize()
        .map_err(|e| AppError::Internal(format!("Failed to resolve skill dir: {}", e)))?;
    let skills_dir_canonical = skills_dir.canonicalize()
        .map_err(|e| AppError::Internal(format!("Failed to resolve skills dir: {}", e)))?;
    if skill_dir_canonical == skills_dir_canonical {
        return Err(AppError::BadRequest("Cannot delete the skills root directory".to_string()));
    }
    if !skill_dir_canonical.starts_with(&skills_dir_canonical) {
        return Err(AppError::BadRequest("Invalid skill name: path escapes skills directory".to_string()));
    }
    if skill_dir_canonical.parent() != Some(skills_dir_canonical.as_path()) {
        return Err(AppError::BadRequest("Invalid skill name: must be a direct child of skills directory".to_string()));
    }

    let skill_name = query.skill_name.clone();
    tokio::task::spawn_blocking(move || {
        std::fs::remove_dir_all(&skill_dir)
            .map_err(|e| AppError::Internal(format!("Failed to delete skill: {}", e)))
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking join error: {}", e)))??;

    Ok(ApiResponse::ok(format!("Skill '{}' deleted", skill_name)))
}

/// GET /api/skills/export - Export skill as .zip
pub async fn export_skill(
    Query(query): Query<SkillExportQuery>,
) -> Result<Vec<u8>, AppError> {
    // 支持只读来源（`agents`）的导出
    let skills_dir = executor_skills_dir_str(&query.executor)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown executor: {}", query.executor)))?;

    // 用 resolve_skill_path_for_read 校验 skill_name
    // 对于只读操作，允许符号链接指向 skills 目录外的路径
    let skill_dir = resolve_skill_path_for_read(&skills_dir, &query.skill_name)?;

    // Create zip in memory
    let mut zip_data = Vec::new();
    {
        let mut zip_writer = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_data));
        let options = FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);

        add_dir_to_zip(&mut zip_writer, &skill_dir, &query.skill_name, &options)
            .map_err(|e| AppError::Internal(format!("Failed to create archive: {}", e)))?;

        zip_writer.finish()
            .map_err(|e| AppError::Internal(format!("Failed to finish archive: {}", e)))?;
    }

    Ok(zip_data)
}

fn add_dir_to_zip<W: std::io::Write + std::io::Seek>(
    zip_writer: &mut zip::ZipWriter<W>,
    dir: &std::path::Path,
    prefix: &str,
    options: &FileOptions<()>,
) -> std::io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        // read_dir 保证条目有效，file_name() 不可能为 None（根路径除外，但这里是子目录遍历）
        #[allow(clippy::unwrap_used)]
        let name = format!("{}/{}", prefix, path.file_name().unwrap().to_string_lossy());

        if path.is_dir() {
            add_dir_to_zip(zip_writer, &path, &name, options)?;
        } else {
            zip_writer.start_file(name, *options)?;
            let mut file = std::fs::File::open(&path)?;
            std::io::copy(&mut file, zip_writer)?;
        }
    }

    Ok(())
}

/// POST /api/skills/import - Import skill from .zip
pub async fn import_skill(
    State(_state): State<AppState>,
    params: Query<ImportRequest>,
    body: axum::body::Bytes,
) -> Result<ApiResponse<ImportResult>, AppError> {
    // 只读 skill 来源（如 `agents`）禁止导入覆盖
    if is_readonly_skill_source(&params.executor) {
        return Err(AppError::BadRequest(format!(
            "Executor '{}' is a read-only skill source; cannot import here",
            params.executor
        )));
    }
    let et = crate::adapters::parse_executor_type(&params.executor)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown executor: {}", params.executor)))?;

    let skills_dir = executor_skills_dir(et)
        .ok_or_else(|| AppError::BadRequest("No skills directory for this executor".to_string()))?;

    std::fs::create_dir_all(&skills_dir)
        .map_err(|e| AppError::Internal(format!("Failed to create skills dir: {}", e)))?;

    // Decode zip
    let cursor = std::io::Cursor::new(body.to_vec());
    let mut archive = ZipArchive::new(cursor)
        .map_err(|e| AppError::BadRequest(format!("Invalid zip archive: {}", e)))?;

    let flatten = params.flatten.unwrap_or(true);
    let skill_name = params.skill_name.clone().unwrap_or_else(|| "imported-skill".to_string());

    // Validate skill_name: reject absolute paths and parent directory traversal
    if skill_name.starts_with('/') || skill_name.contains("..") {
        return Err(AppError::BadRequest("Invalid skill name: absolute paths and parent directory traversal are not allowed".to_string()));
    }

    let target_dir = skills_dir.join(&skill_name);

    // 安全设计：先解到 **临时目录**，全部 entry 校验通过后再原子替换 target_dir。
    //
    // 必要性：直接解到 target_dir 时，如果第 5 个 entry 才触发大小限制或
    // 路径校验，前面 4 个文件已经写盘但 API 返回 400，**用户看到的现象是
    // 旧 skill 被部分覆盖 + 导入失败**。用临时目录 + 原子 rename 能保证：
    // 1) 校验全过才动原 skill
    // 2) 任何中途失败都只留下临时垃圾，target_dir 完整无缺
    //
    // 临时目录名加 PID + 单调计数器：单 PID 区分**进程**级并发，
    // counter 区分**同进程内**并发（不同 async handler 并行 import 同一 skill 时）
    let staging_id = next_staging_id();
    let staging_dir = skills_dir.join(format!(".{}.import.tmp.{}.{}", skill_name, std::process::id(), staging_id));
    // 清理可能的残留临时目录（上次失败留下的）
    if staging_dir.exists() {
        let _ = std::fs::remove_dir_all(&staging_dir);
    }
    std::fs::create_dir_all(&staging_dir)
        .map_err(|e| AppError::Internal(format!("Failed to create staging dir: {}", e)))?;

    // 提取作用域：staging_dir 是唯一允许写入的地方
    let extract_result: Result<i32, AppError> = (|| {
        // 校验 staging_dir 解析后仍在 skills_dir 之下（防御符号链接绕过）
        let staging_canonical = staging_dir.canonicalize()
            .map_err(|e| AppError::Internal(format!("Failed to resolve staging dir: {}", e)))?;
        let skills_dir_canonical = skills_dir.canonicalize()
            .map_err(|e| AppError::Internal(format!("Failed to resolve skills dir: {}", e)))?;
        if !staging_canonical.starts_with(&skills_dir_canonical) {
            return Err(AppError::BadRequest("Invalid staging path: escapes skills directory".to_string()));
        }

        // Zip bomb protection: limits for extracted files
        const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;    // 50 MB per file
        const MAX_TOTAL_SIZE: u64 = 200 * 1024 * 1024;   // 200 MB total
        let mut total_extracted: u64 = 0;
        let mut imported_files = 0i32;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)
                .map_err(|e| AppError::Internal(format!("Failed to read zip entry: {}", e)))?;

            let path = file.mangled_name();
            let outpath = path.clone();

            // Reject absolute paths and paths with parent directory traversal
            if outpath.is_absolute() || outpath.components().any(|c| c.as_os_str() == "..") {
                return Err(AppError::BadRequest(format!("Invalid path in archive: {}", outpath.display())));
            }

            let file_name = outpath.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // Skip directories and hidden files
            if file_name.starts_with('.') || file_name.is_empty() {
                continue;
            }

            // Check declared size early to reject obviously large files
            let declared_size = file.size();
            if declared_size > MAX_FILE_SIZE {
                return Err(AppError::BadRequest(format!(
                    "File too large in archive: {} ({} bytes)", file_name, declared_size
                )));
            }

            // 注意：所有 dest_path 都在 staging_dir 下，不再是 target_dir
            let dest_path = if flatten {
                staging_dir.join(&file_name)
            } else {
                staging_dir.join(&outpath)
            };

            // Verify dest_path is still under staging_dir（防御性检查）
            if let Ok(dest_path_canonical) = dest_path.canonicalize() {
                if !dest_path_canonical.starts_with(&staging_canonical) {
                    return Err(AppError::BadRequest(format!("Path escapes staging directory: {}", outpath.display())));
                }
            }

            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| AppError::Internal(format!("Failed to create dir: {}", e)))?;
            }

            let mut outfile = std::fs::File::create(&dest_path)
                .map_err(|e| AppError::Internal(format!("Failed to create file: {}", e)))?;

            // Use take() to enforce per-file size limit, protecting against zip bombs
            let mut reader = file.by_ref().take(MAX_FILE_SIZE + 1);
            let written = std::io::copy(&mut reader, &mut outfile)?;
            if written > MAX_FILE_SIZE {
                std::fs::remove_file(&dest_path).ok();
                return Err(AppError::BadRequest(format!(
                    "File exceeds size limit during extraction: {} ({} bytes)", file_name, written
                )));
            }
            total_extracted += written;
            if total_extracted > MAX_TOTAL_SIZE {
                return Err(AppError::BadRequest(format!(
                    "Total extracted size exceeds limit ({} bytes)", MAX_TOTAL_SIZE
                )));
            }
            imported_files += 1;
        }
        Ok(imported_files)
    })();

    // 提取失败：清理临时目录，target_dir 保持原样不动
    let imported_files = match extract_result {
        Ok(n) => n,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging_dir);
            return Err(e);
        }
    };

    // 提取成功：原子替换 target_dir
    // 1. 如果 target_dir 已存在（更新场景），先备份到 .old 暂存，原子替换后删 .old
    // 2. 失败时用 .old 恢复
    let backup_dir = skills_dir.join(format!(".{}.old.tmp.{}", skill_name, std::process::id()));
    let _ = std::fs::remove_dir_all(&backup_dir); // 清残留
    let had_existing = target_dir.exists();

    if had_existing {
        // rename 在某些 fs 上不能覆盖已存在目录；用 .old 暂存中转
        std::fs::rename(&target_dir, &backup_dir)
            .map_err(|e| AppError::Internal(format!("Failed to backup existing skill: {}", e)))?;
    }

    let swap_result = std::fs::rename(&staging_dir, &target_dir);

    if let Err(e) = swap_result {
        // 替换失败：恢复 backup 到 target_dir，保留旧数据
        if had_existing {
            let _ = std::fs::rename(&backup_dir, &target_dir);
        }
        let _ = std::fs::remove_dir_all(&staging_dir);
        return Err(AppError::Internal(format!("Failed to commit import: {}", e)));
    }

    // 替换成功：清理 backup
    if had_existing {
        let _ = std::fs::remove_dir_all(&backup_dir);
    }

    Ok(ApiResponse::ok(ImportResult {
        skill_name,
        imported_files,
        message: format!("Successfully imported {} files", imported_files),
    }))
}

#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub skill_name: String,
    pub imported_files: i32,
    pub message: String,
}

fn collect_skill_files(base: &std::path::Path, current: &std::path::Path, files: &mut Vec<SkillFileInfo>) {
    if let Ok(entries) = std::fs::read_dir(current) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    let rel_path = path.strip_prefix(base)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    files.push(SkillFileInfo {
                        path: rel_path,
                        size: metadata.len(),
                        modified_at: metadata.modified().ok().map(|t| {
                            let secs = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                            chrono::DateTime::from_timestamp(secs as i64, 0)
                                .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
                                .unwrap_or_default()
                        }).unwrap_or_default(),
                    });
                } else if metadata.is_dir() {
                    collect_skill_files(base, &path, files);
                }
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SkillContentResponse {
    pub skill_name: String,
    pub executor: String,
    pub content: String,
    pub files: Vec<SkillFileInfo>,
}

#[derive(Debug, Serialize)]
pub struct SkillFileInfo {
    pub path: String,
    pub size: u64,
    pub modified_at: String,
}

#[derive(Debug, Serialize)]
pub struct SkillFileContentResponse {
    pub path: String,
    pub content: String,
}

/// GET /api/skills/compare - Cross-executor skill comparison matrix
///
/// 比 8 个 ExecutorType 多扫了 `agents`（`~/.agents/skills`），让用户
/// 能看到 "lark-doc" 这类 skill 在哪些来源里有、版本是不是落后。
///
/// 输出结构：每个 skill 一行，每个来源一列，单元格标记 present/version。
/// 这样前端可以画 N 行的对比表格，**任意两个来源**之间都能对比。
///
/// 实现选择：所有磁盘 IO（`discover_skills_for` 内部的 read_dir 递归）
/// 放到 `spawn_blocking` 里跑，避免大目录（如 hermes 146 个 skill）
/// 阻塞 tokio reactor worker。
pub async fn compare_skills(
    State(_state): State<AppState>,
) -> Result<ApiResponse<Vec<SkillComparison>>, AppError> {
    // spawn_blocking：read_dir 不能跑在 tokio worker 上
    let comparisons = tokio::task::spawn_blocking(move || {
        // 第一遍：把所有来源的 skills 扫成双层 map（source → name → meta）
        // 嵌套 map 让后面 lookup 是 O(1)，避免对每个 skill 名都做线性扫描
        let mut all_skills: HashMap<String, HashMap<String, SkillMeta>> = HashMap::new();
        for name in ALL_SKILL_SOURCES {
            let es = discover_skills_for(name, executor_label_for_source(name));
            // 单独内层 map：覆盖同源同名 skill（实际不会发生，但防御性编码）
            let mut map = HashMap::new();
            for skill in es.skills {
                map.insert(skill.name.clone(), skill);
            }
            all_skills.insert((*name).to_string(), map);
        }

        // 取所有来源的 skill 名字的并集，作为对比的"行"集合
        // 走 HashSet 是为了去重（同名 skill 在多个来源里只算一行）
        let mut skill_names: Vec<String> = all_skills.values()
            .flat_map(|m| m.keys().cloned())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        // 排序让响应顺序稳定，前端表格渲染不会因调用时机不同而抖动
        skill_names.sort();

        // 第二遍：每个 skill 名生成一行对比，标记每个来源有没有
        let comparisons: Vec<SkillComparison> = skill_names.into_iter().map(|name| {
            // 内层循环：每个来源都查一遍这个 skill 在不在
            // 用 if-let-some 而不是 .map().unwrap_or() 写更直白
            let mut executors_map = HashMap::new();
            for src in ALL_SKILL_SOURCES {
                let key = (*src).to_string();
                if let Some(skill) = all_skills.get(&key).and_then(|m| m.get(&name)) {
                    // 命中：填 present + 版本信息
                    executors_map.insert(key, SkillPresence {
                        present: true,
                        version: skill.version.clone(),
                        modified_at: skill.modified_at.clone(),
                    });
                } else {
                    // 未命中：填 present=false，前端用灰色格子展示
                    executors_map.insert(key, SkillPresence {
                        present: false,
                        version: None,
                        modified_at: None,
                    });
                }
            }

            // description 按 ALL_SKILL_SOURCES 固定顺序查，第一个非空的胜出
            // （用 HashMap 迭代顺序不确定，跨调用 description 可能漂移）
            let description = ALL_SKILL_SOURCES
                .iter()
                .filter_map(|src| all_skills.get(*src).and_then(|m| m.get(&name)))
                .find_map(|s| {
                    // 跳过空 description：可能某个来源的 SKILL.md 没写 description
                    if s.description.is_empty() { None } else { Some(s.description.clone()) }
                })
                .unwrap_or_default();

            SkillComparison {
                skill_name: name,
                description,
                executors: executors_map,
            }
        }).collect();

        comparisons
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking join error: {}", e)))?;

    Ok(ApiResponse::ok(comparisons))
}

/// 比较两个版本字符串，返回 Ordering
/// 优先 semver 比较，无法解析时 fallback 到字符串比较
fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    // 尝试 semver 解析
    if let (Ok(va), Ok(vb)) = (semver::Version::parse(a), semver::Version::parse(b)) {
        return va.cmp(&vb);
    }
    // fallback: 字符串比较
    a.cmp(b)
}

/// 从多个版本中找出最新版本的执行器
fn find_latest_executor(versions: &[SkillVersionInfo]) -> Option<&SkillVersionInfo> {
    versions.iter()
        .filter(|v| v.version.is_some())
        .max_by(|a, b| {
            compare_versions(
                a.version.as_deref().unwrap_or(""),
                b.version.as_deref().unwrap_or(""),
            )
        })
}

/// GET /api/skills/version-update - 检测 skill 版本更新
///
/// 返回所有在不同执行器间版本不同的 skill，标记最新版本和需要更新的执行器。
/// 版本比较策略：优先 semver，无法解析时 fallback 到字符串比较。
pub async fn version_update_list(
    State(_state): State<AppState>,
) -> Result<ApiResponse<Vec<SkillVersionUpdate>>, AppError> {
    let updates = tokio::task::spawn_blocking(move || {
        // 第一遍：把所有来源的 skills 扫成双层 map（source → name → meta）
        let mut all_skills: HashMap<String, HashMap<String, SkillMeta>> = HashMap::new();
        for name in ALL_SKILL_SOURCES {
            let es = discover_skills_for(name, executor_label_for_source(name));
            let mut map = HashMap::new();
            for skill in es.skills {
                map.insert(skill.name.clone(), skill);
            }
            all_skills.insert((*name).to_string(), map);
        }

        // 取所有来源的 skill 名字的并集
        let mut skill_names: Vec<String> = all_skills.values()
            .flat_map(|m| m.keys().cloned())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        skill_names.sort();

        // 第二遍：每个 skill 名生成版本更新检测结果
        let updates: Vec<SkillVersionUpdate> = skill_names.into_iter().filter_map(|name| {
            // 收集所有执行器的版本信息
            let mut versions: Vec<SkillVersionInfo> = Vec::new();
            for src in ALL_SKILL_SOURCES {
                let key = (*src).to_string();
                if let Some(skill) = all_skills.get(&key).and_then(|m| m.get(&name)) {
                    versions.push(SkillVersionInfo {
                        executor: key.clone(),
                        executor_label: executor_label_for_source(src).to_string(),
                        version: skill.version.clone(),
                        modified_at: skill.modified_at.clone(),
                        is_latest: false,
                    });
                }
            }

            // 只有当 skill 在多个执行器中存在且版本不同时才返回
            if versions.len() < 2 {
                return None;
            }

            // 找出最新版本的执行器
            let latest = find_latest_executor(&versions)?;
            let latest_version = latest.version.clone();
            let latest_executor = latest.executor.clone();

            // 标记最新版本
            for v in versions.iter_mut() {
                v.is_latest = v.version == latest_version;
            }

            // 检查是否有执行器需要更新（版本不同或没有版本号）
            let has_update = versions.iter().any(|v| {
                v.executor != latest_executor && v.version != latest_version
            });

            // 只有存在版本差异时才返回
            if !has_update {
                return None;
            }

            // description 按 ALL_SKILL_SOURCES 固定顺序查，第一个非空的胜出
            let description = ALL_SKILL_SOURCES
                .iter()
                .filter_map(|src| all_skills.get(*src).and_then(|m| m.get(&name)))
                .find_map(|s| {
                    if s.description.is_empty() { None } else { Some(s.description.clone()) }
                })
                .unwrap_or_default();

            Some(SkillVersionUpdate {
                skill_name: name,
                description,
                versions,
                latest_version,
                latest_executor,
                has_update,
            })
        }).collect();

        updates
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking join error: {}", e)))?;

    Ok(ApiResponse::ok(updates))
}

/// POST /api/skills/sync - Sync skill from one executor to others
///
/// 允许 `agents` 作为 source（只读 → 复制到其他执行器），但**禁止**作为 target
/// （避免误覆盖 `~/.agents/skills/` 里的内容）。
pub async fn sync_skill(
    State(_state): State<AppState>,
    ApiJson(req): ApiJson<SyncRequest>,
) -> Result<ApiResponse<String>, AppError> {
    // source 接受 ExecutorType 或 `agents`（只读）
    let source_dir = executor_skills_dir_str(&req.source_executor)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown source executor: {}", req.source_executor)))?;

    // 统一 containment 校验
    // 404（NotFound）在这里对用户不友好，转化为带上下文的 BadRequest
    //
    // 注意：SKILL.md 中 YAML front matter 定义的 name 可能与磁盘目录名不一致。
    // 例如 SKILL.md 中写 `name: r2-backup` 但目录名是 `imported-skill`。
    // `resolve_skill_path_under` 是按磁盘路径查找的，如果按 name 找不到，
    // 需要 fallback 到扫描所有子目录，匹配 YAML 中定义的 name。
    let skill_dir = resolve_skill_path_under(&source_dir, &req.skill_name)
        .or_else(|e| {
            // 按 name 直接 join 找不到时，尝试扫描所有子目录匹配 YAML front matter 中的 name
            if matches!(e, AppError::NotFound) {
                // 扫描 source_dir 下所有 skill 子目录，匹配 SKILL.md 的 YAML name
                let mut found: Option<PathBuf> = None;
                if let Ok(entries) = std::fs::read_dir(&source_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if !path.is_dir() {
                            continue;
                        }
                        let skill_md = path.join("SKILL.md");
                        if skill_md.exists() {
                            if let Ok(content) = std::fs::read_to_string(&skill_md) {
                                if let Some(yaml) = extract_yaml_front_matter(&content) {
                                    for line in yaml.lines() {
                                        if let Some(val) = line.strip_prefix("name:") {
                                            let yaml_name = val.trim().trim_matches('"').to_string();
                                            if yaml_name == req.skill_name {
                                                found = Some(path);
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if found.is_some() {
                            break;
                        }
                    }
                }
                match found {
                    Some(path) => Ok(path),
                    None => Err(AppError::BadRequest(format!(
                        "Skill '{}' not found in executor '{}' (directory: {})",
                        req.skill_name,
                        req.source_executor,
                        source_dir.display()
                    ))),
                }
            } else {
                Err(e)
            }
        })?;

    let mut synced = Vec::new();
    let mut errors = Vec::new();

    for target in &req.target_executors {
        // target 拒绝只读来源
        if is_readonly_skill_source(target) {
            errors.push(format!(
                "Target executor '{}' is read-only; cannot sync into it",
                target
            ));
            continue;
        }
        let target_et = match crate::adapters::parse_executor_type(target) {
            Some(et) => et,
            None => {
                errors.push(format!("Unknown target executor: {}", target));
                continue;
            }
        };

        let target_dir = match executor_skills_dir(target_et) {
            Some(d) => d,
            None => {
                errors.push(format!("No skills directory for {}", target));
                continue;
            }
        };

        // Create target skills directory if needed
        std::fs::create_dir_all(&target_dir)
            .map_err(|e| AppError::Internal(format!("Failed to create target dir: {}", e)))?;

        // Flatten directory: take only the last part of the skill name
        // e.g., "creative/joke-teller" -> "joke-teller"
        // 防御：先 trim 末尾 '/'，再 fallback 整体，保证 target_skill_name 永不为空
        // （否则 dest = target_dir.join("") 会指向 skills 根目录，触发误删）
        let trimmed = req.skill_name.trim_end_matches('/');
        let target_skill_name = trimmed.rsplit('/').next().unwrap_or(trimmed);
        if target_skill_name.is_empty() || target_skill_name.contains('/') {
            errors.push(format!("Invalid skill name '{}' for sync target", req.skill_name));
            continue;
        }
        let dest = target_dir.join(target_skill_name);

        // Use temporary directory for atomic replace
        let temp_dest = target_dir.join(format!("{}.tmp.{}", target_skill_name, std::process::id()));

        // Clean up any existing temp dir from previous failed runs
        if temp_dest.exists() {
            let _ = std::fs::remove_dir_all(&temp_dest);
        }

        // Copy to temporary directory
        match copy_dir_recursive_flat(&skill_dir, &temp_dest, true) {
            Ok(_) => {
                // Remove existing destination if present
                if dest.exists() {
                    if let Err(e) = std::fs::remove_dir_all(&dest) {
                        errors.push(format!("Failed to remove existing {}: {}", target, e));
                        let _ = std::fs::remove_dir_all(&temp_dest);
                        continue;
                    }
                }

                // Atomically rename temp to destination
                if let Err(e) = std::fs::rename(&temp_dest, &dest) {
                    // On some systems rename cannot overwrite, try copy+remove
                    if let Err(e2) = copy_dir_recursive_flat(&temp_dest, &dest, true) {
                        errors.push(format!("Failed to sync to {}: {} (rename failed: {})", target, e2, e));
                        let _ = std::fs::remove_dir_all(&temp_dest);
                        continue;
                    }
                    let _ = std::fs::remove_dir_all(&temp_dest);
                }
                synced.push(format!("{} ({})", target, target_skill_name));
            }
            Err(e) => {
                let _ = std::fs::remove_dir_all(&temp_dest);
                errors.push(format!("Failed to sync to {}: {}", target, e));
            }
        }
    }

    if synced.is_empty() && !errors.is_empty() {
        return Err(AppError::BadRequest(errors.join("; ")));
    }

    let mut msg = format!("Synced '{}' (flattened) to: {}", req.skill_name, synced.join(", "));
    if !errors.is_empty() {
        msg.push_str(&format!(" | Errors: {}", errors.join("; ")));
    }

    Ok(ApiResponse::ok(msg))
}

/// Copy directory recursively, optionally flattening subdirectories
fn copy_dir_recursive_flat(src: &std::path::Path, dst: &std::path::Path, flatten: bool) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let file_name = entry.file_name();

        if src_path.is_dir() {
            if flatten {
                // When flattening, copy files directly without subdirectory structure
                // e.g., skill_dir/creative/something -> dest/something
                copy_dir_recursive_flat(&src_path, dst, flatten)?;
            } else {
                // Preserve structure
                let dst_path = dst.join(&file_name);
                copy_dir_recursive_flat(&src_path, &dst_path, flatten)?;
            }
        } else {
            std::fs::copy(&src_path, dst.join(&file_name))?;
        }
    }
    Ok(())
}


/// GET /api/skills/invocations - List skill invocation records
pub async fn list_invocations(
    State(state): State<AppState>,
    Query(query): Query<InvocationQuery>,
) -> Result<ApiResponse<PaginatedInvocations>, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let offset = ((page - 1).max(0)) * limit;

    let invocations = state.db.get_skill_invocations(
        offset,
        limit,
        query.skill_name.as_deref(),
        query.executor.as_deref(),
    ).await.map_err(|e| AppError::Internal(e.to_string()))?;

    let total = state.db.get_skill_invocations_count(
        query.skill_name.as_deref(),
        query.executor.as_deref(),
    ).await.map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(ApiResponse::ok(PaginatedInvocations {
        items: invocations,
        total,
        page,
        limit,
    }))
}

/// POST /api/skills/invocations - Record a skill invocation
pub async fn record_invocation(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<RecordInvocationRequest>,
) -> Result<ApiResponse<i64>, AppError> {
    let id = state.db.record_skill_invocation(
        &req.skill_name,
        &req.executor,
        req.todo_id,
        &req.status,
        req.duration_ms,
    ).await.map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(ApiResponse::ok(id))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;
    use crate::models::ExecutorType;

    // ── executor_label() tests ───────────────────────────────────────────

    #[test]
    fn test_executor_label_kilo() {
        assert_eq!(executor_label(ExecutorType::Kilo), "Kilo");
    }

    #[test]
    fn test_executor_label_all_known_executors() {
        // Regression guard: if a new executor is added to the enum but not to executor_label(),
        // the compiler will panic at runtime (non-exhaustive match). This test verifies the
        // known executor labels are correct and Kilo is included.
        assert_eq!(executor_label(ExecutorType::Claudecode), "Claude Code");
        assert_eq!(executor_label(ExecutorType::Hermes), "Hermes");
        assert_eq!(executor_label(ExecutorType::Codex), "Codex");
        assert_eq!(executor_label(ExecutorType::Codebuddy), "CodeBuddy");
        assert_eq!(executor_label(ExecutorType::Opencode), "Opencode");
        assert_eq!(executor_label(ExecutorType::Atomcode), "AtomCode");
        assert_eq!(executor_label(ExecutorType::Kimi), "Kimi");
        assert_eq!(executor_label(ExecutorType::Mobilecoder), "MobileCoder");
        assert_eq!(executor_label(ExecutorType::Codewhale), "CodeWhale");
        assert_eq!(executor_label(ExecutorType::Pi), "Pi");
        assert_eq!(executor_label(ExecutorType::Mimo), "MiMo");
        assert_eq!(executor_label(ExecutorType::Zhanlu), "Zhanlu");
        assert_eq!(executor_label(ExecutorType::Kilo), "Kilo");
    }

    // ── ALL_EXECUTORS array tests ────────────────────────────────────────

    #[test]
    fn test_all_executors_contains_kilo() {
        assert!(ALL_EXECUTORS.contains(&ExecutorType::Kilo),
            "ALL_EXECUTORS should contain ExecutorType::Kilo");
    }

    #[test]
    fn test_all_executors_count_is_thirteen() {
        // The comment says 13 = 12 old + Kilo. Guard the count so additions are noticed.
        assert_eq!(ALL_EXECUTORS.len(), 13,
            "ALL_EXECUTORS length mismatch; update the array and this test when adding executors");
    }

    #[test]
    fn test_all_executors_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for et in &ALL_EXECUTORS {
            assert!(seen.insert(et.as_str()),
                "Duplicate executor in ALL_EXECUTORS: {}", et.as_str());
        }
    }

    // ── executor_label_for_source() tests ───────────────────────────────

    #[test]
    fn test_executor_label_for_source_kilo() {
        assert_eq!(executor_label_for_source("kilo"), "Kilo");
    }

    #[test]
    fn test_executor_label_for_source_agents_is_special() {
        assert_eq!(executor_label_for_source("agents"), "Agents");
    }

    #[test]
    fn test_executor_label_for_source_unknown_returns_empty() {
        assert_eq!(executor_label_for_source("does_not_exist"), "");
    }

    // ── is_readonly_skill_source() tests ────────────────────────────────

    #[test]
    fn test_is_readonly_skill_source_agents() {
        assert!(is_readonly_skill_source("agents"));
    }

    #[test]
    fn test_is_readonly_skill_source_kilo_is_not_readonly() {
        assert!(!is_readonly_skill_source("kilo"));
    }

    // ── extract_yaml_front_matter() tests ───────────────────────────────

    #[test]
    fn test_extract_yaml_front_matter_basic() {
        let content = "---\nname: test\ndescription: a test skill\n---\nBody here";
        let yaml = extract_yaml_front_matter(content).unwrap();
        assert!(yaml.contains("name: test"));
        assert!(yaml.contains("description: a test skill"));
    }

    #[test]
    fn test_extract_yaml_front_matter_missing_returns_none() {
        let content = "No front matter here at all";
        assert!(extract_yaml_front_matter(content).is_none());
    }

    // ── parse_skill_yaml_header() tests ─────────────────────────────────

    #[test]
    fn test_parse_skill_yaml_header_complete() {
        let content = "---\nname: my-skill\ndescription: Does something useful\nversion: 1.2.3\nauthor: Alice\nlicense: MIT\n---\nBody";
        let meta = parse_skill_yaml_header(content);
        assert_eq!(meta.name, "my-skill");
        assert_eq!(meta.description, "Does something useful");
        assert_eq!(meta.version, Some("1.2.3".to_string()));
        assert_eq!(meta.author, Some("Alice".to_string()));
        assert_eq!(meta.license, Some("MIT".to_string()));
    }

    #[test]
    fn test_parse_skill_yaml_header_fallback_name_from_heading() {
        let content = "# My Skill Title\nSome description text here.";
        let meta = parse_skill_yaml_header(content);
        assert_eq!(meta.name, "My Skill Title");
    }

    // ── resolve_skill_path_for_read() tests ─────────────────────────────────

    #[test]
    fn test_resolve_skill_path_for_read_empty_name() {
        let base = Path::new("/skills");
        let result = resolve_skill_path_for_read(base, "");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_skill_path_for_read_absolute_path_rejected() {
        let base = Path::new("/skills");
        let result = resolve_skill_path_for_read(base, "/etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_skill_path_for_read_parent_traversal_rejected() {
        let base = Path::new("/skills");
        let result = resolve_skill_path_for_read(base, "../etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_skill_path_for_read_double_parent_traversal_rejected() {
        let base = Path::new("/skills");
        let result = resolve_skill_path_for_read(base, "foo/../../../etc/passwd");
        assert!(result.is_err());
    }
}
