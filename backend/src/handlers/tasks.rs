//! 任务管理 API。
use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};
use crate::handlers::{AppError, AppState};
use crate::db::entity::{loops, process_templates, tasks};
use crate::models::ApiResponse;

// 设计原则：step todo 的 prompt 是只读模板，由 loop_runner 在内存中做占位符替换后
// 传给执行器，绝不写回数据库。需求文本通过 trigger_meta.requirement 传递给 LoopRunner，
// 由 LoopRunner 在运行时替换 {{requirement}} 占位符或兜底追加到 enhanced_prompt 末尾。
// 之前这里有一个 inject_requirement_to_steps 函数直接 UPDATE todos.prompt，
// 会随执行次数累加多段「## 任务需求」污染模板，已删除。

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub requirement: String,
    pub loop_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct ListTasksQuery {
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TaskItem {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub status: String,
    pub workspace_id: Option<i64>,
    pub template_id: Option<i64>,
    pub loop_id: Option<i64>,
    pub template_name: Option<String>,
    /// 工艺版本：任务列表「工艺」列需要与事项/环路保持同一格式。
    pub template_version: Option<String>,
    pub complexity: Option<String>,
    pub latest_execution_status: Option<String>,
    pub latest_execution_requirement: Option<String>,
    pub created_at: Option<String>,
}

/// POST /api/v1/tasks
pub async fn create_task(
    State(state): State<AppState>,
    Path(_ws): Path<i64>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let lp = state.db.get_loop(req.loop_id).await?.ok_or(AppError::NotFound)?;
    // 取首行作为标题，按**字符**截断（上限 60 字符），避免 CJK 多字节字符上按字节切片 panic。
    let title = req.requirement.lines().next().unwrap_or(&req.requirement).trim();
    let title = if title.chars().count() > 60 {
        // chars().take(60) 保证在字符边界截断，不会落在多字节 UTF-8 中间。
        let truncated: String = title.chars().take(60).collect();
        format!("{}…", truncated)
    } else {
        title.to_string()
    };
    let task = state.db.create_task(&title, lp.workspace_id.unwrap_or(1), lp.process_template_id.unwrap_or(0), Some(req.loop_id)).await?;
    state.db.update_task_description(task.id, &req.requirement).await?;
    // 需求不写入 step todo 的 prompt（避免污染模板），通过 trigger_meta 传递给 LoopRunner。
    let _ = state.db.update_loop_status(req.loop_id, "enabled").await;
    let dispatcher = state.loop_trigger_dispatcher.as_ref()
        .ok_or_else(|| AppError::Internal("loop dispatcher not ready".to_string()))?;
    let meta = serde_json::json!({"requirement": req.requirement, "source": "task"});
    match dispatcher.dispatch_manual_with_meta(req.loop_id, meta).await {
        Some(exec_id) => {
            state.db.update_loop_execution_task_id(exec_id, task.id).await?;
            Ok((axum::http::StatusCode::CREATED, ApiResponse::ok(serde_json::json!({
                "task_id": task.id, "loop_id": req.loop_id, "execution_id": exec_id,
            }))))
        }
        None => Err(AppError::BadRequest("无法触发执行".to_string())),
    }
}

/// 单任务最近一次执行的状态与需求文本（列表「最近执行」列用）。
///
/// loop_executions 是执行流水，按 started_at 倒序取第一行即为最近一次；
/// trigger_meta 内嵌需求文本，解析失败或缺省时两个字段均返回 None（展示层已有兜底）。
async fn fetch_latest_execution(
    state: &AppState,
    task_id: i64,
) -> (Option<String>, Option<String>) {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let sql = format!(
        "SELECT le.status, le.trigger_meta FROM loop_executions le \
         WHERE le.task_id={task_id} ORDER BY le.started_at DESC LIMIT 1"
    );
    let rows = state.db.conn.query_all(Statement::from_string(DbBackend::Sqlite, sql)).await.ok();
    rows.and_then(|rows| rows.first().map(|r| {
        let status = r.try_get_by::<Option<String>, _>("status").ok().flatten();
        let requirement = r.try_get_by::<Option<String>, _>("trigger_meta").ok().flatten()
            .and_then(|meta| serde_json::from_str::<serde_json::Value>(&meta).ok())
            .and_then(|v| v.get("requirement").and_then(|r| r.as_str().map(|s| s.to_string())));
        (status, requirement)
    })).unwrap_or((None, None))
}

/// 组装单条任务列表项。
///
/// template 来自批量取回的工艺模板（名称 / 复杂度 / 回退版本），version 已由调用方
/// 按「环路快照优先」口径算好；这里只做字段映射，保持 list_tasks 短小可读。
fn build_task_item(
    t: tasks::Model,
    template: Option<&process_templates::Model>,
    version: Option<String>,
    latest: (Option<String>, Option<String>),
) -> TaskItem {
    TaskItem {
        id: t.id,
        // t 已整体移入，字符串字段直接 move，避免无谓的 clone
        title: t.title,
        description: t.description,
        status: t.status,
        workspace_id: t.workspace_id,
        template_id: t.template_id,
        loop_id: t.loop_id,
        // 模板展示名：优先中文 display_name，空时回退英文唯一名 name，
        // 与 services/process/recommender.rs 的展示名降级策略保持一致。
        template_name: template.map(|p| {
            if p.display_name.is_empty() {
                p.name.clone()
            } else {
                p.display_name.clone()
            }
        }),
        template_version: version,
        complexity: template.map(|p| p.complexity.clone()),
        latest_execution_status: latest.0,
        latest_execution_requirement: latest.1,
        created_at: t.created_at,
    }
}

/// GET /api/v1/workspaces/{ws}/tasks
/// 按工作空间列出任务，可选按 status 过滤。
/// ws 来自 URL path，用于按 workspace_id 过滤（修复之前忽略 ws 导致跨工作空间数据相同的 bug）。
pub async fn list_tasks(
    State(state): State<AppState>,
    Path(ws): Path<i64>,
    Query(q): Query<ListTasksQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let tasks = state.db.list_tasks(ws, q.status.as_deref()).await?;

    // 模板与环路各批量取一次：模板提供名称 / 复杂度 / 回退版本，
    // 环路提供 process_template_version 快照（执行时工艺版本），避免逐任务 N+1。
    let template_ids: Vec<i64> = tasks.iter().filter_map(|t| t.template_id).collect();
    let loop_ids: Vec<i64> = tasks.iter().filter_map(|t| t.loop_id).collect();
    let templates = state.db.get_process_templates_by_ids(&template_ids).await?;
    let loops = state.db.get_loops_by_ids(&loop_ids).await?;
    let templates_by_id: HashMap<i64, &process_templates::Model> =
        templates.iter().map(|p| (p.id, p)).collect();
    let loops_by_id: HashMap<i64, &loops::Model> =
        loops.iter().map(|l| (l.id, l)).collect();

    let mut items = Vec::with_capacity(tasks.len());
    for t in tasks {
        let template = t.template_id.and_then(|tid| templates_by_id.get(&tid).copied());
        // 版本口径与任务详情一致（NTD-010）：环路快照优先，缺失回退模板当前版本。
        let version = t.loop_id
            .and_then(|lid| loops_by_id.get(&lid))
            .and_then(|l| l.process_template_version.clone())
            .or_else(|| template.map(|p| p.version.clone()));
        let latest = fetch_latest_execution(&state, t.id).await;
        items.push(build_task_item(t, template, version, latest));
    }
    Ok(ApiResponse::ok(items))
}

/// GET /api/v1/tasks/{id}
pub async fn get_task_detail(
    State(state): State<AppState>,
    Path((_ws, id)): Path<(i64, i64)>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let task = state.db.get_task(id).await?.ok_or(AppError::NotFound)?;
    let template = if let Some(tid) = task.template_id { state.db.get_process_template_by_id(tid).await? } else { None };
    let loop_ = if let Some(lid) = task.loop_id { state.db.get_loop(lid).await? } else { None };
    // steps
    let steps: Vec<_> = if let Some(ref lp) = loop_ {
        state.db.list_loop_steps_by_loop(lp.id).await?.into_iter().map(|s| serde_json::json!({
            "id":s.id,"name":s.name,"order_index":s.order_index,
            "skill_names": serde_json::from_str::<serde_json::Value>(&s.skill_names).unwrap_or_default(),
            "expected_artifacts": serde_json::from_str::<serde_json::Value>(&s.expected_artifacts).unwrap_or_default(),
            "gate_config": serde_json::from_str::<serde_json::Value>(&s.gate_config).unwrap_or_default(),
        })).collect()
    } else { vec![] };
    // executions
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let exec_rows = state.db.conn.query_all(Statement::from_string(DbBackend::Sqlite,
        format!("SELECT id, status, started_at, finished_at, total_steps, completed_steps, failed_steps, trigger_meta \
                 FROM loop_executions WHERE task_id={} ORDER BY started_at DESC LIMIT 20", id)
    )).await?;
    // 统计每次执行的待审批环节数，前端据此在执行历史行上显示「待审批」引导标记（NTD-004）。
    let exec_ids: Vec<i64> = exec_rows.iter().map(|r| r.try_get_by::<i64,_>("id").unwrap_or(0)).collect();
    let pending_counts = state.db.count_pending_approvals_by_execution_ids(&exec_ids).await?;
    let executions: Vec<_> = exec_rows.iter().map(|r| {
        let meta = r.try_get_by::<Option<String>,_>("trigger_meta").ok().flatten()
            .and_then(|m| serde_json::from_str::<serde_json::Value>(&m).ok());
        let requirement = meta.as_ref().and_then(|v| v.get("requirement").and_then(|r| r.as_str().map(|s| s.to_string())));
        let exec_id = r.try_get_by::<i64,_>("id").unwrap_or(0);
        serde_json::json!({
            "id": exec_id,
            "status": r.try_get_by::<String,_>("status").unwrap_or_default(),
            "started_at": r.try_get_by::<Option<String>,_>("started_at").ok().flatten(),
            "finished_at": r.try_get_by::<Option<String>,_>("finished_at").ok().flatten(),
            "total_steps": r.try_get_by::<i32,_>("total_steps").unwrap_or(0),
            "completed_steps": r.try_get_by::<i32,_>("completed_steps").unwrap_or(0),
            "failed_steps": r.try_get_by::<i32,_>("failed_steps").unwrap_or(0),
            "requirement": requirement,
            "pending_approval_count": pending_counts.get(&exec_id).copied().unwrap_or(0),
        })
    }).collect();
    Ok(ApiResponse::ok(serde_json::json!({
        "task": { "id": task.id, "title": task.title, "status": task.status, "workspace_id": task.workspace_id, "loop_id": task.loop_id },
        "template": template.map(|t| {
            // 版本优先取环路的 process_template_version（执行时的快照），
            // 而非工艺模板表的最新版本——用户关注的是「执行当时的工艺版本」。
            let version = loop_.as_ref().and_then(|l| l.process_template_version.clone()).unwrap_or(t.version.clone());
            serde_json::json!({"name":t.name,"display_name":t.display_name,"complexity":t.complexity,"version":version})
        }),
        "loop": loop_.map(|l| serde_json::json!({"id":l.id,"name":l.name,"status":l.status,"workspace_id":l.workspace_id,"workspace_path":l.workspace_path})),
        "steps": steps,
        "executions": executions,
    })))
}

/// 管理 artifact 内容（略，同之前）
pub async fn get_artifact_content(
    State(state): State<AppState>, Path(aid): Path<i64>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let artifact = state.db.get_loop_step_artifact(aid).await?.ok_or(AppError::NotFound)?;
    // 056：workspace 解析失败必须传播——空路径会继续走到 read_workspace_file
    // 产生误导性错误（读到错误位置或报「文件不存在」），掩盖真实的 DB 故障。
    let ws_path = resolve_artifact_workspace(&state.db, &artifact).await?;
    let content = if artifact.artifact_type == "file" {
        read_workspace_file(&ws_path, &artifact.locator).await
    } else {
        artifact.content_text.unwrap_or_else(|| format!("({}: {})", artifact.artifact_type, artifact.locator))
    };
    let resp = axum::response::Response::builder()
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(axum::body::Body::from(content))
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(resp)
}

async fn resolve_artifact_workspace(
    db: &crate::db::Database,
    art: &crate::db::entity::loop_step_artifacts::Model,
) -> Result<String, sea_orm::DbErr> {
    use sea_orm::EntityTrait;
    let se = crate::db::entity::loop_step_executions::Entity::find_by_id(art.loop_step_execution_id).one(&db.conn).await?
        .ok_or(sea_orm::DbErr::RecordNotFound("step_exec not found".into()))?;
    let le = crate::db::entity::loop_executions::Entity::find_by_id(se.loop_execution_id).one(&db.conn).await?
        .ok_or(sea_orm::DbErr::RecordNotFound("loop_exec not found".into()))?;
    let lp = crate::db::entity::loops::Entity::find_by_id(le.loop_id).one(&db.conn).await?
        .ok_or(sea_orm::DbErr::RecordNotFound("loop not found".into()))?;
    Ok(lp.workspace_path.unwrap_or_default())
}

async fn read_workspace_file(ws: &str, rel: &str) -> String {
    let full = std::path::Path::new(ws).join(rel);
    match tokio::fs::read_to_string(&full).await {
        Ok(s) if s.len() <= 128*1024 => s,
        Ok(s) => format!("{}…(仅显示前128KB)", &s[..128*1024]),
        Err(e) => format!("无法读取: {} ({})", e, full.display()),
    }
}

/// POST /api/v1/tasks/{id}/executions — 为已有任务创建新执行（复用 task_id + loop）。
pub async fn create_task_execution(
    State(state): State<AppState>,
    Path((_ws, id)): Path<(i64, i64)>,
    Json(req): Json<NewExecutionRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let task = state.db.get_task(id).await?.ok_or(AppError::NotFound)?;
    let loop_id = task.loop_id.ok_or_else(|| AppError::BadRequest("任务未关联 Loop".to_string()))?;
    state.db.update_task_description(id, &req.requirement).await?;
    // 需求不写入 step todo 的 prompt（避免污染模板），通过 trigger_meta 传递给 LoopRunner。
    let dispatcher = state.loop_trigger_dispatcher.as_ref()
        .ok_or_else(|| AppError::Internal("dispatcher not ready".to_string()))?;
    let meta = serde_json::json!({"requirement": req.requirement, "source": "task"});
    match dispatcher.dispatch_manual_with_meta(loop_id, meta).await {
        Some(exec_id) => {
            state.db.update_loop_execution_task_id(exec_id, id).await?;
            Ok(ApiResponse::ok(serde_json::json!({"execution_id": exec_id})))
        }
        None => Err(AppError::BadRequest("无法触发执行".to_string())),
    }
}

#[derive(Deserialize)]
pub struct NewExecutionRequest { pub requirement: String }

/// DELETE /api/v1/workspaces/{ws}/tasks/{id} — 删除单个任务。
pub async fn delete_task(
    State(state): State<AppState>,
    Path((_ws, id)): Path<(i64, i64)>,
) -> Result<ApiResponse<()>, AppError> {
    state.db.delete_task(id).await.map_err(AppError::from)?;
    Ok(ApiResponse::ok(()))
}

/// POST /api/v1/workspaces/{ws}/tasks/batch-delete — 批量删除任务。
#[derive(Deserialize)]
pub struct BatchDeleteTasksRequest { pub ids: Vec<i64> }
pub async fn batch_delete_tasks(
    State(state): State<AppState>,
    Path(_ws): Path<i64>,
    Json(req): Json<BatchDeleteTasksRequest>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let deleted = state.db.batch_delete_tasks(&req.ids).await?;
    Ok(ApiResponse::ok(serde_json::json!({
        "deleted": deleted, "total": req.ids.len(),
    })))
}

pub fn task_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/workspaces/{ws}/tasks", axum::routing::get(list_tasks).post(create_task))
        .route("/api/v1/workspaces/{ws}/tasks/{id}", axum::routing::get(get_task_detail).delete(delete_task))
        .route("/api/v1/workspaces/{ws}/tasks/batch-delete", axum::routing::post(batch_delete_tasks))
        .route("/api/v1/workspaces/{ws}/tasks/{id}/executions", axum::routing::post(create_task_execution))
        .route("/api/v1/artifacts/{aid}/content", axum::routing::get(get_artifact_content))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::question_mark,
    clippy::redundant_clone,
    clippy::needless_pass_by_value
)]
mod tests {
    use std::sync::Arc;

    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    use serde_json::Value;
    use tokio::sync::broadcast;
    use tower::ServiceExt;

