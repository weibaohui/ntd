use axum::{
    extract::{Path, Query, State},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use super::{AppError, AppState};
use crate::models::ApiResponse;

// 096-W3-PR2：6 个 scan_X / get_X_detail 函数族已收编进 scanners 子模块（与其 Scanner 同址），
// 本文件只保留类型、trait、SCANNERS 注册表、扫描调度与 4 个真 handler
pub mod scanners;

use scanners::home_dir;
// iter_jsonl_files：仅本文件保留的边界测试（cfg(test)）直接调用，条件导入避免 lib 目标 unused
#[cfg(test)]
use scanners::iter_jsonl_files;
use scanners::{
    AtomCodeScanner, ClaudeCodeScanner, CodexScanner, HermesScanner, KimiScanner, PiScanner,
};

// ─── Request / Response types ─────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListSessionsQuery {
    pub page: Option<u64>,
    pub page_size: Option<u64>,
    pub status: Option<String>,    // "active" | "completed"
    pub source: Option<String>,    // filter by tool source: "claude-code", "codex", "hermes", etc.
    pub executor: Option<String>,  // filter by entrypoint
    pub project: Option<String>,   // filter by project path (partial match)
    pub search: Option<String>,    // search in first prompt
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub source: String,            // "claude-code" | "codex" | "hermes" | "kimi" | "atomcode" | "cc-connect"
    pub project_path: String,
    pub status: String,
    pub executor: String,
    pub model: String,
    pub git_branch: Option<String>,
    pub message_count: u32,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub first_prompt: Option<String>,
    pub created_at: Option<String>,
    pub last_active_at: Option<String>,
    pub file_size: u64,
    pub version: Option<String>,
    pub subagent_count: u32,
}

#[derive(Debug, Serialize)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionInfo>,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
}

#[derive(Debug, Serialize)]
pub struct SessionStats {
    pub total_sessions: u64,
    pub active_sessions: u64,
    pub today_sessions: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub by_source: HashMap<String, u64>,
    pub by_executor: HashMap<String, u64>,
    pub by_project: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionMessage {
    pub role: String,
    pub content_preview: String,
    pub model: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub timestamp: Option<String>,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubAgentInfo {
    pub agent_type: String,
    pub description: String,
    pub message_count: u32,
}

#[derive(Debug, Serialize)]
pub struct SessionDetail {
    pub info: SessionInfo,
    pub messages: Vec<SessionMessage>,
    pub subagents: Vec<SubAgentInfo>,
}

// ─── Helpers ──────────────────────────────────────────────




// ─── SessionScanner trait + registry ──────────────────────
//
// 抽象目标：让 6 个 executor 共享同一组「列出会话 / 取单个会话详情」的协议，
// 调用方通过 `&'static dyn SessionScanner` 派发而不是裸函数指针。
//
// 选 `&'static dyn Trait` 而不是 `Box<dyn Trait>` 是因为：
//   1) 6 个 scanner 是编译期已知的零大小单例（`ClaudeCodeScanner` 等都是 unit struct），
//      引用语义天然满足 'static + Send + Sync；
//   2) `Box<dyn>` 会让每次 `scan_for_executors` 触发堆分配，收益为 0；
//   3) `inventory` 之类的注册宏会引入新依赖，与本仓 YAGNI 原则冲突。
//
// `name()` 同时承担「executor 标识」和 `SessionInfo.source` 写入——
// 6 个 scanner 的 source 字符串与 name 完全一致（"claudecode" / "codex" / ...），
// 拆两个方法只会引入没有差异的样板。
pub trait SessionScanner: Send + Sync {
    /// executor 标识；用于 `get_scanner(name)` 查找,也是 SessionInfo.source 的字面值
    fn name(&self) -> &'static str;
    /// 扫描 home_dir 下的会话文件,追加到 `out`
    fn scan(&self, out: &mut Vec<SessionInfo>);
    /// 按 session_id 取单个会话的完整详情,找不到返回 None
    fn get_detail(&self, session_id: &str) -> Option<SessionDetail>;
}

/// 全局 scanner 注册表。顺序即 `get_session_detail` 的回退顺序。
///
/// 用 `static` 数组保证 'static 生命周期,scanner 本身是零大小 struct
/// (`PhantomData` 都不需要) 不会增加可执行文件体积。
pub static SCANNERS: &[&'static dyn SessionScanner] = &[
    &ClaudeCodeScanner,
    &CodexScanner,
    &HermesScanner,
    &KimiScanner,
    &AtomCodeScanner,
    &PiScanner,
];

// ─── Unified scan ─────────────────────────────────────────


/// 把 `home_dir` 转成完整路径,并沿 `scan_for_executors` 旧行为——
/// 若 `session_dir` 配了 `~` 前缀,做一次 `~` 展开,然后判断目录是否存在。
///
/// 提到独立函数以满足 ≤30 行函数体约束,逻辑上 1:1 对应旧版 `if !exists` 分支。
fn exec_session_dir_exists(exec: &crate::models::ExecutorConfig) -> bool {
    if exec.session_dir.is_empty() {
        // 空 session_dir 视为"用 home_dir 下的默认路径"——scanner 内部自行定位,
        // 此处不阻断,沿用旧行为。
        return true;
    }
    let expanded = exec.session_dir.replace('~', &dirs::home_dir().unwrap_or_default().to_string_lossy());
    std::path::Path::new(&expanded).exists()
}

/// 在 SCANNERS 中按 name 查找。返回 trait object 引用,生命周期 'static
/// 由注册表保证;`exec.name` 不在注册表(如 codebuddy / opencode 等未实现
/// session 存储的 executor)时返回 None,沿用旧 `get_scanner` 的 None 语义。
fn get_scanner(name: &str) -> Option<&'static dyn SessionScanner> {
    SCANNERS.iter().copied().find(|s| s.name() == name)
}

fn scan_for_executors(enabled_executors: &[crate::models::ExecutorConfig]) -> Vec<SessionInfo> {
    let mut sessions = Vec::new();

    for exec in enabled_executors {
        // session_dir 显式配置但目录不存在 → 跳过该 executor,沿用旧逻辑
        if !exec_session_dir_exists(exec) {
            continue;
        }

        if let Some(scanner) = get_scanner(&exec.name) {
            scanner.scan(&mut sessions);
        }
    }

    // Sort by last_active_at descending
    sessions.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));
    sessions
}

