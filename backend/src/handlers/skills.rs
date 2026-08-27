//! Skills management handler.
//!
//! Discovers skills from executor directories, provides comparison, sync,
//! and execution tracking APIs.

use axum::{
    Router,
    extract::{Query, State},
    routing::{get, post},
};
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
        // codewhale / kilo：历史 PR（#428 / Kilo）漏加了 skills 目录映射，
        // 导致 Skills 管理、对比矩阵、skill install 一直看不到这两个执行器；
        // 本次接 dsh 时一并补齐（session 目录见 adapters::EXECUTORS 注册表）
        "codewhale" => Some(home.join(".codewhale").join("skills")),
        "kilo" => Some(home.join(".kilo").join("skills")),
        "kimi" => Some(home.join(".kimi").join("skills")),
        "mobilecoder" => Some(home.join(".mobile-coder").join("skills")),
        "pi" => Some(home.join(".pi").join("skills")),
        "mimo" => Some(home.join(".local/share/mimocode").join("skills")),
        // Zhanlu: Issue #673 新增执行器，session 路径为 ~/.local/share/zhanlu/storage，
        // skills 目录与 session 目录同根
        "zhanlu" => Some(home.join(".local/share/zhanlu").join("skills")),
        // agents 是只读 skill 来源：扫描但不参与执行器管理/Todo 执行
        "agents" => Some(home.join(".agents").join("skills")),
        // dsh（DeepSeek Harness）：可写 skill 来源，非 ExecutorType、不参与 Todo 执行，
        // 但 delete/import/sync 等写操作全部放行（与只读的 agents 相区别）
        "dsh" => Some(home.join(".dsh").join("skills")),
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
pub(crate) fn resolve_skill_path_under(base: &Path, skill_name: &str) -> Result<PathBuf, AppError> {
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
    // 093-B4：原 13 臂 match 与 ExecutorDef.display_name 逐字重复，删除改查注册表；
    // 查不到时回退规范名（as_str），比 panic 宽容且不会返回空串。
    crate::adapters::find_executor_by_type(et)
        .map(|d| d.display_name)
        .unwrap_or_else(|| et.as_str())
}

// 仅用于本文件 tests 模块的数组完整性自检（元素齐全 / 无重复 / 含 Kilo）；
// 生产代码请用 ALL_SKILL_SOURCES。cfg(test) 避免该常量在非测试构建中成为 dead_code。
// 13 = 12 个旧执行器 + 新增的 Kilo
// 注意：加新执行器必须同时更新下面数组与本注释的计数，否则会出现 H1 同型错位。
#[cfg(test)]
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

// 注：原 SkillInvocation / PaginatedInvocations / InvocationQuery 仅服务于
// 前端「调用追踪」tab 的列表接口（GET /api/skills/invocations）。
// 该 tab 已移除，故删：
//   - SkillInvocation: 仅 PaginationInvocations.items 引用，删了 Paginated 后也死
//   - PaginatedInvocations: 列表接口返回值
//   - InvocationQuery: 列表接口 query 参数
// 保留：POST /api/skills/invocations（record_invocation）+ record_skill_invocation
// DB 方法——Dashboard 的「调用次数 / 成功率」走 db/dashboard.rs 聚合统计。