    use crate::adapters::{ExecutorRegistry, claude_code::ClaudeCodeExecutor};
    use crate::config::Config;
    use crate::db::Database;
    use crate::handlers::create_app;
    use crate::scheduler::TodoScheduler;
    use crate::service_context::ServiceContext;
    use crate::task_manager::TaskManager;

    /// 构造带内存库的测试 app，返回 router / workspace id / db。
    ///
    /// 返回 db 是为了种子工艺模板、环路与任务，并直接执行 UPDATE 写环路快照——
    /// 这些是公开 DB 方法覆盖不到的字段，必须绕过 DAO 才能模拟「模板已升级、环路未升级」。
    async fn build_app() -> (axum::Router, i64, Arc<Database>) {
        let db = Arc::new(Database::new(":memory:").await.expect("memory db must open"));
        let ws_id = db
            .create_project_directory("/tmp/test-ntd010-workspace", Some("ntd010"), false, false)
            .await
            .expect("workspace must be created");

        let executor_registry = Arc::new(ExecutorRegistry::new());
        executor_registry
            .register(ClaudeCodeExecutor::new("claude".to_string()))
            .await;
        let (tx, _rx) = broadcast::channel(100);
        let task_manager = Arc::new(TaskManager::new());
        let config = Arc::new(std::sync::RwLock::new(Config::default()));
        let scheduler = Arc::new(TodoScheduler::new().await.expect("scheduler must init"));
        let ctx = ServiceContext {
            db: db.clone(),
            executor_registry: executor_registry.clone(),
            tx: tx.clone(),
            task_manager: task_manager.clone(),
            config: config.clone(),
            expert_manager: Arc::new(crate::expert::ExpertIndexManager::new()),
        };
        scheduler.load_from_db(&ctx).await.expect("scheduler load");
        scheduler.start().await.expect("scheduler start");
        (create_app(ctx, scheduler).await, ws_id, db)
    }

