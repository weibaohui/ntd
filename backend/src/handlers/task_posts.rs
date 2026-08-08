//! 任务讨论区 API（需求 060：论坛跟帖 + @专家/@执行器 触发执行后回帖）。
//!
//! 设计要点：
//! - 人帖（kind=human）：纯 Markdown 评论，直接入库。
//! - 智能体帖（kind=agent）：人帖里含 @专家/@执行器 时触发。流程为
//!   「建隐藏载体 Todo（todo_type=4）→ start_todo_execution（spawn 后立即返回 record_id）
//!   → 写 running 占位帖」；执行完成时由 completion.rs 的 discussion 分支回写结论。
//! - 执行系统是 Todo 中心的，必须借载体 Todo 承载 executor/prompt/expert_name。

use std::sync::{Arc, RwLock};

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::adapters::{find_executor, ExecutorRegistry};
use crate::config::Config;
use crate::db::Database;
use crate::db::entity::{task_posts, tasks};
use crate::db::task_post::{
    NewPost, KIND_AGENT, KIND_HUMAN, STATUS_FAILED, STATUS_RUNNING, STATUS_SUCCESS,
};
use crate::expert::ExpertIndexManager;
use crate::executor_service::{run_todo_execution_boxed, ExecEvent, RunTodoExecutionRequest};
use crate::handlers::execution::start_todo_execution;
use crate::handlers::{AppError, AppState};
use crate::models::{ApiResponse, ExecutionStatus};
use crate::task_manager::TaskManager;

/// 讨论触发用的 trigger_type：completion.rs 据此回写智能体占位帖。
/// 委派任务首帖也复用此值（首帖等同人工 @ 触发）；自动接力（P2）用 "discussion_auto"。
pub(crate) const TRIGGER_DISCUSSION: &str = "discussion";

/// 自动接力专用 trigger_type（需求 092 P2）：管家调度触发的执行用此值。
/// completion.rs 对 discussion / discussion_auto 同样走讨论回写，并由 delegate_relay
/// 推进下一轮接力；与人工 discussion 区分，便于日志/统计与防递归识别。
pub(crate) const TRIGGER_DISCUSSION_AUTO: &str = "discussion_auto";

/// 内联到 carrier prompt 的最近主楼层数；平衡「上下文充分」与「prompt 体积」。
/// 被 @ 的 AI 既能看到即时上下文，又可用 ntd 命令拉取更多，故取较小的 5。
const DISCUSSION_HISTORY_LIMIT: u64 = 5;

/// 单条历史正文的字符截断上限，防止长帖（如执行器长结论）撑爆 prompt。
const HISTORY_POST_TRUNCATE: usize = 500;

// ---------------------------------------------------------------------------
// 请求 / 响应结构
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreatePostRequest {
    pub content: String,
    /// 回复指定楼层（楼中楼）；None=主楼层。应用层校验目标必须为主楼层（深度≤1）。
    pub parent_post_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ListPostsQuery {
    /// 页码从 1 起（缺省 1）。
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

/// 提及的传输结构（序列化进 task_posts.mentions JSON）。
#[derive(Debug, Serialize)]
pub struct MentionDto {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub display: String,
}

// ---------------------------------------------------------------------------
// 路由
// ---------------------------------------------------------------------------

/// 讨论帖路由，挂在任务路由下：`/api/v1/workspaces/{ws}/tasks/{id}/posts`。
pub fn task_post_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/workspaces/{ws}/tasks/{id}/posts",
            get(list_posts).post(create_post),
        )
        .route(
            "/api/v1/workspaces/{ws}/tasks/{id}/posts/{pid}",
            get(get_post).delete(delete_post),
        )
}

// ---------------------------------------------------------------------------
// @ 解析（纯函数，便于单测）
// ---------------------------------------------------------------------------

/// 判定一个字符是否可作为 @token 的组成部分。
/// 字母/数字（含 CJK）/下划线/连字符/点继续；遇空格或标点则结束。
fn is_mention_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '.'
}

/// 从正文中抽取所有 `@token` 原始串（未解析），按出现顺序返回。
/// 纯函数：只做文本切分，不触碰 DB / state，便于单测覆盖边界。
pub(crate) fn extract_at_tokens(content: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = content.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '@' {
            continue;
        }
        // @ 之后贪心 consume，直到非 mention 字符。
        let mut tok = String::new();
        while let Some(&nc) = chars.peek() {
            if is_mention_char(nc) {
                tok.push(nc);
                chars.next();
            } else {
                break;
            }
        }
        if !tok.is_empty() {
            tokens.push(tok);
        }
    }
    tokens
}

/// 把已解析的 MentionDto 列表序列化成 mentions 列要存的 JSON 字符串。
fn serialize_mentions(mentions: &[MentionDto]) -> String {
    serde_json::to_string(mentions).unwrap_or_else(|_| "[]".to_string())
}

/// 解析 @token 为结构化提及：先专家后执行器；都不中则忽略（当普通文本）。
/// 同名歧义按需求§10/设计§6.4「先专家后执行器」消歧——单 token 的判定交给 [`classify_mention`]。
///
/// 入参只取 `expert_manager`（不耦合整个 AppState）：一是本函数仅用它查专家，二是
/// 需求 092 P2 的 completion 自动接力要复用同一份解析逻辑（那边只有 Arc 组件、无 AppState），
/// 拆参后人工发帖与自动接力共用，零解析逻辑漂移。
pub(crate) async fn resolve_mentions(
    tokens: &[String],
    expert_manager: &crate::expert::ExpertIndexManager,
) -> Vec<MentionDto> {
    let mut out: Vec<MentionDto> = Vec::new();
    for tok in tokens {
        // get_expert_by_name 是 parking_lot 同步读；命中结果交给纯函数分类，保持本函数薄。
        let expert = expert_manager.get_expert_by_name(tok);
        if let Some(m) = classify_mention(tok, expert.as_ref()) {
            out.push(m);
        }
    }
    out
}

/// 对单个 @token 分类：先专家后执行器，都不中返回 None。
///
/// 提取为纯函数（不 async、不碰 DB），便于单测同名消歧顺序（需求§10）。
/// `expert` 由调用方先查 expert_manager 传入，避免本函数耦合 AppState。
fn classify_mention(tok: &str, expert: Option<&crate::expert::ExpertMetadata>) -> Option<MentionDto> {
    // 先专家：同名时人设优先（专家是更具体的语义实体，比执行器优先）。
    if let Some(meta) = expert {
        return Some(MentionDto {
            kind: "expert".to_string(),
            name: meta.name.clone(),
            display: meta.display_name_zh.clone().unwrap_or_else(|| meta.name.clone()),
        });
    }
    // 再执行器：名/别名大小写不敏感（find_executor 内部 trim + lowercase）。
    find_executor(tok).map(|def| MentionDto {
        kind: "executor".to_string(),
        name: def.name.to_string(),
        display: def.display_name.to_string(),
    })
}

/// 取提及里的首个执行器名（决定由哪个执行器承载）。
fn first_executor(mentions: &[MentionDto]) -> Option<&str> {
    mentions.iter().find(|m| m.kind == "executor").map(|m| m.name.as_str())
}

/// 取提及里的首个专家名（决定套哪个人设）。
fn first_expert(mentions: &[MentionDto]) -> Option<&str> {
    mentions.iter().find(|m| m.kind == "expert").map(|m| m.name.as_str())
}

/// 智能体帖展示作者：优先专家、其次执行器。
fn agent_author(mentions: &[MentionDto]) -> String {
    first_expert(mentions)
        .or_else(|| first_executor(mentions))
        .unwrap_or("智能体")
        .to_string()
}

// ---------------------------------------------------------------------------
// 执行触发
// ---------------------------------------------------------------------------

/// 拼装发给执行器的 message：任务上下文 + 讨论历史 + 本次诉求 + ntd 命令用法。
///
/// 注入任务 id / workspace id / 最近讨论，让被 @ 的 AI 不再只看到当前一条楼层，
/// 而能了解讨论全貌；并附 ntd 命令，AI 可按需拉取更多（仿 gh pr view --comments）。
/// 专家人设由 inject_expert_context 按 todo.expert_name 自动前置，这里不重复。
fn build_carrier_prompt(task: &tasks::Model, post_content: &str, history_text: &str) -> String {
    // workspace_id 回退到 0 仅作文案占位；讨论触发时 require_task_in_ws 已保证
    // task.workspace_id 与 URL 里的 ws 一致，正常路径不会命中回退分支。
    let ws_id = task.workspace_id.unwrap_or(0);
    // 历史为空时给明确占位，避免 AI 误判「历史段缺失」为读取异常。
    let history_section = if history_text.trim().is_empty() {
        "（暂无历史）".to_string()
    } else {
        history_text.to_string()
    };
    format!(
        "你被 @ 到「任务 #{id}（工作空间 #{ws}）」的讨论区。请基于上下文给出可直接用于回复的结论（Markdown）。\n\n\
         任务标题：{title}\n任务需求：\n{desc}\n\n\
         ## 既有讨论上下文（最近 {n} 条，按时间正序）\n{history}\n\n\
         ## 本次讨论诉求\n{content}\n\n\
         ## 了解全貌的 ntd 命令（默认连本地 ntd，无需额外参数）\n\
         - 任务全貌：ntd task view --workspace-id {ws} --task {id}\n\
         - 完整讨论历史：ntd task posts --workspace-id {ws} --task {id} list",
        id = task.id,
        ws = ws_id,
        title = task.title,
        desc = task.description,
        history = history_section,
        content = post_content,
        n = DISCUSSION_HISTORY_LIMIT,
    )
}