// ─── Handlers ─────────────────────────────────────────────

/// 对已排序/过滤后的切片做分页，返回 (total, 当前页数据)。
/// 调用方负责把 page/page_size clamp 到合法范围（page>=1, page_size>=1）；
/// 本函数只做切片算术，即便传入越界 page 也安全返回空页而非 panic——
/// 关键防御：`start = (page-1)*page_size` 在 page=0 时会下溢，故调用方必须 .max(1)。
/// 抽成纯函数是为了让 page=0 / 越界 / 末页不足等边界可被单测直接覆盖。
fn paginate<T: Clone>(items: &[T], page: u64, page_size: u64) -> (u64, Vec<T>) {
    let total = items.len() as u64;
    // page 已保证 >=1，故 (page-1) 不会下溢；page_size 同理 >=1。
    let start = ((page - 1) * page_size) as usize;
    let end = start.saturating_add(page_size as usize).min(items.len());
    let page_data = if start < items.len() {
        items[start..end].to_vec()
    } else {
        // start 越过末尾（如 page 过大）→ 空页，而非 panic。
        Vec::new()
    };
    (total, page_data)
}

// ─── Session scan cache（091 性能优化）─────────────────────
// scan_for_executors 会遍历多个执行器的 session 目录、逐文件解析 JSONL，是重磁盘 IO。
// list_sessions / get_session_stats 在翻页、搜索、切页时反复触发它；前端无轮询，重复扫盘
// 全来自这些交互。用 30s TTL 缓存 + 单飞锁（双重检查）把同一 executor 配置下的并发扫描收敛成一次。
struct SessionsCacheEntry {
    sessions: Vec<SessionInfo>,
    expires_at: Instant,
}

static SESSIONS_CACHE: LazyLock<RwLock<HashMap<String, SessionsCacheEntry>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// 单飞锁：缓存过期时并发刷新只放行一个去扫盘，其余等锁后走双重检查命中（仿 DASHBOARD_REFRESH_LOCK）。
static SESSIONS_SCAN_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