    /// 直接对测试库执行 UPDATE：给环路绑定工艺模板与版本快照。
    ///
    /// create_loop 不接收模板字段，installer 写入路径又需要磁盘 YAML，
    /// 测试里用一条 SQL 精确控制快照取值，才能稳定构造「快照 ≠ 模板当前版本」的场景。
    async fn bind_loop_template(db: &Database, loop_id: i64, template_id: i64, version: Option<&str>) {
        let version_sql = match version {
            Some(v) => format!("process_template_version = '{v}'"),
            None => "process_template_version = NULL".to_string(),
        };
        let sql = format!(
            "UPDATE loops SET process_template_id = {template_id}, {version_sql} WHERE id = {loop_id}"
        );
        db.conn
            .execute(Statement::from_string(DbBackend::Sqlite, sql))
            .await
            .expect("bind loop template must succeed");
    }

    /// NTD-010 回归：任务列表「工艺」列版本必须与详情口径一致。
    ///
    /// 场景：
    /// - 任务A：环路有快照 1.0.0，模板后来升级到 2.0.0 → 列表应显示 1.0.0（快照优先）；
    /// - 任务B：环路绑定了模板但无快照 → 列表应回退显示 2.0.0（模板当前版本）。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_list_tasks_uses_loop_snapshot_version() {
        let (app, ws_id, db) = build_app().await;