/// 拉最近 N 条主楼层讨论帖（N=DISCUSSION_HISTORY_LIMIT），失败静默回退空 Vec。
///
/// 讨论历史注入是增强项，读库失败不应阻断 @ 触发——故 unwrap_or_default 兜底，
/// 与 inject_workspace_prompt / inject_expert_context 的降级哲学保持一致。
async fn fetch_recent_main_posts(db: &Database, task_id: i64) -> Vec<task_posts::Model> {
    // 用「最新 N 条」查询（DESC 取 N 再反转）：被 @ 的 AI 需要最近上下文，而非最早的。
    db.list_recent_main_posts(task_id, DISCUSSION_HISTORY_LIMIT)
        .await
        .unwrap_or_default()
}

/// 把帖子列表格式化为逐条文本「- [作者(身份) 状态] 正文」，每条正文截断防膨胀。
///
/// 纯函数（无 IO），便于单测覆盖截断 / 空列表 / 多条等边界。身份(kind)与状态(status)
/// 都暴露，让 AI 能区分人帖与智能体帖、并看到历史中失败(failed)的回复。
fn format_discussion_history(posts: &[task_posts::Model]) -> String {
    posts
        .iter()
        .map(|p| {
            // 正文超长按 char 边界截断，保留前若干字符 + 省略号，控制 prompt 体积。
            let truncated = truncate_chars(&p.content, HISTORY_POST_TRUNCATE);
            format!("- [{}({}) {}] {}", p.author_name, p.kind, p.status, truncated)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 按字符（非字节）截断到 max_chars，超长追加省略号。
///
/// 单独抽出是因为 Rust 的 &str 是 UTF-8，直接按字节下标切片会 panic；
/// 讨论正文多为中文，必须按 char 边界截断。
fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max_chars).collect();
    t.push('…');
    t
}

/// 按 workspace_id 反查项目目录；查不到或失败返回空串（执行器降级用默认 workspace）。
/// 任务表不存 workspace_path，必须经此查 workspace 表补齐执行所需的真实路径。
///
/// 入参只取 `db`（不耦合 AppState）：需求 092 P2 的自动接力在 completion 路径触发，那边
/// 只有 Arc<Database>、无 AppState；拆参后人工 @ 触发与自动接力共用同一份路径解析。
async fn resolve_ws_path(db: &Database, ws_id: i64) -> String {
    db.get_project_directory_by_id(ws_id)
        .await
        .ok()
        .flatten()
        .map(|p| p.path)
        .unwrap_or_default()
}

/// 建 carrier Todo 并触发执行，返回 (carrier_todo_id, record_id)。
///
/// start_todo_execution 内部 tokio::spawn 后立即返回 record_id，因此本函数不阻塞，
/// HTTP 请求可即刻返回占位帖。执行落定时由 completion.rs discussion 分支回写。
///
/// 豁免说明（>50 行）：主体是线性管道——取执行器/专家 → 解析 ws_path → 建 carrier todo
/// → 触发执行 → 取 record_id；其中 RunTodoExecutionRequest 构造是连续的纯数据赋值块
/// （无 if/else/循环），符合规范豁免场景 #1（纯数据构建）+ #2（线性管道），拆分会把请求
/// 字段打散成多参数函数、反而割裂阅读，故保留单函数。
async fn trigger_discussion_execution(
    state: &AppState,
    task: &tasks::Model,
    mentions: &[MentionDto],
    carrier_prompt: &str,
    trigger_type: &str,
) -> Result<(i64, i64), AppError> {
    let executor_name = first_executor(mentions);
    let expert_name = first_expert(mentions);
    let ws_id = task.workspace_id.unwrap_or(1);
    // 任务表不存 workspace_path，按 workspace_id 反查项目目录取真实路径（执行需要）。
    let ws_path = resolve_ws_path(state.db.as_ref(), ws_id).await;
    let title = format!("讨论触发 @{}", agent_author(mentions));
    let todo_id = state
        .db
        .create_discussion_todo(
            title,
            carrier_prompt.to_string(),
            executor_name,
            expert_name,
            ws_id,
            &ws_path,
        )
        .await?;
    // 构造执行请求：trigger_type 由调用方传入（discussion=人工/委派首帖，
    // discussion_auto=自动接力），completion.rs 据此走对应回写分支；
    // expert_manager 注入让 @专家 时 inject_expert_context 能加载人设。
    let result = start_todo_execution(RunTodoExecutionRequest {
        db: state.db.clone(),
        executor_registry: state.executor_registry.clone(),
        tx: state.tx.clone(),
        task_manager: state.task_manager.clone(),
        config: state.config.clone(),
        todo_id,
        message: carrier_prompt.to_string(),
        req_executor: executor_name.map(|s| s.to_string()),
        req_model: None,
        trigger_type: trigger_type.to_string(),
        params: None,
        resume_session_id: None,
        resume_message: None,
        source_todo_id: Some(todo_id),
        source_todo_title: Some(format!("讨论帖-任务{}", task.id)),
        loop_step_execution_id: None,
        step_id: None,
        feishu_bot_id: None,
        feishu_receive_id: None,
        feishu_receive_id_type: None,
        workspace_path: if ws_path.is_empty() { None } else { Some(ws_path) },
        workspace_id: Some(ws_id),
        expert_manager: Some(state.expert_manager.clone()),
    })
    .await;
    // start_todo_execution 失败时软删已建的载体 todo，避免残留（review suggestion）；
    // 成功时取 record_id（为 None 视为启动失败，同样软删）。
    let record_id = match result {
        Ok(r) => r
            .record_id
            .ok_or_else(|| AppError::Internal("执行启动失败：未获取到执行记录 ID".to_string()))?,
        Err(e) => {
            let _ = state.db.soft_delete_todo(todo_id).await;
            return Err(e);
        }
    };
    Ok((todo_id, record_id))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET .../tasks/{id}/posts — 主楼层分页列表，每条主楼层附带其楼中楼 replies。
///
/// 按主楼层分页（避免一次拉全量）；楼中楼随当前页主楼层批量取回并组装成树，
/// 前端拿到即可直接渲染、无需再分组、无 N+1。响应含 total/page/limit 供翻页。
pub async fn list_posts(
    State(state): State<AppState>,
    Path((ws, task_id)): Path<(i64, i64)>,
    Query(q): Query<ListPostsQuery>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    // 校验 task 属于 path 的 workspace，防跨 ws 越权读讨论帖。
    require_task_in_ws(state.db.as_ref(), task_id, ws).await?;
    let page = q.page.unwrap_or(1).max(1);
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let db = &state.db;
    // 当前页主楼层 + 总数（两次轻查询）。
    let main_posts = db
        .list_main_posts_paged(task_id, page, limit)
        .await
        .map_err(AppError::from)?;
    let total = db
        .count_main_posts(task_id)
        .await
        .map_err(AppError::from)?;
    // 任务级 running 总数（跨页），用于 Tab 角标；与当前页分离，避免翻页时角标跳变。
    let running_total = db
        .count_running_posts(task_id)
        .await
        .map_err(AppError::from)?;
    // 当前页主楼层各自的楼中楼：一次 IN 查询批量取回，再组装挂载。
    let parent_ids: Vec<i64> = main_posts.iter().map(|p| p.id).collect();
    let replies = db
        .list_replies_for(task_id, &parent_ids)
        .await
        .map_err(AppError::from)?;
    let items = build_post_tree(&main_posts, replies);
    Ok(ApiResponse::ok(serde_json::json!({
        "items": items,
        "total": total,
        "running_total": running_total,
        "page": page,
        "limit": limit,
    })))
}

/// 把主楼层与其楼中楼组装成树：每条主楼层挂其 replies（按 id ASC）。
///
/// 纯函数（不触碰 DB），便于单测。楼中楼按 parent_post_id 归属对应主楼层；
/// 未匹配到主楼层的孤儿回复被丢弃（当前页主楼层必含其楼中楼的 parent，理论不出现）。
fn build_post_tree(
    main_posts: &[task_posts::Model],
    replies: Vec<task_posts::Model>,
) -> Vec<serde_json::Value> {
    use std::collections::HashMap;
    // 先按 parent_post_id 分桶，避免对每个主楼层线性扫描。
    let mut bucket: HashMap<i64, Vec<serde_json::Value>> = HashMap::new();
    for r in replies {
        if let Some(pid) = r.parent_post_id {
            let val = serde_json::to_value(&r).unwrap_or(serde_json::Value::Null);
            bucket.entry(pid).or_default().push(val);
        }
    }
    main_posts
        .iter()
        .map(|m| attach_replies(m, bucket.remove(&m.id)))
        .collect()
}

/// 把楼中楼 replies 挂到主楼层序列化结果上（无则空数组），保证前端 replies 字段恒存在。
fn attach_replies(main: &task_posts::Model, replies: Option<Vec<serde_json::Value>>) -> serde_json::Value {
    let mut v = serde_json::to_value(main).unwrap_or(serde_json::Value::Null);
    if let Some(obj) = v.as_object_mut() {
        obj.insert("replies".to_string(), serde_json::Value::Array(replies.unwrap_or_default()));
    }
    v
}

/// GET .../tasks/{id}/posts/{pid} — 单帖（前端轮询占位帖状态用）。
pub async fn get_post(
    State(state): State<AppState>,
    Path((ws, task_id, pid)): Path<(i64, i64, i64)>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    // 校验 task 属该 ws（防越权），再校验 post 属该 task。
    require_task_in_ws(state.db.as_ref(), task_id, ws).await?;
    let post = state
        .db
        .get_task_post(pid)
        .await
        .map_err(AppError::from)?
        .ok_or(AppError::NotFound)?;
    if post.task_id != task_id {
        return Err(AppError::NotFound);
    }
    // Model → Value：与 list/delete 返回类型统一为 ApiResponse<Value>。
    Ok(ApiResponse::ok(
        serde_json::to_value(&post).unwrap_or(serde_json::Value::Null),
    ))
}

/// DELETE .../tasks/{id}/posts/{pid} — 删帖。
///
/// 删 running 智能体帖时联动取消其后台执行，否则执行落定后 completion.rs 的 discussion
/// 分支会再回写出一条结论帖——用户删帖的意图被旁路。人帖与已落定（success/failed）帖直接删。
/// 鉴权（仅作者可删）见 #6：项目无用户系统，暂不区分，先按内容删。
pub async fn delete_post(
    State(state): State<AppState>,
    Path((ws, task_id, pid)): Path<(i64, i64, i64)>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    // 校验 task 属该 ws（防越权），再取帖并校验 post 属该 task。
    require_task_in_ws(state.db.as_ref(), task_id, ws).await?;
    // 先取帖：判断是否 running 智能体帖；是则先取消后台执行，再删帖。
    let post = state
        .db
        .get_task_post(pid)
        .await
        .map_err(AppError::from)?
        .ok_or(AppError::NotFound)?;
    if post.task_id != task_id {
        return Err(AppError::NotFound);
    }
    if post.kind == KIND_AGENT && post.status == STATUS_RUNNING {
        if let Some(record_id) = post.source_execution_id {
            cancel_running_post_execution(&state, record_id).await;
        }
    }
    let affected = state
        .db
        .delete_task_post(pid)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::ok(serde_json::json!({ "deleted": affected })))
}

/// 取消某执行记录的后台进程（删 running 帖时联动调用）。
///
/// 复用 `stop_execution_handler` 的内核思路：`record.task_id`（spawn 时生成的 UUID）
/// → `task_manager.cancel` 发取消信号；若任务已不在 manager（自然结束/崩溃）但记录仍
/// running，则 `force_fail` 兜底清理悬挂。取消失败只记日志、不阻断删帖——删帖是用户
/// 明确意图，不应因取消信号未命中而回滚。
async fn cancel_running_post_execution(state: &AppState, record_id: i64) {
    // 记录不存在或已非 running：无需取消（get 失败也静默，删帖照常进行）。
    let Ok(Some(rec)) = state.db.get_execution_record(record_id).await else {
        return;
    };
    if rec.status != ExecutionStatus::Running {
        return;
    }
    // task_id 为 spawn 时生成的 UUID；命中则发取消信号，执行内部 cancel 分支负责更新 DB。
    let cancelled = match rec.task_id.as_deref() {
        Some(tid) => state.task_manager.cancel(tid).await,
        None => false,
    };
    if !cancelled {
        // 已不在 manager 但记录仍 running：强制置 failed，避免悬挂记录。
        if let Err(e) = state.db.force_fail_execution_record(record_id).await {
            tracing::warn!(error = %e, record_id, "force_fail execution record on post delete failed");
        }
    }
}

/// 在任务讨论区落地一条「含 @ 的人帖」，并（当 @ 到执行器/专家时）触发对应执行、写智能体占位帖。
///
/// 供 [`create_post`] handler（人工发帖）与 [`crate::handlers::tasks::create_task`]（委派任务
/// 首帖）共用，避免两处各写一遍「解析 @ → 写人帖 → 建载体 todo → 触发执行 → 写占位帖」，
/// 防止逻辑漂移。`trigger_type` 控制完成回写分支：人工/委派首帖传 [`TRIGGER_DISCUSSION`]，
/// 自动接力（P2）将传 `discussion_auto`。
///
/// 入参 `content` 由调用方先 trim 判空；`parent_post_id` 决定是主楼层还是楼中楼。
/// `force_mention`：调用方已服务端校验存在的强制触发目标（委派首帖用）；传 Some 时直接注入
/// mentions，绕过文本 @ 解析——避免 assignee 名字含空格/标点时 extract_at_tokens 截断 token、
/// 首帖 @ 不命中导致静默不触发执行（人工发帖传 None，仍走文本解析）。
/// 返回 `(人帖 JSON, 智能体占位帖 Option)`——未 @ 到执行器/专家且无 force 时第二项为 None（纯评论）。
pub(crate) async fn land_mention_post(
    state: &AppState,
    task: &tasks::Model,
    content: &str,
    parent_post_id: Option<i64>,
    author: &str,
    trigger_type: &str,
    force_mention: Option<MentionDto>,
) -> Result<(serde_json::Value, Option<serde_json::Value>), AppError> {
    // 解析 @token 为结构化提及（先专家后执行器）；纯文本无 @ 时为空，仅写人帖、不触发执行。
    let tokens = extract_at_tokens(content);
    let mut mentions = resolve_mentions(&tokens, &state.expert_manager).await;
    // 强制触发目标注入：assignee 已校验存在，确保即便其名含空格/标点也一定能命中触发。
    if let Some(m) = force_mention {
        if !mentions.iter().any(|x| x.kind == m.kind && x.name == m.name) {
            mentions.insert(0, m);
        }
    }
    let mentions_json = serialize_mentions(&mentions);

    // 人帖无条件落库（无论是否触发执行，用户的发言都要对讨论区可见）。
    let human = state
        .db
        .create_task_post(NewPost {
            task_id: task.id,
            parent_post_id,
            kind: KIND_HUMAN,
            author_name: author,
            executor: None,
            expert_name: None,
            content,
            mentions_json: &mentions_json,
            status: "sent",
            source_execution_id: None,
            source_todo_id: None,
        })
        .await
        .map_err(AppError::from)?;

    // 仅当 @ 到执行器或专家才触发执行；否则就是纯评论，不产生智能体帖。
    let has_trigger = first_executor(&mentions).is_some() || first_expert(&mentions).is_some();
    let agent_post = if has_trigger {
        Some(create_agent_post(state, task, &mentions, &mentions_json, content, trigger_type).await)
    } else {
        None
    };

    let human_val = serde_json::to_value(&human).unwrap_or(serde_json::Value::Null);
    Ok((human_val, agent_post))
}

/// POST .../tasks/{id}/posts — 创建人帖；含 @专家/@执行器 时同时触发执行并写占位帖。
pub async fn create_post(
    State(state): State<AppState>,
    Path((ws, task_id)): Path<(i64, i64)>,
    Json(req): Json<CreatePostRequest>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    // 取 task 并校验属于 path 的 workspace（防跨 ws 越权写讨论帖）；返回 task 复用。
    let task = require_task_in_ws(state.db.as_ref(), task_id, ws).await?;
    // trim 后判空，避免纯空白帖。
    let content = req.content.trim();
    if content.is_empty() {
        return Err(AppError::BadRequest("内容不能为空".to_string()));
    }
    // 楼中楼：校验 parent 属于本任务且为主楼层（parent_post_id IS NULL），保证深度≤1。
    if let Some(pid) = req.parent_post_id {
        validate_reply_parent(&state, task_id, pid).await?;
    }
    // 落人帖 +（若 @ 到执行器/专家）触发执行写占位帖；与委派首帖共用同一路径，零逻辑漂移。
    // 人工发帖统一作者「我」、trigger_type=discussion（completion 走 discussion 回写分支）。
    let (human_post, agent_post) = land_mention_post(
        &state,
        &task,
        content,
        req.parent_post_id,
        "我",
        TRIGGER_DISCUSSION,
        None,
    )
    .await?;

    Ok(ApiResponse::ok(serde_json::json!({
        "human_post": human_post,
        "agent_post": agent_post,
    })))
}

/// 补偿「执行在占位帖插入前就结束」的竞态：completion.rs 的 discussion 回写按
/// `source_execution_id` 找占位帖，占位帖还没插入时会错过（返回 0）。这里在插入占位帖
/// 之后复查 record 状态，若已非 running 则手动 finalize 回写，避免占位帖永久停在 running。
/// 签名只取 `&Database`（它只用 `db`）而非 `&AppState`：这样 completion 接力路径
/// （`spawn_relay_execution` 手里只有 `DelegateRelayHandles` 的 Arc 句柄、没有 AppState）
/// 也能复用同一补偿逻辑，避免 060 人工触发与 092 接力两处各写一遍竞态补偿。
async fn compensate_finished_execution(db: &Database, record_id: i64) {
    let Ok(Some(rec)) = db.get_execution_record(record_id).await else {
        return;
    };
    // 仍 running：正常等 completion 回调即可，无需补偿。
    if rec.status == ExecutionStatus::Running {
        return;
    }
    let success = matches!(rec.status, ExecutionStatus::Success);
    // result/executor 从执行记录取，与 completion.rs 正常回写路径一致。
    let result = rec.result.unwrap_or_default();
    if let Err(e) = db
        .finalize_discussion_post(record_id, success, &result, rec.executor.as_deref())
        .await
    {
        tracing::warn!(error = %e, record_id, "compensate finalize discussion post failed");
    }
}

/// 触发执行并写智能体占位帖；触发失败时写一条 failed 帖（不阻塞人帖已落库）。
/// `post_content` 是触发它的那条人帖正文——执行器必须看到真实诉求才能作答。
/// 智能体帖的共享字段（全借用，零拥有）：author/executor/expert/mentions 来自 @ 解析、
/// task_id 来自任务。与每次落定的 content/status/关联记录分离，收口 create_agent_post 的
/// Ok/Err 两分支，避免重复手写整段 NewPost。字段都是 Copy/借用，无需 clone。
struct AgentPostSpec<'a> {
    task_id: i64,
    author: &'a str,
    executor: Option<&'a str>,
    expert_name: Option<&'a str>,
    mentions_json: &'a str,
}

