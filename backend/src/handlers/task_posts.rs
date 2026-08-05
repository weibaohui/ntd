//! 任务讨论区 API（需求 060：论坛跟帖 + @专家/@执行器 触发执行后回帖）。
//!
//! 设计要点：
//! - 人帖（kind=human）：纯 Markdown 评论，直接入库。
//! - 智能体帖（kind=agent）：人帖里含 @专家/@执行器 时触发。流程为
//!   「建隐藏载体 Todo（todo_type=4）→ start_todo_execution（spawn 后立即返回 record_id）
//!   → 写 running 占位帖」；执行完成时由 completion.rs 的 discussion 分支回写结论。
//! - 执行系统是 Todo 中心的，必须借载体 Todo 承载 executor/prompt/expert_name。

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::adapters::find_executor;
use crate::db::entity::tasks;
use crate::db::task_post::{
    NewPost, KIND_AGENT, KIND_HUMAN, STATUS_FAILED, STATUS_RUNNING,
};
use crate::executor_service::RunTodoExecutionRequest;
use crate::handlers::execution::start_todo_execution;
use crate::handlers::{AppError, AppState};
use crate::models::ApiResponse;

/// 讨论触发用的 trigger_type：completion.rs 据此回写智能体占位帖。
const TRIGGER_DISCUSSION: &str = "discussion";

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

/// 解析 @token 为结构化提及：先匹配执行器，再匹配专家；都不中则忽略（当普通文本）。
///
/// 同名歧义（专家名与执行器名撞）按「先执行器后专家」消歧；
/// `@选择器` 在前端若显式带类型，会插入带空格的规范名，此处仍能命中。
async fn resolve_mentions(tokens: &[String], state: &AppState) -> Vec<MentionDto> {
    let mut out: Vec<MentionDto> = Vec::new();
    for tok in tokens {
        // 执行器名/别名大小写不敏感（find_executor 内部 trim + lowercase）。
        if let Some(def) = find_executor(tok) {
            out.push(MentionDto {
                kind: "executor".to_string(),
                name: def.name.to_string(),
                display: def.display_name.to_string(),
            });
            continue;
        }
        if let Some(meta) = state.expert_manager.get_expert_by_name(tok) {
            out.push(MentionDto {
                kind: "expert".to_string(),
                name: meta.name.clone(),
                display: meta.display_name_zh.unwrap_or_else(|| meta.name.clone()),
            });
        }
    }
    out
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

/// 拼装发给执行器的 message：任务上下文 + 帖子诉求。
/// 专家人设由 inject_expert_context 按 todo.expert_name 自动前置，这里不重复。
fn build_carrier_prompt(task: &tasks::Model, post_content: &str) -> String {
    format!(
        "你被 @ 到任务讨论区。请针对下面的讨论诉求给出可直接用于回复的结论（Markdown）。\n\n\
         任务标题：{title}\n任务需求：\n{desc}\n\n讨论诉求：\n{content}",
        title = task.title,
        desc = task.description,
        content = post_content,
    )
}

/// 建 carrier Todo 并触发执行，返回 (carrier_todo_id, record_id)。
///
/// start_todo_execution 内部 tokio::spawn 后立即返回 record_id，因此本函数不阻塞，
/// HTTP 请求可即刻返回占位帖。执行落定时由 completion.rs discussion 分支回写。
async fn trigger_discussion_execution(
    state: &AppState,
    task: &tasks::Model,
    mentions: &[MentionDto],
    carrier_prompt: &str,
) -> Result<(i64, i64), AppError> {
    let executor_name = first_executor(mentions);
    let expert_name = first_expert(mentions);
    let ws_id = task.workspace_id.unwrap_or(1);
    // 任务表不存 workspace_path，按 workspace_id 反查项目目录取真实路径（执行需要）。
    let ws_path = state
        .db
        .get_project_directory_by_id(ws_id)
        .await
        .ok()
        .flatten()
        .map(|p| p.path)
        .unwrap_or_default();
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
    // 构造执行请求：trigger_type=discussion 让完成回写走 discussion 分支；
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
        trigger_type: TRIGGER_DISCUSSION.to_string(),
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
    .await?;
    // start_todo_execution 在 record_id 为 None 时已返回 Err，此处再兜底一次。
    let record_id = result
        .record_id
        .ok_or_else(|| AppError::Internal("执行启动失败：未获取到执行记录 ID".to_string()))?;
    Ok((todo_id, record_id))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET .../tasks/{id}/posts — 列出该任务全部帖子（主楼层 + 楼中楼），前端按 parent 分组。
pub async fn list_posts(
    State(state): State<AppState>,
    Path((_ws, task_id)): Path<(i64, i64)>,
    Query(q): Query<ListPostsQuery>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let limit = q.limit.unwrap_or(200).clamp(1, 500);
    let posts = state
        .db
        .list_all_task_posts(task_id, limit)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::ok(serde_json::json!({ "items": posts })))
}

/// GET .../tasks/{id}/posts/{pid} — 单帖（前端轮询占位帖状态用）。
pub async fn get_post(
    State(state): State<AppState>,
    Path((_ws, _task_id, pid)): Path<(i64, i64, i64)>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let post = state
        .db
        .get_task_post(pid)
        .await
        .map_err(AppError::from)?
        .ok_or(AppError::NotFound)?;
    // Model → Value：与 list/delete 返回类型统一为 ApiResponse<Value>。
    Ok(ApiResponse::ok(
        serde_json::to_value(&post).unwrap_or(serde_json::Value::Null),
    ))
}

/// DELETE .../tasks/{id}/posts/{pid} — 删帖（MVP：直接硬删，不区分作者，因项目无用户系统）。
pub async fn delete_post(
    State(state): State<AppState>,
    Path((_ws, _task_id, pid)): Path<(i64, i64, i64)>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let affected = state
        .db
        .delete_task_post(pid)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::ok(serde_json::json!({ "deleted": affected })))
}