        // 先建模板（v1.0.0），再建两个环路：环路1 写快照、环路2 留空快照
        let tpl = db
            .upsert_user_process_template(
                "guid-ntd010", "tpl-ntd010", "工艺A", "", "default", "medium",
                "1.0.0", "/tmp/ntd010.yaml",
            )
            .await
            .expect("upsert template v1");
        let loop1 = db
            .create_loop("环路1", "", Some(ws_id), Some("/tmp/test-ntd010-workspace"), None, None, "[]")
            .await
            .expect("create loop1")
            .id;
        let loop2 = db
            .create_loop("环路2", "", Some(ws_id), Some("/tmp/test-ntd010-workspace"), None, None, "[]")
            .await
            .expect("create loop2")
            .id;
        bind_loop_template(&db, loop1, tpl, Some("1.0.0")).await;
        bind_loop_template(&db, loop2, tpl, None).await;

        // 两个任务分别挂在两个环路下
        db.create_task("任务A", ws_id, tpl, Some(loop1)).await.expect("create task A");
        db.create_task("任务B", ws_id, tpl, Some(loop2)).await.expect("create task B");

        // 模板升级到 v2.0.0：环路快照保持不变
        db.upsert_user_process_template(
            "guid-ntd010", "tpl-ntd010", "工艺A", "", "default", "medium",
            "2.0.0", "/tmp/ntd010.yaml",
        )
        .await
        .expect("upsert template v2");