impl<'a> AgentPostSpec<'a> {
    /// 按本次落定状态（running/failed）与关联执行记录/载体 todo 组装 NewPost。
    /// 'a: 'b 保证 spec 借用的字段活得比 content/status（arm 内临时变量）更久。
    fn into_post<'b>(
        self,
        content: &'b str,
        status: &'b str,
        source_execution_id: Option<i64>,
        source_todo_id: Option<i64>,
    ) -> NewPost<'b>
    where
        'a: 'b,
    {
        NewPost {
            task_id: self.task_id,
            parent_post_id: None,
            kind: KIND_AGENT,
            author_name: self.author,
            executor: self.executor,
            expert_name: self.expert_name,
            content,
            mentions_json: self.mentions_json,
            status,
            source_execution_id,
            source_todo_id,
        }
    }
}

async fn create_agent_post(
    state: &AppState,
    task: &tasks::Model,
    mentions: &[MentionDto],
    mentions_json: &str,
    post_content: &str,
    trigger_type: &str,
) -> serde_json::Value {
    let author = agent_author(mentions);
    let executor = first_executor(mentions);
    let expert = first_expert(mentions);
    // 共享字段提前打包：Ok/Err 两分支仅 content/status/关联 id 不同，spec 收口避免重复构造 NewPost。
    // match 两 arm 互斥（运行时只走一条），故可各自 move 同一份 spec。
    let spec = AgentPostSpec {
        task_id: task.id,
        author: &author,
        executor,
        expert_name: expert,
        mentions_json,
    };
    // 先拉最近若干条主楼层讨论注入 prompt，让被 @ 的 AI 了解讨论全貌；
    // 读库失败由 fetch_recent_main_posts 静默回退空，不阻断本次触发。
    let recent = fetch_recent_main_posts(state.db.as_ref(), task.id).await;
    let history_text = format_discussion_history(&recent);
    let prompt = build_carrier_prompt(task, post_content, &history_text);
    match trigger_discussion_execution(state, task, mentions, &prompt, trigger_type).await {
        Ok((todo_id, record_id)) => {
            // 占位帖：running，关联执行记录与载体 todo；executor/expert 来自 @ 解析。
            let placeholder = format!("{author} 正在干活…");
            let np = spec.into_post(&placeholder, STATUS_RUNNING, Some(record_id), Some(todo_id));
            let post_val = insert_agent_post(state, np).await;
            // 补偿极快完成的执行：trigger 与 insert 占位帖之间若执行已结束，completion.rs 的
            // discussion 回写会错过（此时占位帖刚插入）→ 占位帖永久 running。插入后复查 record，
            // 已结束则手动回写。
            compensate_finished_execution(state.db.as_ref(), record_id).await;
            post_val
        }
        Err(e) => {
            // 触发失败也留痕：写一条 failed 帖，让用户看到「没能启动」。
            let failed_content = format!("{author} 触发失败：{e:?}");
            let np = spec.into_post(&failed_content, STATUS_FAILED, None, None);
            insert_agent_post(state, np).await
        }
    }
}