#[derive(Debug, Deserialize)]
pub struct SyncRequest {
    pub source_executor: String,
    pub skill_name: String,
    pub target_executors: Vec<String>,
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

/// 把绝对路径转成 ~ 相对路径（家目录前缀替换为 ~）。
///
/// 用途：skills_dir 会进入前端分享提示词（resource_dir），
/// 保留绝对路径会暴露家目录下的用户名；~ 相对路径让 AI 执行时自行展开。
fn home_relative(path: &std::path::Path) -> String {
    let abs = path.to_string_lossy().to_string();
    // 仅在路径确实位于家目录下时替换；否则原样返回（理论上不会发生）
    match dirs::home_dir() {
        Some(home) => match path.strip_prefix(&home) {
            Ok(rel) => format!("~/{rel}", rel = rel.display()),
            Err(_) => abs,
        },
        None => abs,
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

    // 提前转 ~ 相对路径一次（避免后续多次系统调用；同时供前端分享提示词引用，不暴露用户名）
    let dir_str = home_relative(&skills_dir);
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
    skills.sort_by_key(|a| a.name.to_lowercase());

    ExecutorSkills {
        executor: name.to_string(),
        executor_label: label.to_string(),
        skills_dir: dir_str,
        skills_dir_exists: exists,
        skills,
    }
}

// ── API handlers ────────────────────────────────────────────────────────

/// 参与 skill 扫描/对比的所有来源：13 个执行器 + 只读来源 `agents` + 可写来源 `dsh`。
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
    // codewhale/kilo 历史漏登记（见 executor_skills_dir_str 注释），随 dsh 一并补齐
    "codewhale", "kilo",
    "agents",
    "dsh",
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
        // dsh（DeepSeek Harness）：可写 skill 来源，UI 显示名
        "dsh" => "Dsh",
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
/// 扫描所有执行器之外，还扫 `~/.agents/skills`（只读）与 `~/.dsh/skills`（可写）两个非执行器来源。
/// agents 不参与 Todo 执行，但能在 Skills 总览/对比/同步里看到并使用。
///
/// 实现选择：每个来源的目录 IO 放在 `spawn_blocking` 里跑，
/// 因为 read_dir 在大目录（hermes 146 个 skill）下可能阻塞 tokio worker。
pub async fn list_skills(
    State(_state): State<AppState>,
) -> Result<ApiResponse<Vec<ExecutorSkills>>, AppError> {
    // spawn_blocking：磁盘 IO 不能跑在 tokio reactor 上，否则会卡住其他请求
    let result = tokio::task::spawn_blocking(move || {
        // 顺序遍历所有来源：单次调用只 IO 一次，顺序 vs 并行收益不大，
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
    // 用字符串映射而非 parse_executor_type：dsh 这类非 ExecutorType 的可写来源
    // 也能删除（映射不到的未知名字仍然 400，安全性不降级）
    let skills_dir = executor_skills_dir_str(&query.executor)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown executor: {}", query.executor)))?;

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

/// 校验 import 的 skill_name：**仅允许单层普通目录名**（不含分隔符、不以点开头）。
///
/// 为什么收紧到这个程度：
/// 1. 名字会被 format! 原样拼进 staging/backup 目录名——"./"、"." 一类输入能拼出含 ".."
///    的逃逸组件，在 canonicalize 拦截前就在 skills 根之外建目录/写文件（CodeRabbit
///    二轮实测路径）。字符串层面在这里一次性封死，比事后 canonicalize 更早、更可靠。
/// 2. 各执行器的 skill 目录都是 `<skills>/<name>/` 两层布局，嵌套名没有真实用途：
///    sync 目标本就 rsplit 取末段拍平，delete 也拒子路径；此前允许嵌套反而造成
///    「首次导入父目录不存在 → rename ENOENT → 500」的口径分裂。
/// 3. 与 delete_skill 的「拒 '/' 与 '..'」口径对齐，同一 skill 名在所有写接口下语义一致。
fn validate_import_skill_name(name: &str) -> Result<(), AppError> {
    if name.is_empty()
        || name.starts_with('.')
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
    {
        return Err(AppError::BadRequest(
            "Invalid skill name: expected a plain single-segment directory name (no separators, no leading dot, no traversal)"
                .to_string(),
        ));
    }
    // Windows 兜底：CI 产出 nt.exe，"C:foo" 这类盘符相对路径没有任何分隔符也能解析到
    // skills_dir 之外。Path::components() 按 OS 语义解析，Normal 之外的任意组件
    // （Prefix/RootDir/CurDir/ParentDir）一律拒绝。
    if !std::path::Path::new(name)
        .components()
        .all(|c| matches!(c, std::path::Component::Normal(_)))
    {
        return Err(AppError::BadRequest(
            "Invalid skill name: expected a plain single-segment directory name (no separators, no leading dot, no traversal)"
                .to_string(),
        ));
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
    // 用字符串映射而非 parse_executor_type：dsh 这类非 ExecutorType 的可写来源
    // 也能导入（映射不到的未知名字仍然 400，安全性不降级）
    let skills_dir = executor_skills_dir_str(&params.executor)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown executor: {}", params.executor)))?;

    std::fs::create_dir_all(&skills_dir)
        .map_err(|e| AppError::Internal(format!("Failed to create skills dir: {}", e)))?;

    // Decode zip
    let cursor = std::io::Cursor::new(body.to_vec());
    let mut archive = ZipArchive::new(cursor)
        .map_err(|e| AppError::BadRequest(format!("Invalid zip archive: {}", e)))?;

    let flatten = params.flatten.unwrap_or(true);
    let skill_name = params.skill_name.clone().unwrap_or_else(|| "imported-skill".to_string());

    // 校验 skill_name：空串/单点/绝对路径/父级穿越都会让 target_dir 偏离预期
    // （空与 "." 会让 join 解析回 skills 根，后续替换会删整个 skills 目录），
    // 在 join 前由专门函数统一拦截
    validate_import_skill_name(&skill_name)?;

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

    // 提取成功：原子替换 target_dir。
    // 「备份 → 换入 → 成功才删备份」的提交细节统一收敛到 commit_swap_with_backup：
    // 与 sync 的提交流程同源同语义，避免两处各自演化出不同的失败恢复口径。
    // 成功时 staging 已被 rename 消费、这里的 remove 是无害 no-op；
    // 失败时（备份失败/换入双失败）则负责把 staging 清掉，不让临时垃圾残留磁盘。
    let commit_result = commit_swap_with_backup(&staging_dir, &target_dir);
    let _ = std::fs::remove_dir_all(&staging_dir);
    // 错误统一按 Internal 返回（与历史「Failed to commit import」口径一致）
    commit_result.map_err(|e| AppError::Internal(format!("Failed to commit import: {}", e)))?;

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
/// 除执行器外还扫 `agents`/`dsh` 等非执行器来源，让用户
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
        // 非 ExecutorType 的 skill 来源（无 CLI 执行链，但目录可作为同步目标）：
        // agents（delete/import 仍只读保护）与 dsh（完全可写）。
        // 用集合而非字面量比较，后续新增非执行器来源只需往集合加一项。
        const NON_EXECUTOR_SOURCES: &[&str] = &["agents", "dsh"];
        let target_dir = if NON_EXECUTOR_SOURCES.contains(&target.as_str()) {
            // 这些来源不在 ExecutorType 枚举中，用 executor_skills_dir_str 单独解析
            match executor_skills_dir_str(target) {
                Some(d) => d,
                None => {
                    errors.push(format!("No skills directory for {}", target));
                    continue;
                }
            }
        } else {
            let target_et = match crate::adapters::parse_executor_type(target) {
                Some(et) => et,
                None => {
                    errors.push(format!("Unknown target executor: {}", target));
                    continue;
                }
            };
            match executor_skills_dir(target_et) {
                Some(d) => d,
                None => {
                    errors.push(format!("No skills directory for {}", target));
                    continue;
                }
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
        // temp 名加唯一后缀（PID+计数器）：并发同步同一 skill 到同一目标时，
        // 各请求持有独立临时目录，A 的清理动作不会删掉 B 正在拷贝的半成品
        let temp_dest = target_dir.join(format!(
            "{}.tmp.{}.{}",
            target_skill_name,
            std::process::id(),
            next_staging_id()
        ));

        // Clean up any existing temp dir from previous failed runs
        if temp_dest.exists() {
            let _ = std::fs::remove_dir_all(&temp_dest);
        }

        // Copy to temporary directory
        match copy_dir_recursive_flat(&skill_dir, &temp_dest, true) {
            Ok(_) => {
                // 提交走共享的「备份 → 换入 → 成功才删备份」流程：
                // 先删 dest 的旧写法在 rename+copy 双失败时会静默丢掉用户原 skill。
                // 本目标提交失败只记 errors、不中断剩余目标（与既有行为一致）；
                // 失败路径下 staging 会被留在原地，统一清掉再继续。
                let commit_result = commit_swap_with_backup(&temp_dest, &dest);
                let _ = std::fs::remove_dir_all(&temp_dest);
                match commit_result {
                    Ok(()) => synced.push(format!("{} ({})", target, target_skill_name)),
                    Err(msg) => errors.push(format!("Failed to sync to {}: {}", target, msg)),
                }
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

/// 把已就绪的 `temp` 目录原子换入 `dest`：「备份 → 换入 → 成功才删备份」。
///
/// 为什么不能「先删 dest 再 rename」：rename 与整树拷贝兜底双失败的瞬间，
/// 旧内容已经不存在了——调用方只能报错，但用户的原始 skill 无法恢复。
/// 先把 dest 挪进备份目录，保证任何一个失败点都能把现场原样还原：
/// - 备份这一步自身失败 → 什么都没动过，直接报错；
/// - rename(temp→dest) 失败 → 试整树拷贝兜底（部分文件系统不允许覆盖式 rename）；
///   兜底也失败时清掉半成品 dest、把备份 rename 回去再报错，原始数据完好；
/// - 只有提交确认成功后才删除备份。
///
/// 并发安全：备份目录名带 PID + staging 计数器唯一后缀。若同 skill 的并行操作
/// 共享一个备份名，本请求入口的清理会删掉对方刚移入的旧数据，对方提交失败后
/// 就无从恢复（CodeRabbit Major 级发现）；唯一名让每个请求的备份互不可见。
///
/// 错误统一返回 String 消息（调用方各自的 errors/AppError 口径自行包装）。
fn commit_swap_with_backup(temp: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    let parent = dest.parent().unwrap_or(dest);
    // 备份与 temp 同目录放置：保证同一文件系统内 rename 才能保持原子性
    let backup = parent.join(format!(
        "{}.old.tmp.{}.{}",
        dest.file_name().and_then(|n| n.to_str()).unwrap_or("skill"),
        std::process::id(),
        next_staging_id()
    ));
    let had_existing = dest.exists();
    if had_existing {
        std::fs::rename(dest, &backup)
            .map_err(|e| format!("Failed to backup existing destination: {}", e))?;
    }

    // 快路径：rename 直接换入成功
    if std::fs::rename(temp, dest).is_ok() {
        if had_existing {
            let _ = std::fs::remove_dir_all(&backup);
        }
        return Ok(());
    }

    // 慢路径：rename 失败时整树拷贝兜底（跨设备/不支持覆盖的 fs）
    match copy_dir_recursive_flat(temp, dest, true) {
        Ok(()) => {
            let _ = std::fs::remove_dir_all(temp);
            if had_existing {
                let _ = std::fs::remove_dir_all(&backup);
            }
            Ok(())
        }
        Err(copy_err) => {
            // 双失败：清掉半成品 dest 并还原备份。还原本身也失败时必须把该事实
            // 写进错误信息暴露给调用方——绝不能静默吞掉「现场已非原始状态」。
            let _ = std::fs::remove_dir_all(dest);
            let mut msg = format!("{} (rename fallback also failed)", copy_err);
            if had_existing {
                if let Err(e) = std::fs::rename(&backup, dest) {
                    msg.push_str(&format!(
                        "; CRITICAL: failed to restore backup {} back to {}: {}",
                        backup.display(),
                        dest.display(),
                        e
                    ));
                    return Err(msg);
                }
            }
            Err(msg)
        }
    }
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


/// POST /api/skills/invocations - Record a skill invocation
/// （原 GET 列表接口已删除——前端「调用追踪」tab 移除，没有读取方。
///  Dashboard 的「调用次数/成功率」走 db/dashboard.rs 聚合统计，
///  与此列表接口是不同的代码路径。）
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

/// v1 API 路由：所有路径使用完整的 `/api/v1/skills/...` 前缀，
/// 不与外层 router 嵌套（flat 结构）。
///
/// 映射规则：保持与 skills_routes() 相同的 handler 函数，仅路径前缀改为 /api/v1/skills。
pub fn v1_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/skills", get(list_skills).delete(delete_skill))
        .route("/api/v1/skills/compare", get(compare_skills))
        .route("/api/v1/skills/version-update", get(version_update_list))
        .route("/api/v1/skills/sync", post(sync_skill))
        .route("/api/v1/skills/invocations", post(record_invocation))
        .route("/api/v1/skills/content", get(get_skill_content))
        .route("/api/v1/skills/file", get(get_skill_file))
        .route("/api/v1/skills/export", get(export_skill))
        .route("/api/v1/skills/import", post(import_skill))
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

    /// 093-B4：label 改查注册表后，补「每个 ExecutorType 都有注册项 + 工厂可构造」断言——
    /// 这是「新增执行器忘注册/忘挂工厂」的回归守卫。
    #[test]
    fn test_find_executor_by_type_returns_registered_definitions_with_factories() {
        for et in [
            ExecutorType::Claudecode, ExecutorType::Hermes, ExecutorType::Codex,
            ExecutorType::Codebuddy, ExecutorType::Opencode, ExecutorType::Atomcode,
            ExecutorType::Kimi, ExecutorType::Mobilecoder, ExecutorType::Codewhale,
            ExecutorType::Pi, ExecutorType::Mimo, ExecutorType::Zhanlu, ExecutorType::Kilo,
        ] {
            let def = crate::adapters::find_executor_by_type(et)
                .unwrap_or_else(|| panic!("{et:?} 未在 EXECUTORS 注册表"));
            assert!(!def.display_name.is_empty());
            // 工厂必须能构造出提取器（验证 create_extractor 已正确挂载）
            let _extractor = (def.create_extractor)();
        }
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
    fn test_executor_label_for_source_dsh() {
        // dsh 是可写 skill 来源（非 ExecutorType），显示名走专门分支；
        // 若忘了加分支会返回空串，UI 退化显示原始名 "dsh"
        assert_eq!(executor_label_for_source("dsh"), "Dsh");
    }

    #[test]
    fn test_executor_skills_dir_str_dsh() {
        // 目录映射是 dsh 接入的唯一事实源：list/compare/sync/delete/import 全部经此解析
        let home = dirs::home_dir().expect("home 目录应存在");
        assert_eq!(
            executor_skills_dir_str("dsh"),
            Some(home.join(".dsh").join("skills"))
        );
    }

    #[test]
    fn test_all_skill_sources_contains_dsh() {
        // ALL_SKILL_SOURCES 驱动 list/compare/version-update 三个接口的来源枚举，
        // 漏加则 dsh 在总览/对比/版本检测里完全不可见
        assert!(ALL_SKILL_SOURCES.contains(&"dsh"),
            "ALL_SKILL_SOURCES should contain dsh");
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

    #[test]
    fn test_is_readonly_skill_source_dsh_is_writable() {
        // dsh 与 agents 的核心区别：可写来源，delete/import 不被只读守卫拦截
        assert!(!is_readonly_skill_source("dsh"));
    }

    #[test]
    fn test_validate_import_skill_name_rejects_empty_and_dot() {
        // 空串与单点会让 join 解析回 skills 根，触发「删除原目标」时清空整个 skills 目录；
        // 绝对路径与父级穿越维持既有拒绝。回归 CodeRabbit 指出的 dsh 可写后放大边界。
        assert!(validate_import_skill_name("").is_err());
        assert!(validate_import_skill_name(".").is_err());
        assert!(validate_import_skill_name("/abs/path").is_err());
        assert!(validate_import_skill_name("../up").is_err());
        // 合法名称放行：单层普通目录名（与 delete/sync 的单层口径一致）
        assert!(validate_import_skill_name("ntd-usage").is_ok());
    }

    #[test]
    fn test_validate_import_skill_name_rejects_separator_and_curdir_variants() {
        // 回归 CodeRabbit 二轮指出旁路："./"、"./." 不等于 "." 但同样解析回根，
        // 且经 format! 拼进 staging 目录名后会生成含 ".." 的逃逸组件，
        // 在 canonicalize 拦截前就已越界建目录。
        // 另回归嵌套相对路径：首次导入会因父目录缺失 ENOENT 500，
        // 口径统一为「仅允许单层目录名」后与 delete/sync 一致（sync 本来就拍平、delete 本来就拒子路径）。
        assert!(validate_import_skill_name("./").is_err());
        assert!(validate_import_skill_name("./.").is_err());
        assert!(validate_import_skill_name("./ntd-usage").is_err());
        assert!(validate_import_skill_name(".//").is_err());
        assert!(validate_import_skill_name("creative/joke-teller").is_err());
        // Windows 反斜杠分隔符同拒（CI 产出 windows 二进制）
        assert!(validate_import_skill_name("ns\\skill").is_err());
    }

    // ── commit_swap_with_backup() tests ─────────────────────────────────

    #[test]
    fn test_commit_swap_with_backup_replaces_existing_dest_cleanly() {
        // 更新场景回归：dest 已存在时必须整体换成新内容，且成功路径
        // 不得残留任何备份目录（旧行为「先删后换」在失败时会丢原数据）。
        let root = tempfile::tempdir().expect("tempdir");
        let dest = root.path().join("skill-a");
        std::fs::create_dir_all(dest.join("sub")).expect("create old skill dir");
        std::fs::write(dest.join("sub/old.txt"), b"old").expect("write old content");

        let staging = root.path().join(".skill-a.staging");
        std::fs::create_dir_all(&staging).expect("create staging");
        std::fs::write(staging.join("new.txt"), b"new").expect("write new content");

        commit_swap_with_backup(&staging, &dest).expect("replace should succeed");

        assert!(dest.join("new.txt").exists(), "新内容应就位");
        assert!(!dest.join("old.txt").exists(), "旧内容应被整体替换");
        assert!(!staging.exists(), "staging 应被 rename 消费");
        // 成功提交后备份目录必须清理干净，否则磁盘上会积累 .skill-a.old.tmp.* 垃圾
        let leftovers: Vec<String> = std::fs::read_dir(root.path())
            .expect("read root")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains(".old.tmp."))
            .collect();
        assert!(leftovers.is_empty(), "backup leaked: {:?}", leftovers);
    }

    #[test]
    fn test_commit_swap_with_backup_fresh_install_creates_dest() {
        // 首次安装场景：dest 不存在时不走备份分支也要能正确落位
        let root = tempfile::tempdir().expect("tempdir");
        let dest = root.path().join("brand-new");
        let staging = root.path().join(".brand-new.staging");
        std::fs::create_dir_all(&staging).expect("create staging");
        std::fs::write(staging.join("f.txt"), b"x").expect("write file");

        commit_swap_with_backup(&staging, &dest).expect("fresh install should succeed");

        assert!(dest.join("f.txt").exists(), "首次安装内容应就位");
        assert!(!staging.exists(), "staging 应被消费");
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

    #[test]
    fn test_home_relative_replaces_home_prefix() {
        // 家目录前缀应替换为 ~（分享提示词用，不暴露用户名）
        let home = dirs::home_dir().expect("home 目录应存在");
        let under_home = home.join(".claude").join("skills");
        let rel = home_relative(&under_home);
        assert!(rel.starts_with("~/"), "应转成 ~ 相对: {}", rel);
        assert!(!rel.contains("/Users/"), "不应暴露绝对路径前缀: {}", rel);
    }

    #[test]
    fn test_home_relative_outside_home_returns_absolute() {
        // 不在家目录下的路径原样返回（理论上不会发生，但保持语义正确）
        let rel = home_relative(std::path::Path::new("/tmp/ntd-skills"));
        assert_eq!(rel, "/tmp/ntd-skills");
    }
}