/// 按 (name, session_dir) 生成 executor 配置签名作为缓存键：配置变更（启停执行器/改目录）会落到
/// 不同 key，避免读到旧配置的缓存；排序保证顺序无关。
fn executors_signature(executors: &[crate::models::ExecutorConfig]) -> String {
    // 收集 (name, session_dir) 引用对，排序后拼成稳定字符串。
    let mut sig: Vec<(&str, &str)> = executors
        .iter()
        .map(|e| (e.name.as_str(), e.session_dir.as_str()))
        .collect();
    sig.sort_unstable();
    sig.iter()
        .map(|(n, d)| format!("{n}={d}"))
        .collect::<Vec<_>>()
        .join("|")
}

/// 读取未过期的缓存项（快路径抽函数，保持 handler 与单飞段都 <30 行）。
async fn sessions_cache_get_fresh(key: &str) -> Option<Vec<SessionInfo>> {
    let cache = SESSIONS_CACHE.read().await;
    let entry = cache.get(key)?;
    if entry.expires_at > Instant::now() {
        Some(entry.sessions.clone())
    } else {
        None
    }
}

/// 取（缓存命中或单飞重扫）全量 sessions。30s TTL；扫描任务 panic 时若有旧值则 stale 回退。
/// 入参取 owned Vec 便于 move 进 spawn_blocking 闭包。
async fn cached_scan_sessions(
    executors: Vec<crate::models::ExecutorConfig>,
) -> Result<Vec<SessionInfo>, AppError> {
    let key = executors_signature(&executors);
    // 快路径：未过期直接命中，避开磁盘扫描。
    if let Some(v) = sessions_cache_get_fresh(&key).await {
        return Ok(v);
    }
    // 单飞：并发过期刷新只放行一个去扫盘。
    let _permit = SESSIONS_SCAN_LOCK.lock().await;
    // 双重检查：等锁期间可能已有并发请求完成扫描。
    if let Some(v) = sessions_cache_get_fresh(&key).await {
        return Ok(v);
    }
    // 取旧值备用：扫描 panic 时 stale-while-revalidate，返旧值比 5xx 更有用。
    let stale = SESSIONS_CACHE
        .read()
        .await
        .get(&key)
        .map(|e| e.sessions.clone());
    match tokio::task::spawn_blocking(move || scan_for_executors(&executors)).await {
        Ok(sessions) => {
            SESSIONS_CACHE.write().await.insert(
                key.clone(),
                SessionsCacheEntry {
                    sessions: sessions.clone(),
                    expires_at: Instant::now() + Duration::from_secs(30),
                },
            );
            Ok(sessions)
        }
        Err(e) => {
            if let Some(stale) = stale {
                tracing::warn!("session scan task failed, returning stale cache: {e}");
                Ok(stale)
            } else {
                Err(AppError::Internal(e.to_string()))
            }
        }
    }
}

pub async fn list_sessions(
    State(state): State<AppState>,
    Query(query): Query<ListSessionsQuery>,
) -> Result<ApiResponse<SessionListResponse>, AppError> {
    // page=0 会让后面的 (page-1) 在 debug 下下溢 panic、release 下 wrap 成巨数后静默返回空页，
    // 故对 page 兜底 .max(1)；page_size 同时 clamp 到 [1,100]，避免 0 或极大值导致空页/超大切片。
    // 与 handlers/execution.rs:35 的分页兜底写法保持一致。
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 100);

    let executors = state
        .db
        .get_enabled_executors()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    // 091：scan_for_executors 是重磁盘扫描，30s TTL 缓存把翻页/搜索/切页的重复扫盘收敛成一次。
    let mut sessions = cached_scan_sessions(executors).await?;

    // 过滤：全部为纯内存 retain，无需 spawn_blocking。
    if let Some(ref status) = query.status {
        sessions.retain(|s| &s.status == status);
    }
    if let Some(ref source) = query.source {
        sessions.retain(|s| s.source == *source);
    }
    if let Some(ref executor) = query.executor {
        sessions.retain(|s| s.executor == *executor);
    }
    if let Some(ref project) = query.project {
        sessions.retain(|s| s.project_path.contains(project));
    }
    if let Some(ref search) = query.search {
        let search_lower = search.to_lowercase();
        sessions.retain(|s| {
            s.first_prompt
                .as_ref()
                .map(|p| p.to_lowercase().contains(&search_lower))
                .unwrap_or(false)
        });
    }

    // 分页算术抽到 paginate 纯函数，便于单测覆盖 page=0 / 越界 / 末页不足等边界。
    let (total, page_data) = paginate(&sessions, page, page_size);
    Ok(ApiResponse::ok(SessionListResponse {
        sessions: page_data,
        total,
        page,
        page_size,
    }))
}