/// 插入一条智能体帖并序列化为 JSON 响应值。
///
/// 接收调用方已构造好的 `NewPost`（kind=KIND_AGENT、parent_post_id=None）。
/// 用 struct 收口而非散开长参数列表——字段多，散开会触发 clippy::too_many_arguments
/// 且调用点可读性差（create_agent_post 的 Ok/Err 两分支已由 AgentPostSpec::into_post 统一组装）。
/// 插入失败返回 null（保持 agent_post: TaskPost|null 契约，前端 filter(Boolean) 丢弃）。
async fn insert_agent_post(state: &AppState, post: NewPost<'_>) -> serde_json::Value {
    match state.db.create_task_post(post).await {
        Ok(p) => serde_json::to_value(&p).unwrap_or(serde_json::Value::Null),
        Err(e) => {
            tracing::warn!(error = %e, "insert agent post failed");
            serde_json::Value::Null
        }
    }
}

/// 取 task 并校验属于指定 workspace；不属于返回 NotFound（不泄露存在性，防越权探测）。
///
/// task_posts 四个 handler 的 path 都带 `{ws}`，必须校验 task 归属该 ws，否则可用别 ws 的
/// task_id 越权读写讨论帖（review S6 安全反思）。
async fn require_task_in_ws(
    db: &crate::db::Database,
    task_id: i64,
    ws_id: i64,
) -> Result<tasks::Model, AppError> {
    let task = db
        .get_task(task_id)
        .await
        .map_err(AppError::from)?
        .ok_or(AppError::NotFound)?;
    if task.workspace_id != Some(ws_id) {
        // task 存在但不属于该 workspace：视为不存在，避免跨工作空间越权读写讨论帖。
        return Err(AppError::NotFound);
    }
    Ok(task)
}

