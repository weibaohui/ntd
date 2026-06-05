//! Skills management handler.
//!
//! Discovers skills from executor directories, provides comparison, sync,
//! and execution tracking APIs.

use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
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
        "joinai" => Some(home.join(".joinai").join("skills")),
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

fn executor_label(et: ExecutorType) -> &'static str {
    match et {
        ExecutorType::Claudecode => "Claude Code",
        ExecutorType::Hermes => "Hermes",
        ExecutorType::Codex => "Codex",
        ExecutorType::Codebuddy => "CodeBuddy",
        ExecutorType::Opencode => "Opencode",
        ExecutorType::Atomcode => "AtomCode",
        ExecutorType::Kimi => "Kimi",
        ExecutorType::Joinai => "JoinAI",
    }
}

// 保留 ALL_EXECUTORS 供其他可能用到的代码；新代码请用 ALL_SKILL_SOURCES
#[allow(dead_code)]
const ALL_EXECUTORS: [ExecutorType; 8] = [
    ExecutorType::Claudecode,
    ExecutorType::Hermes,
    ExecutorType::Codex,
    ExecutorType::Codebuddy,
    ExecutorType::Opencode,
    ExecutorType::Atomcode,
    ExecutorType::Kimi,
    ExecutorType::Joinai,
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

/// 旧 API 的薄包装：保留供可能的内部调用，新代码请用 `discover_skills_for`
#[allow(dead_code)]
fn discover_skills_for_executor(et: ExecutorType) -> ExecutorSkills {
    // 直接走通用版本，ExecutorType 必然有 label
    discover_skills_for(et.as_str(), executor_label(et))
}

// ── API handlers ────────────────────────────────────────────────────────

/// 参与 skill 扫描/对比的所有来源：8 个执行器 + 只读来源 `agents`。
///
/// 用字符串数组而非 `ExecutorType` 数组，方便容纳非 ExecutorType 来源。
/// **新增来源时**：
/// 1. 在 `executor_skills_dir_str` 加分支
/// 2. 在本数组加字符串
/// 3. 如果不是 ExecutorType，在 `executor_label_for_source` 加显示名
const ALL_SKILL_SOURCES: &[&str] = &[
    "claudecode", "codebuddy", "opencode", "atomcode",
    "hermes", "kimi", "joinai", "codex",
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
/// 扫描 8 个 ExecutorType 之外，还扫 `~/.agents/skills`（只读 skill 来源）。
/// agents 不参与 Todo 执行，但能在 Skills 总览/对比/同步里看到并使用。
///
/// 实现选择：每个来源的目录 IO 放在 `spawn_blocking` 里跑，
/// 因为 read_dir 在大目录（hermes 146 个 skill）下可能阻塞 tokio worker。
pub async fn list_skills(
    State(_state): State<AppState>,
) -> Result<ApiResponse<Vec<ExecutorSkills>>, AppError> {
    // spawn_blocking：磁盘 IO 不能跑在 tokio reactor 上，否则会卡住其他请求
    let result = tokio::task::spawn_blocking(move || {
        // 顺序遍历 9 个来源：单次调用只 IO 一次，顺序 vs 并行收益不大，
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

    let skill_dir = skills_dir.join(&query.skill_name);
    if !skill_dir.exists() {
        return Err(AppError::NotFound);
    }

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

    let skill_dir = skills_dir.join(&query.skill_name);
    if !skill_dir.exists() {
        return Err(AppError::NotFound);
    }

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

    std::fs::create_dir_all(&target_dir)
        .map_err(|e| AppError::Internal(format!("Failed to create target dir: {}", e)))?;

    // Canonicalize target_dir to verify it's under skills_dir
    let target_dir = target_dir.canonicalize()
        .map_err(|e| AppError::Internal(format!("Failed to resolve target dir: {}", e)))?;

    // Verify target_dir is still under skills_dir
    let skills_dir_canonical = skills_dir.canonicalize()
        .map_err(|e| AppError::Internal(format!("Failed to resolve skills dir: {}", e)))?;
    if !target_dir.starts_with(&skills_dir_canonical) {
        return Err(AppError::BadRequest("Invalid skill name: would escape skills directory".to_string()));
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

        let dest_path = if flatten {
            // Flatten: extract directly to target dir, ignoring subdirectories
            target_dir.join(&file_name)
        } else {
            // Preserve structure
            target_dir.join(&outpath)
        };

        // Verify dest_path is still under target_dir
        if let Ok(dest_path_canonical) = dest_path.canonicalize() {
            if !dest_path_canonical.starts_with(&target_dir) {
                return Err(AppError::BadRequest(format!("Path escapes target directory: {}", outpath.display())));
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

/// GET /api/skills/compare - Cross-executor skill comparison matrix
///
/// 比 8 个 ExecutorType 多扫了 `agents`（`~/.agents/skills`），让用户
/// 能看到 "lark-doc" 这类 skill 在哪些来源里有、版本是不是落后。
///
/// 输出结构：每个 skill 一行，每个来源一列，单元格标记 present/version。
/// 这样前端可以画 N 行的对比表格，**任意两个来源**之间都能对比。
pub async fn compare_skills(
    State(_state): State<AppState>,
) -> Result<ApiResponse<Vec<SkillComparison>>, AppError> {
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

        // description 从任意一个有该 skill 的来源取（先到先得）
        // 选 first() 是因为 description 跨来源通常一致；不一致时以第一个来源为准
        let description = all_skills.values()
            .filter_map(|m| m.get(&name))
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

    Ok(ApiResponse::ok(comparisons))
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

    let skill_dir = source_dir.join(&req.skill_name);
    if !skill_dir.exists() {
        return Err(AppError::NotFound);
    }

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
        let target_skill_name = req.skill_name.split('/').next_back().unwrap_or(&req.skill_name);
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
        return Err(AppError::Internal(errors.join("; ")));
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