pub async fn get_session_stats(
    State(state): State<AppState>,
) -> Result<ApiResponse<SessionStats>, AppError> {
    let executors = state
        .db
        .get_enabled_executors()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    // 091：与 list_sessions 共用 30s 缓存，stats 在缓存结果上聚合，避免重复扫盘。
    let sessions = cached_scan_sessions(executors).await?;
    Ok(ApiResponse::ok(compute_session_stats(&sessions)))
}

/// 在已扫描的 sessions 上聚合统计（纯函数，便于单测）。密集的数据归并，强行拆分会破坏
/// 单一聚合的完整性，属可豁免的密集统计场景。
fn compute_session_stats(sessions: &[SessionInfo]) -> SessionStats {
    let now = chrono::Utc::now();
    // and_hms_opt(0,0,0) 对任何合法日期都返回 Some——午夜零点永远有效
    #[allow(clippy::unwrap_used)]
    let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();

    let mut by_source: HashMap<String, u64> = HashMap::new();
    let mut by_executor: HashMap<String, u64> = HashMap::new();
    let mut by_project: HashMap<String, u64> = HashMap::new();
    let mut active_count = 0u64;
    let mut today_count = 0u64;
    let mut total_in = 0u64;
    let mut total_out = 0u64;
    for s in sessions {
        *by_source.entry(s.source.clone()).or_insert(0) += 1;
        *by_executor.entry(s.executor.clone()).or_insert(0) += 1;
        *by_project.entry(s.project_path.clone()).or_insert(0) += 1;
        total_in += s.total_input_tokens;
        total_out += s.total_output_tokens;
        if s.status == "active" {
            active_count += 1;
        }
        if let Some(ref created) = s.created_at {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(created) {
                if dt.naive_utc() >= today_start {
                    today_count += 1;
                }
            }
        }
    }
    SessionStats {
        total_sessions: sessions.len() as u64,
        active_sessions: active_count,
        today_sessions: today_count,
        total_input_tokens: total_in,
        total_output_tokens: total_out,
        by_source,
        by_executor,
        by_project,
    }
}