/// 校验被回复楼层属于本任务且为主楼层（parent_post_id IS NULL），保证楼中楼深度≤1。
async fn validate_reply_parent(
    state: &AppState,
    task_id: i64,
    parent_id: i64,
) -> Result<(), AppError> {
    let parent = state
        .db
        .get_task_post(parent_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::BadRequest("回复的楼层不存在".to_string()))?;
    if parent.task_id != task_id {
        return Err(AppError::BadRequest("回复的楼层不属于本任务".to_string()));
    }
    // 只能回复主楼层；回复楼中楼会形成二层嵌套，拒绝。
    if parent.parent_post_id.is_some() {
        return Err(AppError::BadRequest("只能在主楼层下回复（不支持多层嵌套）".to_string()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 自动接力（需求 092 P2：管家调度中枢）
// ---------------------------------------------------------------------------
//
// 接力中枢规则（设计 §5.2）：assignee 管家专家是调度中枢。每次讨论类执行
// （trigger=discussion / discussion_auto）完成、回写结论帖之后，若任务为 delegate +
// auto_continue + 专家 assignee，按「本次执行者是否就是 assignee」分两个子分支：
//   - 是管家（@专家帖的 expert_name == assignee）→ 解析其结论里的 @：
//       含 @yyy → 触发 yyy 执行（接力下一跳）；不含 @ → 管家判定完成，写说明帖终止。
//   - 不是管家（被@者完成）→ 唤醒 assignee 管家决策下一步。
// 护栏：continue_rounds 每轮 +1，达「有效上限」(三级可配)强制停。循环由此有界，不会无限递归。

/// 自动接力轮数的「终极兜底」上限：仅当任务与工作空间均未配置 delegate_max_rounds 时生效
/// （三级解析见 [`resolve_delegate_max_rounds`]）。保留常量以确保任何配置缺失下循环仍必有界。
pub(crate) const MAX_DELEGATE_ROUNDS: i64 = 10;

/// 接力轮数上限的合法上界（防 runaway token 成本）：任务/工作空间配置值不得超过此值，
/// 越界由 [`validate_delegate_max_rounds`] 拒 400。下界恒为 1（至少允许 1 轮接力）。
pub(crate) const DELEGATE_MAX_ROUNDS_CAP: i64 = 50;

/// 校验用户传入的接力上限：`Some(n)` 需落在 `1..=DELEGATE_MAX_ROUNDS_CAP`；`None` 视为
/// 「清除覆盖/用默认」直接放行。create/PATCH 任务、工作空间设置三处复用，集中越界口径。
pub(crate) fn validate_delegate_max_rounds(max: Option<i64>) -> Result<(), AppError> {
    if let Some(n) = max {
        if !(1..=DELEGATE_MAX_ROUNDS_CAP).contains(&n) {
            return Err(AppError::BadRequest(format!(
                "接力轮数上限必须在 1..={DELEGATE_MAX_ROUNDS_CAP} 之间"
            )));
        }
    }
    Ok(())
}

/// 把单个「原始上限值」收敛为有效值：仅当落在合法闭区间 `1..=CAP` 才采纳，否则回退兜底常量。
///
/// 集中「过滤非法 + 回退兜底」口径，供工作空间级 effective 计算（见
/// `agent_bot::get_workspace_settings`）与三级解析的兜底层复用，避免两处各写一遍
/// `filter(>0).unwrap_or(常量)` 而漂移。同时过滤**上界 CAP**：直写库可能写入 >50 的脏值，
/// resolve 作为护栏源头必须把它当无效，否则一个越界脏值会让上限静默放大、护栏形同虚设。
pub(crate) fn workspace_effective_max(raw: Option<i64>) -> i64 {
    raw.filter(|&x| (1..=DELEGATE_MAX_ROUNDS_CAP).contains(&x))
        .unwrap_or(MAX_DELEGATE_ROUNDS)
}

/// 三级解析某委派任务的「接力轮数有效上限」：任务覆盖 → 工作空间默认 → 终极兜底常量。
///
/// 收口在一处，护栏决策与详情展示共用同一口径，避免各路径分散解析导致漂移。DB 读失败时
/// graceful-degrade 回退兜底（`.ok()` 吞错），不阻塞接力主流程（仿 pre_spawn prompt 注入降级）。
/// 任务级与工作空间级都用 `1..=CAP` 区间过滤：直写库的越界脏值（0 / 负 / >CAP）一律视为无效、
/// 回退下一级，绝不把会让护栏失效的值透传为 effective。
pub(crate) async fn resolve_delegate_max_rounds(db: &Database, task: &tasks::Model) -> i64 {
    // 一级：任务自身覆盖优先（越界脏值视为无效，落空进兜底层）。
    if let Some(m) = task
        .delegate_max_rounds
        .filter(|&x| (1..=DELEGATE_MAX_ROUNDS_CAP).contains(&x))
    {
        return m;
    }
    // 二级 + 三级：忽略任务覆盖，解析 工作空间默认 → 兜底常量。
    resolve_delegate_max_rounds_fallback(db, task.workspace_id).await
}

/// 三级解析的「兜底层」：忽略任务级覆盖，仅解析 工作空间默认 → 终极兜底常量。
///
/// 供任务详情徽标编辑器计算「清除任务覆盖后会回退到几」——即留空/恢复默认后的真实落点。
/// 注意不能复用 [`resolve_delegate_max_rounds`]：后者会先取任务覆盖，而这里要的正是
/// 「假设任务覆盖不存在」的值，否则编辑器会把「当前覆盖值」误当成「清除后的回退值」展示。
pub(crate) async fn resolve_delegate_max_rounds_fallback(db: &Database, ws_id: Option<i64>) -> i64 {
    // 工作空间默认：无行 / NULL / 越界 / DB 读失败 → workspace_effective_max 内部统一回退兜底常量。
    if let Some(ws_id) = ws_id {
        let raw = crate::db::workspace_setting::get_workspace_settings(db, ws_id)
            .await
            .ok()
            .flatten()
            .and_then(|s| s.delegate_max_rounds);
        return workspace_effective_max(raw);
    }
    // 任务无 workspace_id（委派任务理论不应如此，防御性兜底）：直接终极兜底常量。
    MAX_DELEGATE_ROUNDS
}

/// 接力纯决策结果：把「读库 + 计数」与「触发执行/写帖」解耦，纯决策可单测覆盖全部分支。
enum DelegateRelayAction {
    /// 已达轮数上限 → 写「达上限」说明帖，停止接力。
    HitLimit,
    /// 管家判定完成（结论不含 @）→ 写「已完成」说明帖，循环自然终止。
    Finished,
    /// 管家结论含 @ → 触发被@者执行（接力下一跳）。
    TriggerMention,
    /// 被@者完成 → 唤醒 assignee 管家决策。
    WakeAssignee,
}

/// 纯决策函数（无 IO）：据轮数 / 执行者身份 / 结论里的 @ 决定下一步动作。
///
/// 抽成纯函数是为了把护栏与分派逻辑从异步编排中剥离，单测可覆盖四条分支，无需真实
/// 执行器（与 060 触发逻辑同策略，不做端到端）。`mentions` 仅在 x_is_assignee 分支有意义
/// （被@者完成时调用方传空 Vec，直接走 WakeAssignee）。
fn plan_delegate_relay(
    rounds: i64,
    max: i64,
    x_is_assignee: bool,
    mentions: &[MentionDto],
) -> DelegateRelayAction {
    // 护栏用 >=：continue_rounds 递增到 max 当轮即熔断，最多 max 轮接力（需求 AC8「达到 10 强制停止」）。
    // 若用 >，rounds==max 不熔断、会放行第 max+1 跳，实际跑 max+1 轮（CodeRabbit #5）。
    if rounds >= max {
        return DelegateRelayAction::HitLimit;
    }
    if x_is_assignee {
        // 管家本轮执行：结论含 @某人 则触发其执行；不含 @ 视为管家判定完成。
        let has_target = mentions
            .iter()
            .any(|m| m.kind == "executor" || m.kind == "expert");
        if has_target {
            DelegateRelayAction::TriggerMention
        } else {
            DelegateRelayAction::Finished
        }
    } else {
        // 被@者完成：唤醒管家决定下一步（重试 / 换人 / 收尾）。
        DelegateRelayAction::WakeAssignee
    }
}

/// completion 自动接力所需的执行句柄。
///
/// completion 路径（SpawnContext）只有 Arc 组件、没有 AppState；用 struct 收口这 6 个句柄，
/// 避免 continue_delegated_task 出现 6+ 参数长列表（CLAUDE.md 函数长度 / 可读性）。
/// 全部以引用借用，触发执行时按需 .clone() 出 owned Arc。
pub(crate) struct DelegateRelayHandles<'a> {
    pub db: &'a Arc<Database>,
    pub executor_registry: &'a Arc<ExecutorRegistry>,
    pub tx: &'a broadcast::Sender<ExecEvent>,
    pub task_manager: &'a Arc<TaskManager>,
    pub config: &'a Arc<RwLock<Config>>,
    pub expert_manager: &'a Arc<ExpertIndexManager>,
}

/// 自动接力入口（completion 回写讨论帖后调用）：读占位帖 / 任务元信息 → 计数 → 决策 → 推进。
///
/// 失败容忍：任一读库 / 触发失败只 tracing::warn 后返回，不影响本次执行已落定的成功状态
/// （与 auto_review / 060 回写降级哲学一致，帖子由前端轮询兜底）。
pub(crate) async fn continue_delegated_task(
    handles: &DelegateRelayHandles<'_>,
    record_id: i64,
    result_str: &str,
) {
    // 1. 占位帖：task_id 定位委派任务，expert_name 判断本次执行者身份。
    let Some(post) = load_relay_post(handles.db, record_id).await else {
        return;
    };
    // 2. 任务元信息 + 前置校验（仅 delegate + auto_continue + 专家 assignee 才接力）。
    let Some(task) = load_delegate_task(handles.db, post.task_id).await else {
        return;
    };
    let Some(assignee) = task.assignee_name.clone() else {
        return;
    };
    // 3. 护栏计数 +1（顺序事件驱动，无并发）；判定本次执行者是否管家本人。
    let new_rounds = match handles.db.increment_continue_rounds(post.task_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, task_id = post.task_id, "increment continue_rounds failed");
            return;
        }
    };
    let x_is_assignee = post.expert_name.as_deref() == Some(assignee.as_str());
    // 4. 仅管家本人执行时才解析其结论里的 @ 作为下一跳；被@者完成时无需解析。
    let mentions = if x_is_assignee {
        resolve_mentions(&extract_at_tokens(result_str), handles.expert_manager).await
    } else {
        Vec::new()
    };
    // 5. 纯决策 → 推进。有效上限三级解析(任务→工作空间→兜底)算一次，决策与触顶文案共用。
    let effective_max = resolve_delegate_max_rounds(handles.db, &task).await;
    let action = plan_delegate_relay(new_rounds, effective_max, x_is_assignee, &mentions);
    execute_relay_action(handles, &task, &assignee, action, &mentions, result_str, effective_max)
        .await;
}

/// 取本次执行对应的占位帖（含 task_id / expert_name）；帖已删或查询失败返回 None。
async fn load_relay_post(db: &Database, record_id: i64) -> Option<task_posts::Model> {
    match db.get_task_post_by_source_execution(record_id).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, record_id, "load relay post failed");
            None
        }
    }
}

/// 取任务并校验是「delegate + auto_continue + 专家 assignee」的接力任务；否则 None。
/// 把前置条件收口在一处，让 continue_delegated_task 主干保持线性可读。
async fn load_delegate_task(db: &Database, task_id: i64) -> Option<tasks::Model> {
    let task = db.get_task(task_id).await.ok().flatten()?;
    // 仅委派 + 开启自动接力 + 专家 assignee 才接力（执行器 P1 已禁用 auto_continue）。
    let is_relay = task.execution_mode == "delegate"
        && task.auto_continue != 0
        && task.assignee_kind.as_deref() == Some("expert")
        && task.assignee_name.is_some();
    if is_relay { Some(task) } else { None }
}

/// 按纯决策结果推进：触发下一跳执行，或写终止说明帖。
async fn execute_relay_action(
    handles: &DelegateRelayHandles<'_>,
    task: &tasks::Model,
    assignee: &str,
    action: DelegateRelayAction,
    mentions: &[MentionDto],
    result_str: &str,
    // 有效上限(三级解析):触顶文案需展示真实阈值，而非写死常量，否则与实际熔断口径不符。
    effective_max: i64,
) {
    match action {
        DelegateRelayAction::HitLimit => {
            let msg = format!(
                "⚠️ 自动接力已达 {} 轮上限，停止调度。如需继续请在讨论区手动 @。",
                effective_max
            );
            insert_relay_note_post(handles, task, assignee, &msg).await;
        }
        DelegateRelayAction::Finished => {
            insert_relay_note_post(handles, task, assignee, "✅ 任务已完成，管家停止调度。").await;
        }
        DelegateRelayAction::TriggerMention => {
            // 管家结论含 @ → 触发被@者执行，喂入管家结论作为诉求上下文。
            spawn_relay_execution(handles, task, mentions, result_str, TRIGGER_DISCUSSION_AUTO).await;
        }
        DelegateRelayAction::WakeAssignee => {
            // 被@者完成 → 唤醒管家决策（@assignee，message=决策指令）。
            let wake = wake_mention(assignee);
            spawn_relay_execution(
                handles,
                task,
                std::slice::from_ref(&wake),
                &build_decision_directive(),
                TRIGGER_DISCUSSION_AUTO,
            )
            .await;
        }
    }
}