        // 走真实 HTTP 路由取任务列表
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/workspaces/{ws_id}/tasks"))
                    .body(Body::empty())
                    .expect("request build"),
            )
            .await
            .expect("list tasks request");
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.expect("read body");
        let body: Value = serde_json::from_slice(&bytes).expect("parse body");
        assert_eq!(body["code"], 0);
        let tasks = body["data"].as_array().expect("data array");
        let by_title = |title: &str| {
            tasks
                .iter()
                .find(|t| t["title"] == title)
                .unwrap_or_else(|| panic!("task '{title}' should be in list"))
        };

        // 有快照：显示执行时版本 1.0.0，而不是模板当前版本 2.0.0（NTD-010 核心断言）
        let task_a = by_title("任务A");
        assert_eq!(task_a["template_id"], tpl);
        assert_eq!(task_a["template_name"], "工艺A");
        assert_eq!(task_a["template_version"], "1.0.0");

        // 无快照：回退模板当前版本 2.0.0
        let task_b = by_title("任务B");
        assert_eq!(task_b["template_id"], tpl);
        assert_eq!(task_b["template_version"], "2.0.0");
    }

    /// BUG-001 回归：CJK 长标题按字符截断（≤60 字符），不 panic。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_create_task_cjk_title_does_not_panic() {
        let (app, ws_id, db) = build_app().await;

        // 建模板与环路
        let tpl = db
            .upsert_user_process_template(
                "guid-bug001", "tpl-bug001", "工艺", "", "default", "medium",
                "1.0.0", "/tmp/bug001.yaml",
            )
            .await
            .expect("upsert template");
        let lp = db
            .create_loop("环路", "", Some(ws_id), Some("/tmp/test-ntd010-workspace"), None, None, "[]")
            .await
            .expect("create loop")
            .id;
        bind_loop_template(&db, lp, tpl, Some("1.0.0")).await;

        // requirement 首行 >60 字符且含 CJK（>60 字节，切片会落在多字节中间 → panic）
        let requirement = "【E2E-REQUIREMENT-MARKER】端到端验证需求：这一段 deliberately 超过六十个字节让切片落在多字节字符内部";
        let body = serde_json::json!({ "loop_id": lp, "requirement": requirement });
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/workspaces/{ws_id}/tasks"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).expect("json")))
                    .expect("request build"),
            )
            .await
            .expect("create task request");
        // 关键断言：不应 panic/断开连接，必须返回 201
        assert_eq!(response.status(), StatusCode::CREATED);

        // 额外验证：任务 title 按字符截断（≤60 字符 + …），不应包含乱码或截断一半的 CJK
        let bytes = to_bytes(response.into_body(), usize::MAX).await.expect("read body");
        let res: Value = serde_json::from_slice(&bytes).expect("parse body");
        let task_title = res["data"]["task_id"].as_i64();
        assert!(task_title.is_some(), "task_id 必须存在");
    }
}