/// POST .../tasks/{id}/posts — 创建人帖；含 @专家/@执行器 时同时触发执行并写占位帖。
pub async fn create_post(
    State(state): State<AppState>,
    Path((_ws, task_id)): Path<(i64, i64)>,
    Json(req): Json<CreatePostRequest>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let task = state
        .db
        .get_task(task_id)
        .await
        .map_err(AppError::from)?
        .ok_or(AppError::NotFound)?;
    // trim 后判空，避免纯空白帖。
    let content = req.content.trim();
    if content.is_empty() {
        return Err(AppError::BadRequest("内容不能为空".to_string()));
    }
    // 楼中楼：校验 parent 属于本任务且为主楼层（parent_post_id IS NULL），保证深度≤1。
    if let Some(pid) = req.parent_post_id {
        validate_reply_parent(&state, task_id, pid).await?;
    }
    // 解析 @ 并解析为结构化提及（执行器/专家）。
    let tokens = extract_at_tokens(content);
    let mentions = resolve_mentions(&tokens, &state).await;
    let mentions_json = serialize_mentions(&mentions);

    // 先写人帖（无论是否触发执行，人帖都落库）。
    let human = state
        .db
        .create_task_post(NewPost {
            task_id,
            parent_post_id: req.parent_post_id,
            kind: KIND_HUMAN,
            author_name: "我",
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

    // 仅当提及了执行器或专家才触发执行；否则就是纯人帖。
    let has_trigger = first_executor(&mentions).is_some() || first_expert(&mentions).is_some();
    let agent_post = if has_trigger {
        Some(create_agent_post(&state, &task, &mentions, &mentions_json, content).await)
    } else {
        None
    };

    Ok(ApiResponse::ok(serde_json::json!({
        "human_post": human,
        "agent_post": agent_post,
    })))
}

/// 触发执行并写智能体占位帖；触发失败时写一条 failed 帖（不阻塞人帖已落库）。
/// `post_content` 是触发它的那条人帖正文——执行器必须看到真实诉求才能作答。
async fn create_agent_post(
    state: &AppState,
    task: &tasks::Model,
    mentions: &[MentionDto],
    mentions_json: &str,
    post_content: &str,
) -> serde_json::Value {
    let author = agent_author(mentions);
    let executor = first_executor(mentions);
    let expert = first_expert(mentions);
    let prompt = build_carrier_prompt(task, post_content);
    match trigger_discussion_execution(state, task, mentions, &prompt).await {
        Ok((todo_id, record_id)) => {
            let placeholder = format!("{author} 正在干活…");
            insert_agent_post(
                state, task.id, &author, executor, expert, &placeholder,
                mentions_json, STATUS_RUNNING, Some(record_id), Some(todo_id),
            )
            .await
        }
        Err(e) => {
            // 触发失败也留痕：写一条 failed 帖，让用户看到「没能启动」。
            let failed_content = format!("{author} 触发失败：{e:?}");
            insert_agent_post(
                state, task.id, &author, executor, expert, &failed_content,
                mentions_json, STATUS_FAILED, None, None,
            )
            .await
        }
    }
}

/// 插入一条智能体帖并序列化为 JSON 响应值；插入失败时返回 `{error}` 而非 panic。
///
/// 参数较多但都描述同一条智能体帖，聚合成结构体会与 `NewPost` 重复，
/// 故沿用 `finalize_normal_completion` 的 `too_many_arguments` 豁免（仓库既定模式）。
#[allow(clippy::too_many_arguments)]
async fn insert_agent_post(
    state: &AppState,
    task_id: i64,
    author: &str,
    executor: Option<&str>,
    expert: Option<&str>,
    content: &str,
    mentions_json: &str,
    status: &str,
    source_execution_id: Option<i64>,
    source_todo_id: Option<i64>,
) -> serde_json::Value {
    let post = state
        .db
        .create_task_post(NewPost {
            task_id,
            parent_post_id: None,
            kind: KIND_AGENT,
            author_name: author,
            executor,
            expert_name: expert,
            content,
            mentions_json,
            status,
            source_execution_id,
            source_todo_id,
        })
        .await;
    match post {
        Ok(p) => serde_json::to_value(&p).unwrap_or(serde_json::Value::Null),
        Err(e) => serde_json::json!({ "error": e.to_string() }),
    }
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
// 单元测试（纯函数部分）
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

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
}