/// 建载体 todo + 触发执行 + 写 running 占位帖（与 create_agent_post 同构，但走 completion 的
/// Arc 句柄而非 AppState；trigger_type 由调用方传 discussion_auto）。
///
/// 豁免说明（>50 行）：线性管道——取执行器/专家 → 解析 ws_path → 拼 prompt/mentions →
/// 建 carrier todo → 触发执行 → 取 record_id → 写占位帖；其中 RunTodoExecutionRequest 构造
/// 是连续纯数据赋值（无分支），符合规范豁免场景 #1（纯数据构建）+ #2（线性管道），与
/// trigger_discussion_execution 同一豁免理由。失败写 failed 帖 + 软删 carrier todo。
async fn spawn_relay_execution(
    handles: &DelegateRelayHandles<'_>,
    task: &tasks::Model,
    mentions: &[MentionDto],
    directive: &str,
    trigger_type: &str,
) {
    let executor_name = first_executor(mentions);
    let expert_name = first_expert(mentions);
    let author = agent_author(mentions);
    let ws_id = task.workspace_id.unwrap_or(1);
    let ws_path = resolve_ws_path(handles.db, ws_id).await;
    // 复用 060 的 prompt 构建 + 历史注入，让接力执行与人工 @ 触发看到同等上下文。
    let recent = fetch_recent_main_posts(handles.db, task.id).await;
    let history = format_discussion_history(&recent);
    let prompt = build_carrier_prompt(task, directive, &history);
    let mentions_json = serialize_mentions(mentions);
    let title = format!("接力触发 @{author}");
    let todo_id = match handles
        .db
        .create_discussion_todo(title, prompt.clone(), executor_name, expert_name, ws_id, &ws_path)
        .await
    {
        Ok(id) => id,
        Err(e) => {
            tracing::warn!(error = %e, task_id = task.id, "relay create carrier todo failed");
            return;
        }
    };
    let request = RunTodoExecutionRequest {
        db: handles.db.clone(),
        executor_registry: handles.executor_registry.clone(),
        tx: handles.tx.clone(),
        task_manager: handles.task_manager.clone(),
        config: handles.config.clone(),
        todo_id,
        message: prompt,
        req_executor: executor_name.map(str::to_string),
        req_model: None,
        trigger_type: trigger_type.to_string(),
        params: None,
        resume_session_id: None,
        resume_message: None,
        source_todo_id: Some(todo_id),
        source_todo_title: Some(format!("接力帖-任务{}", task.id)),
        loop_step_execution_id: None,
        step_id: None,
        feishu_bot_id: None,
        feishu_receive_id: None,
        feishu_receive_id_type: None,
        workspace_path: if ws_path.is_empty() { None } else { Some(ws_path) },
        workspace_id: Some(ws_id),
        expert_manager: Some(handles.expert_manager.clone()),
    };
    // 共享字段打包：成功/失败两分支仅 content/status/关联 id 不同，spec 收口避免重复构造。
    let spec = AgentPostSpec {
        task_id: task.id,
        author: &author,
        executor: executor_name,
        expert_name,
        mentions_json: &mentions_json,
    };
    // 关键：必须用类型擦除的 `run_todo_execution_boxed`（见其文档注释）而非直接 await
    // run_todo_execution。否则「接力 → run_todo_execution → finalize → 接力」coroutine 类型环
    // 会让编译器报 cycle detected / 无法证明 Send。boxed 包装把具体 future 藏在函数体内，
    // 调用方只拿到 `Pin<Box<dyn Future + Send>>` 不透明类型，环在此处终止。
    // run_todo_execution 内部已 spawn 执行器、很快返回 record_id，await 的是「建好执行任务」。
    let exec = run_todo_execution_boxed(request).await;
    match exec.record_id {
        Some(record_id) => {
            // 占位帖：running，关联执行记录与载体 todo。
            let placeholder = format!("{author} 正在干活…");
            let np = spec.into_post(&placeholder, STATUS_RUNNING, Some(record_id), Some(todo_id));
            if let Err(e) = handles.db.create_task_post(np).await {
                tracing::warn!(error = %e, task_id = task.id, "relay insert placeholder failed");
            }
            // 竞态补偿：run_todo_execution 是 fire-and-forget（spawn 后立刻返回 record_id），
            // 与 060 的 create_agent_post 同构——若执行在「返回 record_id」与「占位帖插入」之间
            // 已结束，completion 的 discussion 回写会错过此刻还不存在的占位帖 → 永久 running。
            // 插入后复查 record 状态，已结束则手动回写。（此竞态下接力轮被跳过，遵循失败容忍
            // 哲学：用户可在讨论区手动 @ 推进，不阻断已落定的执行成功。）
            compensate_finished_execution(handles.db.as_ref(), record_id).await;
        }
        None => {
            // 启动失败：软删 carrier todo + 写 failed 帖留痕（不阻塞接力链路已落定的部分）。
            let _ = handles.db.soft_delete_todo(todo_id).await;
            let failed = format!("{author} 接力触发失败");
            let np = spec.into_post(&failed, STATUS_FAILED, None, None);
            if let Err(e) = handles.db.create_task_post(np).await {
                tracing::warn!(error = %e, task_id = task.id, "relay insert failed-post failed");
            }
        }
    }
}

/// 写一条接力终止说明帖（达上限 / 已完成）：以 assignee 管家身份发，无执行关联、mentions 空。
async fn insert_relay_note_post(
    handles: &DelegateRelayHandles<'_>,
    task: &tasks::Model,
    assignee: &str,
    message: &str,
) {
    let np = NewPost {
        task_id: task.id,
        parent_post_id: None,
        kind: KIND_AGENT,
        author_name: assignee,
        executor: None,
        expert_name: Some(assignee),
        content: message,
        mentions_json: "[]",
        status: STATUS_SUCCESS,
        source_execution_id: None,
        source_todo_id: None,
    };
    if let Err(e) = handles.db.create_task_post(np).await {
        tracing::warn!(error = %e, task_id = task.id, "relay insert note post failed");
    }
}

/// 构造「唤醒管家」用的专家提及（单条，name=assignee）。
fn wake_mention(name: &str) -> MentionDto {
    MentionDto {
        kind: "expert".to_string(),
        name: name.to_string(),
        display: name.to_string(),
    }
}

/// 唤醒管家时的决策指令（作为 carrier prompt 的「诉求」段）。
/// 管家 persona 由 inject_expert_context 按 expert_name=assignee 注入，这里只给协调者角色的行为约束。
fn build_decision_directive() -> String {
    "（系统自动唤醒：你是本任务的任务管家 / 协调者。）\n\
     请基于上述任务需求与最近的讨论、执行结论，决定下一步：\n\
     - 若还有事要做，请 @ 合适的专家或执行器去执行（回复中含 @某人 即自动触发其执行）；\n\
     - 若任务已全部完成，请直接给出最终结论，不要 @ 任何人（不含 @ 则任务结束）。"
        .to_string()
}