pub async fn get_session_detail(
    State(_state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<ApiResponse<SessionDetail>, AppError> {
    // 通过 SCANNERS 注册表顺序遍历,与重构前 `if let Some(d) = get_X_detail(...)`
    // 的回退顺序一致——遇到第一个命中的 scanner 即返回,未命中走 NotFound。
    let detail = tokio::task::spawn_blocking(move || {
        for scanner in SCANNERS {
            if let Some(d) = scanner.get_detail(&session_id) {
                return Some(d);
            }
        }
        None
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    match detail {
        Some(d) => Ok(ApiResponse::ok(d)),
        None => Err(AppError::NotFound),
    }
}

pub async fn delete_session(
    State(_state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    tokio::task::spawn_blocking(move || {
        // Try to delete from each source
        let claude_dir = home_dir().join(".claude/projects");
        if let Ok(entries) = std::fs::read_dir(&claude_dir) {
            for entry in entries.flatten() {
                let jsonl = entry.path().join(format!("{}.jsonl", session_id));
                if jsonl.exists() {
                    let _ = std::fs::remove_file(&jsonl);
                    let dir = jsonl.with_extension("");
                    if dir.is_dir() { let _ = std::fs::remove_dir_all(&dir); }
                    return true;
                }
            }
        }
        false
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(ApiResponse::ok(()))
}

// ─── V1 routes ────────────────────────────────────────────

/// V1 路由:所有路径以 `/api/v1/sessions` 开头,不再嵌套到模块级 Router 下。
/// 全局资源保持平铺路径,仅增加版本号前缀 `/api/v1`。
pub fn v1_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/sessions", get(list_sessions))
        .route("/api/v1/sessions/stats", get(get_session_stats))
        .route("/api/v1/sessions/{id}", get(get_session_detail).delete(delete_session))
}

// ─── Detail getters per source ────────────────────────────






// ─── Pi Detail ──────────────────────────────────────────────


// ─── SessionScanner dispatch tests (issue #617) ──────────
//
// 覆盖目标:`get_scanner` 派发语义与重构前完全一致;
// 验证 trait object 的引入没有改变外部可观察行为。
// 这些 case 是纯派发测试,不需要构造 home_dir / 文件系统,
// 因为 SCANNERS 表里的 scan_fn 在 home_dir 不存在时一律早返回。

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod session_scanner_dispatch_tests {
    use super::*;

    /// 6 个已知 scanner 都能在派发表里找到,这是 #617 验收
    /// "所有现有 scanner 仍可被 dispatch" 的基础保证。
    #[test]
    fn get_scanner_resolves_all_known_executors() {
        for name in ["claudecode", "codex", "hermes", "kimi", "atomcode", "pi"] {
            let scanner = get_scanner(name)
                .unwrap_or_else(|| panic!("scanner for {name} should be Some, was None"));
            // 派发得到的 scanner 自报的 name 必须和查询 key 一致;
            // 这一条是 trait object 派发与原 match 派发语义等价的硬性证据。
            assert_eq!(scanner.name(), name);
        }
    }

    /// 重构前的 match 显式列出"无 session 存储"的 executor 名 (codebuddy 等)
    /// 也应当返 None;行为必须与原来逐分支对照一致。
    /// Issue #673 新增的 zhanlu 也属于「暂无 scanner」的范畴,与 opencode/mimo 同列。
    #[test]
    fn get_scanner_returns_none_for_unsupported_executors() {
        for name in ["codebuddy", "opencode", "mobilecoder", "mimo", "zhanlu"] {
            assert!(
                get_scanner(name).is_none(),
                "scanner for {name} should be None (no session storage found)"
            );
        }
    }

    /// 任何不在表内的随机名都返 None,call site 用 `if let Some` 兜底。
    /// 这条 case 防止"加新 executor 名时忘记 push 进 SCANNERS"被静默吞掉。
    #[test]
    fn get_scanner_returns_none_for_unknown_name() {
        for name in ["", "claude-code", "CLAUDECODE", "ClaudeCode", "unknown_tool", "  "] {
            assert!(
                get_scanner(name).is_none(),
                "scanner for {name:?} should be None (not in SCANNERS table)"
            );
        }
    }

    /// 派发得到的 trait object 必须满足 Send + Sync,
    /// 才能在 `spawn_blocking` / `tokio::task::spawn_blocking` 等并发场景使用。
    /// 用 fn 指针约束编译期,运行时不需要真起线程。
    /// `?Sized` 是因为 `dyn SessionScanner` 是 unsized,直接传 `&T` 时
    /// 默认 `T: Sized` 会编译失败。
    #[test]
    fn returned_scanner_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync + ?Sized>(_: &T) {}
        for name in ["claudecode", "codex", "hermes", "kimi", "atomcode", "pi"] {
            let s = get_scanner(name).expect("known scanner");
            assert_send_sync(s);
        }
    }

    /// trait object 派发要能在 `Vec` 上追加结果(不 panic / 不污染已有元素)。
    /// home_dir 在 CI/测试机为空时,所有 scanner 都早返回,只验证"调用路径通"。
    #[test]
    fn scanner_scan_appends_without_panicking() {
        let scanner = get_scanner("pi").expect("pi scanner exists");
        let mut out: Vec<SessionInfo> = Vec::new();
        // 即使 home_dir 缺 .pi/agent/sessions 也不应 panic,允许 out 为空。
        scanner.scan(&mut out);
        // 不强断言 out.len(),因为测试机上可能存在真实 .pi 数据;
        // 这里只确认"调用未 panic + 保留已存在元素"。
        // 加一个 sentinel 验证 out 没有被清空:
        let sentinel = SessionInfo {
            session_id: "sentinel".into(),
            source: "sentinel".into(),
            project_path: String::new(),
            status: "completed".into(),
            executor: String::new(),
            model: String::new(),
            git_branch: None,
            message_count: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            first_prompt: None,
            created_at: None,
            last_active_at: None,
            file_size: 0,
            version: None,
            subagent_count: 0,
        };
        out.push(sentinel.clone());
        scanner.scan(&mut out);
        assert!(
            out.iter().any(|s| s.session_id == "sentinel"),
            "scanner.scan() must not clear `out`; sentinel should still be present"
        );
    }

    /// 回归用例:`scan_for_executors` 在传入空 executor 列表时,必须返空 vec。
    /// 重构前 match 分支一个不命中 → 0 次 scan,行为应保持。
    #[test]
    fn scan_for_executors_with_empty_list_returns_empty() {
        let sessions = scan_for_executors(&[]);
        assert!(sessions.is_empty(), "no executors → no sessions");
    }

    /// 回归用例:传入不支持的 executor 名,应当静默跳过(对应 None 分支),
    /// 不污染结果 vec。这里只验证派发层(不构造 ExecutorConfig 数组,
    /// 因为字段较多且不参与"派发是否跳过"的判断);
    /// 派发拿到 None → `scan_for_executors` 里的 `if let Some(scanner) = ...`
    /// 分支就不会进入,等价于"被静默跳过"。
    #[test]
    fn scan_for_executors_skips_unknown_executors() {
        assert!(get_scanner("codebuddy").is_none());
        assert!(get_scanner("definitely-not-a-real-executor").is_none());
    }
}




// ─── SessionScanner trait + registry tests ─────────────────
//
// 覆盖 6 个 scanner impl 的 name() 派发 + 注册表查找 + 异名容错:
//  - name() 必须与原 scan_X / get_X_detail 内部 hardcode 的 source 字符串一致,
//    否则 SessionInfo.source 会被改写,违反"不改 SessionInfo 字段语义"约束。
//  - SCANNERS 长度必须是 6,保证不漏注册 scanner。
//  - get_scanner("unknown") 返回 None,沿用旧 `match _ => None` 行为。
//  - iter_jsonl_files 在空目录 / 不存在目录 / 混合扩展名 下的边界。
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod session_scanner_tests {
    use super::*;

    /// 锁定 6 个 scanner 的 name() 输出,与原 match 分支的 source 字符串一一对应。
    /// 任一字符串漂移都会触发其它 session 测试的 SessionInfo.source 断言失败。
    #[test]
    fn scanner_name_matches_existing_source_strings() {
        assert_eq!(ClaudeCodeScanner.name(), "claudecode");
        assert_eq!(CodexScanner.name(), "codex");
        assert_eq!(HermesScanner.name(), "hermes");
        assert_eq!(KimiScanner.name(), "kimi");
        assert_eq!(AtomCodeScanner.name(), "atomcode");
        assert_eq!(PiScanner.name(), "pi");
    }

    /// SCANNERS 注册表长度 = 6,与原 match 分支的 Some(...) 数量一致。
    /// 若日后新增 scanner,这里需要同步增加——这正是「单一注册表」想暴露的回归点。
    #[test]
    fn scanners_registry_has_six_entries() {
        assert_eq!(SCANNERS.len(), 6);
    }

    /// get_scanner(name) 对 6 个合法 name 都返回 Some,对未知 name 返 None。
    /// 这条同时验证 SCANNERS 顺序无关性——所有合法 name 都能命中,无所谓注册顺序。
    #[test]
    fn get_scanner_dispatches_all_six_names() {
        for name in ["claudecode", "codex", "hermes", "kimi", "atomcode", "pi"] {
            assert!(get_scanner(name).is_some(), "scanner {name} should be registered");
            assert_eq!(get_scanner(name).unwrap().name(), name);
        }
        // 旧 match 中显式 None 的几个 executor 继续走 None 分支；
        // Issue #673 新增的 zhanlu 同样不在 SCANNERS 内（与 opencode/mimo 一致）
        for name in ["codebuddy", "opencode", "mobilecoder", "mimo", "zhanlu", "unknown", ""] {
            assert!(get_scanner(name).is_none(), "scanner {name} should NOT be registered");
        }
    }

    /// 验证 `get_scanner` 返回的 trait object 是 'static 引用,与 SCANNERS 同生命周期。
    /// 这条主要防止有人误把 SCANNERS 改成 Vec/Box 后导致签名破坏。
    #[test]
    fn get_scanner_returns_static_dyn() {
        let s: &'static dyn SessionScanner = get_scanner("claudecode").expect("registered");
        // 触发 vtable 调用,确认 dyn dispatch 路径通
        let _ = s.name();
    }

    /// iter_jsonl_files 在目录不存在 / 目录存在但为空 / 混合扩展名 三种输入下的行为。
    /// 防御"目录不可读时 panic"——返回空 Vec 是最稳的退化形式。
    #[test]
    fn iter_jsonl_files_handles_missing_or_empty_dir() {
        let missing = std::path::Path::new("/tmp/__ntd_no_such_dir_iter_jsonl_tests__");
        assert!(iter_jsonl_files(missing).is_empty());

        let tmp = std::env::temp_dir().join("__ntd_iter_jsonl_empty__");
        let _ = std::fs::create_dir_all(&tmp);
        assert!(iter_jsonl_files(&tmp).is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// iter_jsonl_files 过滤非 .jsonl 文件,只产出 .jsonl 条目。
    #[test]
    fn iter_jsonl_files_filters_by_extension() {
        let tmp = std::env::temp_dir().join("__ntd_iter_jsonl_filter__");
        let _ = std::fs::create_dir_all(&tmp);
        let _ = std::fs::write(tmp.join("a.jsonl"), "line\n");
        let _ = std::fs::write(tmp.join("b.json"), "{}");
        let _ = std::fs::write(tmp.join("c.txt"), "x");
        let _ = std::fs::write(tmp.join("nested.jsonl"), ""); // 同名扩展名应被收录

        let got = iter_jsonl_files(&tmp);
        let names: Vec<String> = got.into_iter().map(|(_, n)| n).collect();
        // 只列顶层;同目录的 a.jsonl / nested.jsonl 都在
        assert!(names.contains(&"a.jsonl".to_string()));
        assert!(names.contains(&"nested.jsonl".to_string()));
        assert!(!names.iter().any(|n| n == "b.json" || n == "c.txt"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// exec_session_dir_exists 必须与旧 `if !exec.session_dir.is_empty() { 展开 ~ + exists }` 行为一致:
    ///   - 空 session_dir → true (走 scanner 内部 home_dir 定位)
    ///   - 配 ~ 且目录不存在 → false
    ///   - 配 ~ 且目录存在 → true
    #[test]
    fn exec_session_dir_exists_behavior() {
        let mk = |d: &str| crate::models::ExecutorConfig {
            id: 0,
            name: "x".into(),
            path: String::new(),
            enabled: true,
            display_name: "x".into(),
            session_dir: d.into(),
            is_default: false,
            default_model: None,
            supports_models: false,
            created_at: None,
            updated_at: None,
        };
        assert!(exec_session_dir_exists(&mk("")), "empty session_dir should not block");
        // 配一个几乎肯定不存在的路径
        let p = mk("/__ntd_no_such_path_for_session_dir_test__");
        assert!(!exec_session_dir_exists(&p));
        // 配一个真实存在的临时目录
        let tmp = std::env::temp_dir().to_string_lossy().to_string();
        assert!(exec_session_dir_exists(&mk(&tmp)));
    }
}


// 分页纯函数的边界单测：page=0 由调用方 .max(1) 兜底，这里直接验算术安全性。
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod paginate_tests {
    use super::*;

    #[test]
    fn test_paginate_first_page() {
        // 25 项、page=1 size=10 → 返回前 10，total=25。
        let items: Vec<u32> = (0..25).collect();
        let (total, page) = paginate(&items, 1, 10);
        assert_eq!(total, 25);
        assert_eq!(page, (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn test_paginate_last_page_partial() {
        // 25 项、page=3 size=10 → 末页只剩 5 项，不越界。
        let items: Vec<u32> = (0..25).collect();
        let (total, page) = paginate(&items, 3, 10);
        assert_eq!(total, 25);
        assert_eq!(page, (20..25).collect::<Vec<_>>());
    }

    #[test]
    fn test_paginate_page_beyond_end_returns_empty() {
        // page 远超总页数 → 空页而非 panic，total 仍正确。
        let items: Vec<u32> = (0..5).collect();
        let (total, page) = paginate(&items, 100, 10);
        assert_eq!(total, 5);
        assert!(page.is_empty());
    }

    #[test]
    fn test_paginate_empty_input() {
        // 空切片、任意 page → 空页，total=0。
        let items: Vec<u32> = Vec::new();
        let (total, page) = paginate(&items, 1, 10);
        assert_eq!(total, 0);
        assert!(page.is_empty());
    }
}

// 091 session 扫描缓存：executors_signature 纯函数 + 缓存 TTL 命中/过期 + stats 聚合单测。
// 单飞锁（SESSIONS_SCAN_LOCK）是机械的标准模式，与 DASHBOARD_REFRESH_LOCK 同构，不做并发单测。
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod sessions_cache_tests {
    use super::*;
    use crate::models::ExecutorConfig;

    // 构造 ExecutorConfig：缓存签名只读 name + session_dir，其余字段填零值占位。
    fn exec_cfg(name: &str, session_dir: &str) -> ExecutorConfig {
        ExecutorConfig {
            id: 0,
            name: name.into(),
            path: String::new(),
            enabled: true,
            display_name: String::new(),
            session_dir: session_dir.into(),
            is_default: false,
            default_model: None,
            supports_models: false,
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn test_executors_signature_order_independent_and_content_based() {
        // 相同执行器集合、顺序不同 → 同一签名（缓存键稳定，不因顺序抖动 miss）。
        let a = vec![exec_cfg("claude-code", "/a"), exec_cfg("codex", "/b")];
        let b = vec![exec_cfg("codex", "/b"), exec_cfg("claude-code", "/a")];
        assert_eq!(executors_signature(&a), executors_signature(&b));
        // session_dir 变更 → 签名不同（配置漂移后不会读到旧目录的缓存）。
        assert_ne!(
            executors_signature(&[exec_cfg("claude-code", "/a")]),
            executors_signature(&[exec_cfg("claude-code", "/c")]),
        );
    }

    #[tokio::test]
    async fn test_sessions_cache_get_fresh_returns_some_before_expiry() {
        // 直接写一条未过期项：命中应返回 Some（快路径成立）。
        let key = "test_fresh_unique_key_091";
        SESSIONS_CACHE.write().await.insert(
            key.into(),
            SessionsCacheEntry {
                sessions: vec![],
                expires_at: Instant::now() + Duration::from_secs(60),
            },
        );
        assert!(sessions_cache_get_fresh(key).await.is_some());
        // 用唯一 key 并在用例末尾移除，避免污染并行跑的其他用例（全局 static）。
        SESSIONS_CACHE.write().await.remove(key);
    }

    #[tokio::test]
    async fn test_sessions_cache_get_fresh_returns_none_after_expiry() {
        // 写一条已过期项：命中应返回 None（触发上层重扫）。
        let key = "test_stale_unique_key_091";
        SESSIONS_CACHE.write().await.insert(
            key.into(),
            SessionsCacheEntry {
                sessions: vec![],
                expires_at: Instant::now() - Duration::from_secs(1),
            },
        );
        assert!(sessions_cache_get_fresh(key).await.is_none());
        SESSIONS_CACHE.write().await.remove(key);
    }

    // 构造最小 SessionInfo：compute_session_stats 只读 source/status/tokens 等字段，其余填零值。
    fn session(source: &str, status: &str, tokens: u64) -> SessionInfo {
        SessionInfo {
            session_id: String::new(),
            source: source.into(),
            project_path: "/p".into(),
            status: status.into(),
            executor: "claude-code".into(),
            model: String::new(),
            git_branch: None,
            message_count: 0,
            total_input_tokens: tokens,
            total_output_tokens: tokens,
            first_prompt: None,
            created_at: None,
            last_active_at: None,
            file_size: 0,
            version: None,
            subagent_count: 0,
        }
    }

    #[test]
    fn test_compute_session_stats_aggregates_counts_and_tokens() {
        let sessions = vec![
            session("claude-code", "active", 100),
            session("claude-code", "completed", 50),
            session("codex", "active", 10),
        ];
        let stats = compute_session_stats(&sessions);
        assert_eq!(stats.total_sessions, 3);
        assert_eq!(stats.active_sessions, 2, "两个 active");
        // created_at 均为 None → today_sessions 不计。
        assert_eq!(stats.today_sessions, 0);
        assert_eq!(*stats.by_source.get("claude-code").unwrap(), 2);
        assert_eq!(*stats.by_source.get("codex").unwrap(), 1);
        // 每条 input=output=tokens，求和即 100+50+10。
        assert_eq!(stats.total_input_tokens, 160);
        assert_eq!(stats.total_output_tokens, 160);
    }

    #[test]
    fn test_compute_session_stats_empty() {
        let stats = compute_session_stats(&[]);
        assert_eq!(stats.total_sessions, 0);
        assert_eq!(stats.active_sessions, 0);
        assert!(stats.by_source.is_empty());
    }
}