// ---------------------------------------------------------------------------
// 单元测试（纯函数部分）
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::expert::{ExpertMetadata, ExpertSource, ExpertType};

    /// plan_delegate_relay：轮数达上限（rounds == max）即熔断 → HitLimit（护栏优先级最高）。
    #[test]
    fn test_plan_delegate_relay_hit_limit() {
        // 刚好达上限：max == max 应立即熔断（>= 语义；若实现成 > 会漏过这一跳跑到 max+1 轮）。
        assert!(matches!(
            plan_delegate_relay(10, 10, true, &[]),
            DelegateRelayAction::HitLimit
        ));
        // 超上限同样熔断。
        assert!(matches!(
            plan_delegate_relay(11, 10, false, &[]),
            DelegateRelayAction::HitLimit
        ));
    }

    /// 管家本人执行 + 结论含 @执行器/专家 → TriggerMention（接力下一跳）。
    #[test]
    fn test_plan_delegate_relay_trigger_mention() {
        let mentions = vec![MentionDto {
            kind: "executor".to_string(),
            name: "codex".into(),
            display: "Codex".into(),
        }];
        let action = plan_delegate_relay(1, 10, true, &mentions);
        assert!(matches!(action, DelegateRelayAction::TriggerMention));
    }

    /// 管家本人执行 + 结论不含 @ → Finished（循环自然终止）。
    #[test]
    fn test_plan_delegate_relay_finished_when_no_mention() {
        let action = plan_delegate_relay(1, 10, true, &[]);
        assert!(matches!(action, DelegateRelayAction::Finished));
    }

    /// 被@者完成（executor≠assignee）→ WakeAssignee（唤醒管家决策）。
    #[test]
    fn test_plan_delegate_relay_wake_assignee() {
        let action = plan_delegate_relay(1, 10, false, &[]);
        assert!(matches!(action, DelegateRelayAction::WakeAssignee));
    }

    /// 边界：rounds == max-1（未达上限）仍允许推进；rounds == max 才熔断。
    /// 体现「最后一跳在 max-1 触发、达 max 当轮即停」的护栏语义（>= 口径，最多 max 轮）。
    #[test]
    fn test_plan_delegate_relay_boundary_at_max() {
        let with_mention = vec![MentionDto {
            kind: "expert".to_string(),
            name: "e".into(),
            display: "e".into(),
        }];
        // rounds == max-1 未达上限：管家含 @ → TriggerMention；无 @ → Finished。
        assert!(matches!(
            plan_delegate_relay(9, 10, true, &with_mention),
            DelegateRelayAction::TriggerMention
        ));
        assert!(matches!(
            plan_delegate_relay(9, 10, true, &[]),
            DelegateRelayAction::Finished
        ));
        // rounds == max-1 但非管家 → WakeAssignee（护栏只管轮数，不管身份分支）。
        assert!(matches!(
            plan_delegate_relay(9, 10, false, &[]),
            DelegateRelayAction::WakeAssignee
        ));
    }

    /// 基本 @token 抽取：英文、CJK、含下划线的名字都能切出。
    #[test]
    fn test_extract_at_tokens_basic() {
        let tokens = extract_at_tokens("@codex 帮我看看，再 @前端架构师 评审");
        assert_eq!(tokens, vec!["codex".to_string(), "前端架构师".to_string()]);
    }

    /// @ 后紧跟标点（无有效字符）不产出空 token。
    #[test]
    fn test_extract_at_tokens_leading_punct_ignored() {
        let tokens = extract_at_tokens("邮箱 test@example.com 里的 @ 不是提及");
        // @example 会被当 token（含点号），但语义上无害——这里只断言不含空串。
        assert!(!tokens.iter().any(|t| t.is_empty()));
        assert!(tokens.iter().any(|t| t == "example.com"));
    }

    /// 多 @ 连写（无空格分隔）也能逐个切出。
    #[test]
    fn test_extract_at_tokens_consecutive() {
        let tokens = extract_at_tokens("@codex@claude_code");
        assert_eq!(tokens, vec!["codex".to_string(), "claude_code".to_string()]);
    }

    /// is_mention_char 边界：空格/中文标点结束，字母数字下划线连字符点继续。
    #[test]
    fn test_is_mention_char() {
        assert!(is_mention_char('a'));
        assert!(is_mention_char('前'));
        assert!(is_mention_char('_'));
        assert!(is_mention_char('-'));
        assert!(is_mention_char('.'));
        assert!(!is_mention_char(' '));
        assert!(!is_mention_char('，'));
        assert!(!is_mention_char('@'));
    }

    /// serialize_mentions 输出合法 JSON，type 字段被 rename。
    #[test]
    fn test_serialize_mentions() {
        let ms = vec![MentionDto {
            kind: "executor".to_string(),
            name: "claudecode".to_string(),
            display: "Claude Code".to_string(),
        }];
        let s = serialize_mentions(&ms);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v[0]["type"], "executor");
        assert_eq!(v[0]["name"], "claudecode");
    }

    /// 构造一条 task_posts::Model（测试用，避免每次手写全部字段）。
    fn post_model(id: i64, parent: Option<i64>) -> task_posts::Model {
        task_posts::Model {
            id,
            task_id: 1,
            parent_post_id: parent,
            kind: "human".to_string(),
            author_name: "x".to_string(),
            executor: None,
            expert_name: None,
            content: "c".to_string(),
            mentions: "[]".to_string(),
            status: "sent".to_string(),
            source_execution_id: None,
            source_todo_id: None,
            created_at: None,
            updated_at: None,
        }
    }

    /// build_post_tree：楼中楼按 parent 归属主楼层；孤儿回复（parent 不在主楼层）被丢弃。
    #[test]
    fn test_build_post_tree_attaches_replies_and_drops_orphans() {
        let m1 = post_model(1, None);
        let m2 = post_model(2, None);
        let r1 = post_model(10, Some(1)); // 归属 m1
        let orphan = post_model(11, Some(99)); // parent 不在主楼层 → 丢弃
        let tree = build_post_tree(&[m1, m2], vec![r1, orphan]);
        assert_eq!(tree.len(), 2, "主楼层数不变");
        let m1v = tree.iter().find(|v| v["id"].as_i64() == Some(1)).expect("m1");
        assert_eq!(m1v["replies"].as_array().expect("replies array").len(), 1);
        assert_eq!(m1v["replies"][0]["id"].as_i64(), Some(10));
        let m2v = tree.iter().find(|v| v["id"].as_i64() == Some(2)).expect("m2");
        assert!(m2v["replies"].as_array().expect("replies empty").is_empty());
    }

    /// 构造最小可用 ExpertMetadata（仅 name/display_name_zh 影响分类，其余给空值）。
    fn test_expert(name: &str) -> ExpertMetadata {
        ExpertMetadata {
            name: name.to_string(),
            expert_type: ExpertType::Agent,
            version: "1".to_string(),
            source: ExpertSource::System,
            display_name_zh: Some(name.to_string()),
            display_name_en: None,
            profession_zh: None,
            profession_en: None,
            description_zh: None,
            description_en: None,
            avatar_path: None,
            category_id: None,
            definition_dir: String::new(),
            plugin_json_path: String::new(),
            agent_name: None,
            lead_agent: None,
            member_agents: Vec::new(),
            members: Vec::new(),
            skills: Vec::new(),
            default_init_prompt_zh: None,
            default_init_prompt_en: None,
            tags: Vec::new(),
            loaded_at: String::new(),
            is_active: true,
        }
    }

    /// 同名（claudecode 既是内置执行器又注入了同名专家）时专家优先（需求§10）。
    #[test]
    fn test_classify_mention_prefers_expert_on_name_clash() {
        let expert = test_expert("claudecode");
        let m = classify_mention("claudecode", Some(&expert)).expect("应命中");
        assert_eq!(m.kind, "expert", "同名时专家优先（需求§10）");
        assert_eq!(m.name, "claudecode");
    }

    /// 无专家匹配时落到内置执行器（claudecode）。
    #[test]
    fn test_classify_mention_falls_back_to_executor() {
        let m = classify_mention("claudecode", None).expect("应命中内置执行器");
        assert_eq!(m.kind, "executor");
    }

    /// 既非专家也非执行器 → None（当普通文本，不触发执行）。
    #[test]
    fn test_classify_mention_unknown_returns_none() {
        assert!(classify_mention("查无此名_xyz_9", None).is_none());
    }

    /// 构造一条可自定义正文/作者/身份/状态的 task_posts::Model（历史格式化测试用）。
    fn post_with(id: i64, author: &str, kind: &str, status: &str, content: &str) -> task_posts::Model {
        task_posts::Model {
            id,
            task_id: 1,
            parent_post_id: None,
            kind: kind.to_string(),
            author_name: author.to_string(),
            executor: None,
            expert_name: None,
            content: content.to_string(),
            mentions: "[]".to_string(),
            status: status.to_string(),
            source_execution_id: None,
            source_todo_id: None,
            created_at: None,
            updated_at: None,
        }
    }

    /// 构造最小 tasks::Model（carrier prompt 测试用，只关心 id/title/description/workspace_id）。
    fn task_model(id: i64, ws: Option<i64>) -> tasks::Model {
        tasks::Model {
            id,
            title: format!("任务#{id}"),
            description: format!("任务#{id}的需求描述"),
            status: "pending".to_string(),
            workspace_id: ws,
            template_id: None,
            loop_id: None,
            created_by: "tester".to_string(),
            created_at: None,
            updated_at: None,
            // 需求 092 新增字段：测试 helper 给环路默认值/空，不影响讨论触发逻辑。
            execution_mode: "loop".to_string(),
            assignee_kind: None,
            assignee_name: None,
            auto_continue: 0,
            continue_rounds: 0,
            // 接力上限覆盖：helper 默认未覆盖（None → 三级解析回退兜底），不影响讨论触发测试。
            delegate_max_rounds: None,
        }
    }

    /// format_discussion_history：多条帖子逐行拼接，含作者/身份/状态。
    #[test]
    fn test_format_discussion_history_multiple() {
        let posts = vec![
            post_with(1, "alice", "human", "sent", "第一条"),
            post_with(2, "codex", "agent", "success", "第二条结论"),
        ];
        let s = format_discussion_history(&posts);
        assert!(s.contains("[alice(human) sent] 第一条"));
        assert!(s.contains("[codex(agent) success] 第二条结论"));
    }

    /// format_discussion_history：空列表返回空串。
    #[test]
    fn test_format_discussion_history_empty() {
        assert_eq!(format_discussion_history(&[]), "");
    }

    /// format_discussion_history：超长正文按字符截断并加省略号，避免 prompt 膨胀。
    #[test]
    fn test_format_discussion_history_truncates_long_post() {
        let long = "a".repeat(HISTORY_POST_TRUNCATE + 10);
        let posts = vec![post_with(1, "x", "human", "sent", &long)];
        let s = format_discussion_history(&posts);
        // 截断后末尾带省略号，且不残留被截掉的部分。
        assert!(s.ends_with('…'));
        assert!(!s.contains(&"a".repeat(HISTORY_POST_TRUNCATE + 1)));
    }

    /// truncate_chars：未超长原样返回（按 char 计数，中文算 1 个）。
    #[test]
    fn test_truncate_chars_under_limit() {
        assert_eq!(truncate_chars("中文测试", 10), "中文测试");
    }

    /// truncate_chars：超长按 char 截断加省略号。
    #[test]
    fn test_truncate_chars_over_limit() {
        assert_eq!(truncate_chars("一二三四五", 3), "一二三…");
    }

    /// build_carrier_prompt：含任务 id / workspace id / ntd 命令，且命令占位已被实际值替换。
    #[test]
    fn test_build_carrier_prompt_injects_id_ws_and_cmds() {
        let task = task_model(42, Some(7));
        let p = build_carrier_prompt(&task, "帮我分析", "[历史]");
        // 任务与工作空间标识均已注入。
        assert!(p.contains("任务 #42"));
        assert!(p.contains("工作空间 #7"));
        // ntd 命令存在，且 ws/task 占位已替换为实际值（不再含裸 {ws}/{id}）。
        assert!(p.contains("ntd task view --workspace-id 7 --task 42"));
        assert!(p.contains("ntd task posts --workspace-id 7 --task 42 list"));
        assert!(!p.contains("{ws}"));
        assert!(!p.contains("{id}"));
    }

    /// build_carrier_prompt：空历史 → 历史段显示「（暂无历史）」，但仍带任务 id 与命令。
    #[test]
    fn test_build_carrier_prompt_empty_history() {
        let task = task_model(1, Some(1));
        let p = build_carrier_prompt(&task, "诉求", "");
        assert!(p.contains("（暂无历史）"));
        assert!(p.contains("任务 #1"));
        assert!(p.contains("ntd task posts --workspace-id 1 --task 1 list"));
    }

    /// first_executor：返回首个 kind=executor 的 name；不混入专家。
    #[test]
    fn test_first_executor_returns_first_executor_name() {
        let ms = vec![
            MentionDto { kind: "expert".to_string(), name: "架构师".to_string(), display: "架构师".to_string() },
            MentionDto { kind: "executor".to_string(), name: "codex".to_string(), display: "Codex".to_string() },
            MentionDto { kind: "executor".to_string(), name: "claudecode".to_string(), display: "Claude Code".to_string() },
        ];
        // 多个执行器只取首个（决定由谁承载），且不被前置专家干扰。
        assert_eq!(first_executor(&ms), Some("codex"));
    }

    /// first_executor：空列表或仅专家时返回 None。
    #[test]
    fn test_first_executor_none_when_no_executor() {
        assert!(first_executor(&[]).is_none());
        let only_expert = vec![MentionDto {
            kind: "expert".to_string(),
            name: "e".to_string(),
            display: "e".to_string(),
        }];
        assert!(first_executor(&only_expert).is_none());
    }

    /// first_expert：返回首个 kind=expert 的 name；无专家返回 None。
    #[test]
    fn test_first_expert_returns_first_expert_name() {
        let ms = vec![
            MentionDto { kind: "executor".to_string(), name: "codex".to_string(), display: "Codex".to_string() },
            MentionDto { kind: "expert".to_string(), name: "架构师".to_string(), display: "架构师".to_string() },
        ];
        assert_eq!(first_expert(&ms), Some("架构师"));
        assert!(first_expert(&[]).is_none());
    }

    /// agent_author：优先专家、其次执行器、都没有时兜底「智能体」。
    /// 体现展示顺序——@专家 时徽标显示专家名而非底层承载执行器。
    #[test]
    fn test_agent_author_priority_expert_then_executor_then_default() {
        let both = vec![
            MentionDto { kind: "executor".to_string(), name: "codex".to_string(), display: "Codex".to_string() },
            MentionDto { kind: "expert".to_string(), name: "架构师".to_string(), display: "架构师".to_string() },
        ];
        assert_eq!(agent_author(&both), "架构师");
        let only_exec = vec![MentionDto {
            kind: "executor".to_string(),
            name: "codex".to_string(),
            display: "Codex".to_string(),
        }];
        assert_eq!(agent_author(&only_exec), "codex");
        assert_eq!(agent_author(&[]), "智能体");
    }

    /// attach_replies：有 replies 时挂到主楼层 replies 字段，数组原样保留。
    #[test]
    fn test_attach_replies_with_list() {
        let main = post_model(1, None);
        let replies = vec![serde_json::json!({ "id": 10, "content": "回复1" })];
        let v = attach_replies(&main, Some(replies));
        let arr = v["replies"].as_array().expect("replies array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"].as_i64(), Some(10));
    }

    /// attach_replies：传 None 时 replies 为空数组，保证前端 replies 字段恒存在、无需判空。
    #[test]
    fn test_attach_replies_none_yields_empty_array() {
        let main = post_model(2, None);
        let v = attach_replies(&main, None);
        assert!(v["replies"].as_array().expect("replies array").is_empty());
        // 主楼层自身字段未丢（id 仍在）。
        assert_eq!(v["id"].as_i64(), Some(2));
    }

    // —— 需求 092：接力上限配置化（范围校验 + 三级解析）——

    /// validate_delegate_max_rounds：None 与 1..=50 放行，越界（0/-1/51）拒 400。
    #[test]
    fn test_validate_delegate_max_rounds_bounds() {
        // None = 沿用默认，永远放行（create/PATCH/PUT 三处一致）。
        assert!(validate_delegate_max_rounds(None).is_ok());
        // 闭区间边界 1 与 50 合法。
        assert!(validate_delegate_max_rounds(Some(1)).is_ok());
        assert!(validate_delegate_max_rounds(Some(50)).is_ok());
        // 下界外：0（非合法轮数）/ -1 拒。
        assert!(validate_delegate_max_rounds(Some(0)).is_err());
        assert!(validate_delegate_max_rounds(Some(-1)).is_err());
        // 上界外：51 拒（防 runaway token 成本，CAP=50）。
        assert!(validate_delegate_max_rounds(Some(51)).is_err());
    }

    /// resolve_delegate_max_rounds：任务覆盖 > 工作空间默认 > 兜底常量 三级。
    /// 用内存库真实写入两级配置，验证解析优先级与回退链路（非 mock）。
    #[tokio::test]
    async fn test_resolve_delegate_max_rounds_three_levels() {
        let db = Database::new(":memory:").await.expect("memory db");
        // 先置工作空间默认(15)，再建一个带覆盖(7)的任务：覆盖必须优先。
        crate::db::workspace_setting::update_workspace_delegate_max_rounds(&db, 1, Some(15))
            .await
            .expect("set ws default");
        let t1 = db
            .create_delegate_task("T1", "D", 1, "expert", "专家A", true, Some(7))
            .await
            .expect("create t1");
        assert_eq!(
            resolve_delegate_max_rounds(&db, &t1).await,
            7,
            "任务覆盖(7) 优先于工作空间默认(15)"
        );
        // 二级：任务未覆盖(None) → 取工作空间默认。
        let t2 = db
            .create_delegate_task("T2", "D", 1, "expert", "专家A", true, None)
            .await
            .expect("create t2");
        assert_eq!(
            resolve_delegate_max_rounds(&db, &t2).await,
            15,
            "任务未覆盖时取工作空间默认(15)"
        );
        // 三级：工作空间(ws 2)也无配置行 → 兜底常量 10。
        let t3 = db
            .create_delegate_task("T3", "D", 2, "expert", "专家A", true, None)
            .await
            .expect("create t3");
        assert_eq!(
            resolve_delegate_max_rounds(&db, &t3).await,
            MAX_DELEGATE_ROUNDS,
            "两级均缺省 → 终极兜底常量 10"
        );
    }

    /// resolve：任务覆盖为非法 0 时应跳过（回退工作空间默认），验证 .filter(|x|>0) 防御脏数据。
    /// handler 本会拦 0，但 resolve 仍需对「直写库的脏值」鲁棒，不返回 0 这种会让护栏失效的值。
    #[tokio::test]
    async fn test_resolve_delegate_max_rounds_skips_invalid_override() {
        let db = Database::new(":memory:").await.expect("memory db");
        let t = db
            .create_delegate_task("T", "D", 1, "expert", "专家A", true, Some(0))
            .await
            .expect("create");
        crate::db::workspace_setting::update_workspace_delegate_max_rounds(&db, 1, Some(20))
            .await
            .expect("set ws");
        assert_eq!(
            resolve_delegate_max_rounds(&db, &t).await,
            20,
            "覆盖值 0 非法 → 回退工作空间默认(20)，绝不返回 0"
        );
    }

    /// workspace_effective_max：纯函数边界——None/0/负/>CAP 一律回退兜底常量；1..=50 原样返回。
    /// 重点覆盖「直写库脏数据」防护：>50 的值不能作为 effective 透传（会让护栏静默放大）。
    #[test]
    fn test_workspace_effective_max_bounds() {
        // 合法闭区间原样返回。
        assert_eq!(workspace_effective_max(Some(1)), 1);
        assert_eq!(workspace_effective_max(Some(50)), 50);
        // 未配置 / 非法下界 → 兜底常量。
        assert_eq!(workspace_effective_max(None), MAX_DELEGATE_ROUNDS);
        assert_eq!(workspace_effective_max(Some(0)), MAX_DELEGATE_ROUNDS);
        assert_eq!(workspace_effective_max(Some(-5)), MAX_DELEGATE_ROUNDS);
        // 越界上界（脏数据）→ 兜底常量，绝不透传 >CAP 值放大护栏。
        assert_eq!(workspace_effective_max(Some(51)), MAX_DELEGATE_ROUNDS);
        assert_eq!(workspace_effective_max(Some(100)), MAX_DELEGATE_ROUNDS);
    }

    /// resolve：任务覆盖为越界上界（>CAP，脏数据）时应跳过，回退工作空间默认，
    /// 而非把 100 透传成 effective（会让护栏形同虚设）。handler 本会拦 51，但 resolve 须对直写库鲁棒。
    #[tokio::test]
    async fn test_resolve_delegate_max_rounds_skips_over_cap_override() {
        let db = Database::new(":memory:").await.expect("memory db");
        // 工作空间默认 12；任务覆盖直写 100（绕开 handler 校验，模拟脏数据）。
        crate::db::workspace_setting::update_workspace_delegate_max_rounds(&db, 1, Some(12))
            .await
            .expect("set ws");
        let t = db
            .create_delegate_task("T", "D", 1, "expert", "专家A", true, Some(100))
            .await
            .expect("create");
        assert_eq!(
            resolve_delegate_max_rounds(&db, &t).await,
            12,
            "覆盖值 100 越界 → 回退工作空间默认(12)，绝不返回 100"
        );
    }

    /// resolve_delegate_max_rounds_fallback：忽略任务覆盖，只取 工作空间默认 → 兜底。
    /// 徽标编辑器据此展示「清除覆盖后的真实回退值」，须与 effective（含任务覆盖）明确区分。
    #[tokio::test]
    async fn test_resolve_fallback_ignores_task_override() {
        let db = Database::new(":memory:").await.expect("memory db");
        crate::db::workspace_setting::update_workspace_delegate_max_rounds(&db, 1, Some(15))
            .await
            .expect("set ws");
        // 任务有覆盖(7)，但 fallback 必须忽略它，返回工作空间默认(15)。
        let t = db
            .create_delegate_task("T", "D", 1, "expert", "专家A", true, Some(7))
            .await
            .expect("create");
        assert_eq!(
            resolve_delegate_max_rounds_fallback(&db, t.workspace_id).await,
            15,
            "fallback 忽略任务覆盖(7)，回退工作空间默认(15)"
        );
        // 工作空间也无配置(ws 2) → 兜底常量。
        assert_eq!(
            resolve_delegate_max_rounds_fallback(&db, Some(2)).await,
            MAX_DELEGATE_ROUNDS,
            "工作空间未配置 → 兜底常量 10"
        );
    }
}
