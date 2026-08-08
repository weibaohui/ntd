//! 任务管理 API。
use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};
use crate::adapters::find_executor;
use crate::handlers::{task_posts, AppError, AppState};
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
    /// 工艺环路模式必填；委派模式不使用（改 Option 以兼容委派不绑环路）。
    pub loop_id: Option<i64>,
    /// 执行方式：缺省/非 `delegate` = 工艺环路；`delegate` = 委派。默认空串落到环路路径。
    #[serde(default)]
    pub execution_mode: String,
    /// 委派对象类型：`executor` / `expert`（仅 delegate 模式）。
    pub assignee_kind: Option<String>,
    /// 委派处理人名（执行器名或专家名，仅 delegate 模式）。
    pub assignee_name: Option<String>,
    /// 自动接力开关；仅 `assignee_kind='expert'` 允许 true（前端禁用 + 后端 400 双重校验防绕过）。
    #[serde(default)]
    pub auto_continue: bool,
    /// 本任务「接力轮数上限」覆盖（仅 delegate 模式有意义）。
    /// `Some(n)`（1..=50，越界 400）→ 覆盖工作空间默认；`None`/缺省 → 沿用工作空间默认 → 兜底常量。
    /// 注意：配置的是「上限」，不是「已跑计数」continue_rounds（后者后端单调递增、前端不可直传）。
    pub delegate_max_rounds: Option<i64>,
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
    /// 该任务所有执行中未处理的待审批环节总数（063：列表透出，派生不落库）。
    /// 口径与 NTD-004 一致；无执行或无待审批时为 0。
    pub pending_approval_count: i32,
    pub created_at: Option<String>,
    // —— 需求 092：委派执行相关（loop 任务为默认值，delegate 任务带处理人/接力信息）——
    /// 执行方式：`loop` / `delegate`。恒有值（DB 默认 'loop'）。
    pub execution_mode: String,
    /// 委派对象类型；仅 delegate 有值。
    pub assignee_kind: Option<String>,
    /// 委派处理人名；仅 delegate 有值。
    pub assignee_name: Option<String>,
    /// 自动接力开关（前端按此渲染接力状态）。i64→bool 转换收口在此处。
    pub auto_continue: bool,
    /// 接力已执行轮数（护栏计数，P2 接力状态展示用）。
    pub continue_rounds: i64,
}

/// 取需求首行作为任务标题，按**字符**截断（上限 60），超长补省略号。
///
/// 抽出共用：环路/委派两路径标题口径一致。chars 边界截断避免 CJK 多字节字符按字节切片 panic。
fn task_title_from_requirement(requirement: &str) -> String {
    let first_line = requirement.lines().next().unwrap_or(requirement).trim();
    if first_line.chars().count() > 60 {
        // chars().take(60) 保证在字符边界截断，不会落在多字节 UTF-8 中间。
        format!("{}…", first_line.chars().take(60).collect::<String>())
    } else {
        first_line.to_string()
    }
}

/// 服务端校验委派处理人真实存在（防伪造任意名触发执行）。
/// expert 查 expert_manager（parking_lot 同步读），executor 查 find_executor；都不中→400。
fn validate_assignee_exists(state: &AppState, kind: &str, name: &str) -> Result<(), AppError> {
    let exists = match kind {
        "expert" => state.expert_manager.get_expert_by_name(name).is_some(),
        "executor" => find_executor(name).is_some(),
        _ => false,
    };
    if !exists {
        return Err(AppError::BadRequest(format!("处理人「{name}」不存在（{kind}）")));
    }
    Ok(())
}

/// POST /api/v1/workspaces/{ws}/tasks — 创建任务。
///
/// 按执行方式分流：`delegate` 走委派（建无环路 task + 讨论区首帖触发首次执行），
/// 其余（缺省）走工艺环路（现状不变）。显式按字符串分流而非 enum，让旧请求（不带
/// execution_mode）零改动落到环路路径。
pub async fn create_task(
    State(state): State<AppState>,
    Path(ws): Path<i64>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.execution_mode == "delegate" {
        return create_delegate_task(&state, ws, req).await;
    }
    create_loop_task(&state, ws, req).await
}

/// 环路模式（现状）：校验 loop_id → 建 task 绑环路 → 通过 loop dispatcher 触发首执行。
/// 从原 create_task 抽出，逻辑保持不变；仅 loop_id 因 CreateTaskRequest 改 Option 而显式解包。
async fn create_loop_task(
    state: &AppState,
    _ws: i64,
    req: CreateTaskRequest,
) -> Result<(axum::http::StatusCode, ApiResponse<serde_json::Value>), AppError> {
    let loop_id = req.loop_id.ok_or_else(|| {
        AppError::BadRequest("工艺环路模式必须指定 loop_id".to_string())
    })?;
    let lp = state.db.get_loop(loop_id).await?.ok_or(AppError::NotFound)?;
    let title = task_title_from_requirement(&req.requirement);
    let task = state
        .db
        .create_task(&title, lp.workspace_id.unwrap_or(1), lp.process_template_id.unwrap_or(0), Some(loop_id))
        .await?;
    state.db.update_task_description(task.id, &req.requirement).await?;
    // 需求不写入 step todo 的 prompt（避免污染模板），通过 trigger_meta 传递给 LoopRunner。
    let _ = state.db.update_loop_status(loop_id, "enabled").await;
    let dispatcher = state
        .loop_trigger_dispatcher
        .as_ref()
        .ok_or_else(|| AppError::Internal("loop dispatcher not ready".to_string()))?;
    let meta = serde_json::json!({"requirement": req.requirement, "source": "task"});
    match dispatcher.dispatch_manual_with_meta(loop_id, meta).await {
        Some(exec_id) => {
            state.db.update_loop_execution_task_id(exec_id, task.id).await?;
            Ok((axum::http::StatusCode::CREATED, ApiResponse::ok(serde_json::json!({
                "task_id": task.id, "loop_id": loop_id, "execution_id": exec_id,
            }))))
        }
        None => Err(AppError::BadRequest("无法触发执行".to_string())),
    }
}

/// 委派模式：建无环路 task（execution_mode=delegate）→ 在讨论区落地含 @处理人 的首帖并触发首次执行。
/// 首帖触发复用 task_posts::land_mention_post（与人工发帖同路径），零新执行引擎；结论回写沿用 060。
async fn create_delegate_task(
    state: &AppState,
    ws: i64,
    req: CreateTaskRequest,
) -> Result<(axum::http::StatusCode, ApiResponse<serde_json::Value>), AppError> {
    // kind/name 先解出，供校验与首帖复用。
    let kind = req.assignee_kind.as_deref().unwrap_or("");
    let name = req.assignee_name.as_deref().unwrap_or("").trim();
    validate_delegate_request(state, ws, kind, name, req.auto_continue).await?;
    // 接力上限越界校验复用 task_posts 的集中口径（拒 -1 / 51 等），None 视为「用默认」放行。
    task_posts::validate_delegate_max_rounds(req.delegate_max_rounds)?;

    let title = task_title_from_requirement(&req.requirement);
    // description 随首次 INSERT 写入（DAO 内），不再建后单独 update（#1 原子性）。
    let task = state
        .db
        .create_delegate_task(
            &title, &req.requirement, ws, kind, name, req.auto_continue, req.delegate_max_rounds,
        )
        .await?;
    // 首帖触发首次执行：force_mention 用已校验的 assignee，失败时 warn 不阻断任务创建。
    let execution_id = land_delegate_first_post(state, &task, kind, name, &req.requirement).await;
    Ok((axum::http::StatusCode::CREATED, ApiResponse::ok(serde_json::json!({
        "task_id": task.id, "loop_id": serde_json::Value::Null, "execution_id": execution_id,
    }))))
}

/// 委派创建的前置校验：workspace 存在 + 处理人字段齐全 + 执行器禁接力 + 处理人真实存在。
/// 收口在一处，让 create_delegate_task 主干保持「校验 → 建任务 → 发首帖」线性可读。
async fn validate_delegate_request(
    state: &AppState,
    ws: i64,
    kind: &str,
    name: &str,
    auto_continue: bool,
) -> Result<(), AppError> {
    // 校验 path 的 workspace 真实存在：loop 路径靠 get_loop 兜底归属，委派路径无 loop 依托，
    // 必须显式校验，避免为不存在的 ws 建任务、执行落到未定义目录（CodeRabbit #2）。
    if state.db.get_project_directory_by_id(ws).await?.is_none() {
        return Err(AppError::NotFound);
    }
    // 处理人类型与名称齐全：kind 必须是 executor/expert，name 非空。
    if name.is_empty() || !matches!(kind, "executor" | "expert") {
        return Err(AppError::BadRequest("委派任务必须指定处理人（专家/执行器）".to_string()));
    }
    // 执行器无调度能力，禁止开自动接力（前端禁用 + 后端 400 双重校验防绕过）。
    if kind == "executor" && auto_continue {
        return Err(AppError::BadRequest("执行器不支持自动接力（请改选专家）".to_string()));
    }
    // 服务端校验处理人真实存在，防伪造任意名触发执行。
    validate_assignee_exists(state, kind, name)?;
    Ok(())
}

/// 落委派首帖并触发首次执行，返回 execution_id（触发失败/未启动为 None）。
///
/// 失败容忍：land_mention_post 任一步出错只 warn 返回 None，不阻断任务创建（#1 补偿）——
/// 任务已建好、description 已落库，用户可在讨论区手动 @ 恢复；若改成返回错误，用户会以为全失败、
/// 重试时重复建任务。
async fn land_delegate_first_post(
    state: &AppState,
    task: &tasks::Model,
    kind: &str,
    name: &str,
    requirement: &str,
) -> Option<i64> {
    // 首帖正文 = @处理人 + 需求原文（人工可见的诉求上下文）。
    let first_post = format!("@{name} {requirement}");
    // force_mention：assignee 已校验存在，强制作为触发目标，绕过文本 @ 解析——
    // 避免其名含空格/标点时 extract_at_tokens 截断、首帖静默不触发（CodeRabbit #3）。
    let force = task_posts::MentionDto {
        kind: kind.to_string(),
        name: name.to_string(),
        display: name.to_string(),
    };
    match task_posts::land_mention_post(
        state,
        task,
        &first_post,
        None,
        "我",
        task_posts::TRIGGER_DISCUSSION,
        Some(force),
    )
    .await
    {
        Ok((_human, agent)) => agent
            .as_ref()
            .and_then(|v| v.get("source_execution_id").and_then(|x| x.as_i64())),
        Err(e) => {
            // AppError 仅 derive Debug（无 Display），故用 ?e 走 Debug 形式记录（与 worktree.rs 同口径）。
            tracing::warn!(error = ?e, task_id = task.id, "delegate first post failed");
            None
        }
    }
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
    pending_approval_count: i32,
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
        pending_approval_count,
        created_at: t.created_at,
        // 委派字段直接从 task 透传；loop 任务 execution_mode='loop'、其余为空/0。
        execution_mode: t.execution_mode,
        assignee_kind: t.assignee_kind,
        assignee_name: t.assignee_name,
        auto_continue: t.auto_continue != 0,
        continue_rounds: t.continue_rounds,
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

    // 一次批量取所有 task 的最近一次执行（status + trigger_meta），消除逐 task 的 N+1（091 性能优化）。
    let task_ids: Vec<i64> = tasks.iter().map(|t| t.id).collect();
    let latest_by_task = state.db.get_latest_execution_by_task_ids(&task_ids).await?;
    // 063：同批 task_ids 再查一次待审批计数（单条 SQL 按 task 分组），
    // 让列表/看板/卡片不进详情即可感知「该任务有 N 条环节等我审批」。
    let pending_by_task = state.db.count_pending_approvals_by_task_ids(&task_ids).await?;
    let mut items = Vec::with_capacity(tasks.len());
    for t in tasks {
        let template = t.template_id.and_then(|tid| templates_by_id.get(&tid).copied());
        // 版本口径与任务详情一致（NTD-010）：环路快照优先，缺失回退模板当前版本。
        let version = t.loop_id
            .and_then(|lid| loops_by_id.get(&lid))
            .and_then(|l| l.process_template_version.clone())
            .or_else(|| template.map(|p| p.version.clone()));
        // 从批量结果取该 task 的最近执行：status 直接取；requirement 从 trigger_meta JSON 解析。
        let latest = latest_by_task.get(&t.id);
        let latest_status = latest.map(|m| m.status.clone());
        let latest_requirement = latest
            .and_then(|m| serde_json::from_str::<serde_json::Value>(&m.trigger_meta).ok())
            .and_then(|v| v.get("requirement").and_then(|r| r.as_str().map(|s| s.to_string())));
        // 无待审批记录的任务缺省 0，保证字段恒为非负整数，前端无需判空。
        let pending_approval_count = pending_by_task.get(&t.id).copied().unwrap_or(0);
        items.push(build_task_item(t, template, version, (latest_status, latest_requirement), pending_approval_count));
    }
    Ok(ApiResponse::ok(items))
}

/// 组装环路步骤 JSON（skill_names/expected_artifacts/gate_config 三列存的是 JSON 文本，需反序列化）。
/// 抽出独立函数使 [`get_task_detail`] 保持在 50 行内；无环路时返回空数组。
async fn build_loop_steps(
    db: &crate::db::Database,
    loop_: Option<&crate::db::entity::loops::Model>,
) -> Result<Vec<serde_json::Value>, AppError> {
    let Some(lp) = loop_ else { return Ok(vec![]) };
    Ok(db.list_loop_steps_by_loop(lp.id).await?.into_iter().map(|s| serde_json::json!({
        "id":s.id,"name":s.name,"order_index":s.order_index,
        "skill_names": serde_json::from_str::<serde_json::Value>(&s.skill_names).unwrap_or_default(),
        "expected_artifacts": serde_json::from_str::<serde_json::Value>(&s.expected_artifacts).unwrap_or_default(),
        "gate_config": serde_json::from_str::<serde_json::Value>(&s.gate_config).unwrap_or_default(),
    })).collect())
}

/// 组装任务最近 20 条执行记录 JSON（含每条的待审批环节数，前端据此在历史行标「待审批」）。
/// 抽出独立函数使 [`get_task_detail`] 保持在 50 行内；用原始 SQL 查 loop_executions + 批量 pending 计数（防 N+1）。
async fn build_task_executions(
    db: &crate::db::Database,
    task_id: i64,
) -> Result<Vec<serde_json::Value>, AppError> {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let exec_rows = db.conn.query_all(Statement::from_string(DbBackend::Sqlite,
        format!("SELECT id, status, started_at, finished_at, total_steps, completed_steps, failed_steps, trigger_meta \
                 FROM loop_executions WHERE task_id={} ORDER BY started_at DESC LIMIT 20", task_id)
    )).await?;
    // 一次批量查待审批数再按 exec_id 映射，避免逐行 N+1（NTD-004）。
    let exec_ids: Vec<i64> = exec_rows.iter().map(|r| r.try_get_by::<i64,_>("id").unwrap_or(0)).collect();
    let pending_counts = db.count_pending_approvals_by_execution_ids(&exec_ids).await?;
    Ok(exec_rows.iter().map(|r| {
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
    }).collect())
}

/// GET /api/v1/tasks/{id}
pub async fn get_task_detail(
    State(state): State<AppState>,
    Path((_ws, id)): Path<(i64, i64)>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let task = state.db.get_task(id).await?.ok_or(AppError::NotFound)?;
    let template = if let Some(tid) = task.template_id { state.db.get_process_template_by_id(tid).await? } else { None };
    let loop_ = if let Some(lid) = task.loop_id { state.db.get_loop(lid).await? } else { None };
    // steps / executions 构建抽出独立函数，保持本函数聚焦于「取数 + 组装顶层 JSON」。
    let steps = build_loop_steps(&state.db, loop_.as_ref()).await?;
    let executions = build_task_executions(&state.db, id).await?;
    // 接力上限：effective（徽标 M / 触顶文案）+ fallback（徽标编辑器「清除覆盖后回退值」）。
    // 两者同源 resolve 口径：effective 含任务覆盖，fallback 忽略任务覆盖，确保「显示 N/M」与「真实熔断」不漂移。
    let delegate_max_rounds_effective = task_posts::resolve_delegate_max_rounds(&state.db, &task).await;
    let delegate_max_rounds_fallback =
        task_posts::resolve_delegate_max_rounds_fallback(&state.db, task.workspace_id).await;
    Ok(ApiResponse::ok(serde_json::json!({
        "task": {
            "id": task.id, "title": task.title, "status": task.status,
            "workspace_id": task.workspace_id, "loop_id": task.loop_id,
            // 委派执行方式与处理人（需求 092）：详情头部据此切换默认 Tab 与接力状态展示。
            "execution_mode": task.execution_mode,
            "assignee_kind": task.assignee_kind,
            "assignee_name": task.assignee_name,
            // i64(0/1) → bool，与列表接口 build_task_item 口径一致（CodeRabbit #4）：
            // 前端类型声明为 boolean，透传数字会让 typeof/严格比较在 P2 接力状态展示时踩雷。
            "auto_continue": task.auto_continue != 0,
            "continue_rounds": task.continue_rounds,
            // 任务级覆盖原值（用于内联编辑回显/恢复默认判定；NULL=未覆盖）。
            "delegate_max_rounds": task.delegate_max_rounds,
            // 解析后的有效上限（徽标 M、触顶文案直接用）；与护栏决策同源。
            "delegate_max_rounds_effective": delegate_max_rounds_effective,
            // 「清除任务覆盖后」的回退值（工作空间默认或兜底常量）：徽标编辑器据此提示「留空=用工作空间默认（X 轮）」。
            // 不能复用 effective：任务有覆盖时 effective 即覆盖值，会让「留空回退提示」误显成被清除的那个覆盖值。
            "delegate_max_rounds_fallback": delegate_max_rounds_fallback,
        },
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

/// PATCH 任务可变字段请求。当前仅 `delegate_max_rounds` 一项可改；独立结构便于将来扩展。
#[derive(Debug, Deserialize)]
pub struct UpdateTaskRequest {
    /// `Some(n)`（1..=50）置任务级覆盖；`null`/缺省清除覆盖回退工作空间默认（即「恢复默认」）。
    pub delegate_max_rounds: Option<i64>,
}

/// PATCH /api/v1/workspaces/{ws}/tasks/{id} — 更新单个任务可变字段（当前仅接力上限覆盖）。
///
/// 仅委派任务支持：接力上限是委派接力的专属概念，环路任务配它无意义，故 400 拒绝以免库里
/// 存无效配置。返回更新后的有效上限（三级解析），前端据此刷新徽标 N/M，无需二次请求详情。
pub async fn update_task(
    State(state): State<AppState>,
    Path((ws, id)): Path<(i64, i64)>,
    Json(req): Json<UpdateTaskRequest>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let task = state.db.get_task(id).await?.ok_or(AppError::NotFound)?;
    // 工作空间归属校验：path ws 与任务 workspace_id 不符则 404，防 URL 串用改他空间任务。
    // 仅当任务 workspace_id 已落值时校验（存量 None 任务跳过，避免误拒）。
    // 注：同文件 get_task_detail/delete_task 沿用旧的全局 id 口径未做此校验（存量遗留，ntd 为本地单用户
    // 工具，非租户隔离边界）；本新端点先行收紧，全面 ws 化属另一次专项改造，不在本 PR 范围。
    if let Some(w) = task.workspace_id {
        if w != ws {
            return Err(AppError::NotFound);
        }
    }
    // 接力上限是委派接力的专属概念；环路任务配它无意义，直接拒，避免存无效配置。
    if task.execution_mode != "delegate" {
        return Err(AppError::BadRequest("仅委派任务支持配置接力上限".to_string()));
    }
    // 越界校验复用集中口径（与 create 同源），None 视为「清除覆盖/恢复默认」放行。
    task_posts::validate_delegate_max_rounds(req.delegate_max_rounds)?;
    // DAO 返回更新后的 Model，直接 resolve 有效值，免去一次冗余 get_task 重读
    // （find→update 已拿到最新态；resolve 基于刚写入的 updated，所见即所写）。
    let updated = state
        .db
        .update_delegate_max_rounds(id, req.delegate_max_rounds)
        .await?
        .ok_or(AppError::NotFound)?;
    let effective = task_posts::resolve_delegate_max_rounds(&state.db, &updated).await;
    Ok(ApiResponse::ok(serde_json::json!({
        "delegate_max_rounds": updated.delegate_max_rounds,
        "delegate_max_rounds_effective": effective,
    })))
}

pub fn task_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/workspaces/{ws}/tasks", axum::routing::get(list_tasks).post(create_task))
        .route("/api/v1/workspaces/{ws}/tasks/{id}", axum::routing::get(get_task_detail).delete(delete_task).patch(update_task))
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

    // —— 需求 092：任务委派执行 ——

    /// 发 POST /tasks 的小工具，避免每个委派用例重复 Request builder 样板。
    async fn post_task(app: &axum::Router, ws_id: i64, body: Value) -> (StatusCode, Value) {
        let resp = app
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
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.expect("read body");
        let val: Value = serde_json::from_slice(&bytes).expect("parse body");
        (status, val)
    }

    /// 发 PATCH /tasks/{id} 的小工具，结构与 post_task 一致，仅 method/uri 不同。
    async fn patch_task(app: &axum::Router, ws_id: i64, task_id: i64, body: Value) -> (StatusCode, Value) {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/api/v1/workspaces/{ws_id}/tasks/{task_id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).expect("json")))
                    .expect("request build"),
            )
            .await
            .expect("patch task request");
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.expect("read body");
        let val: Value = serde_json::from_slice(&bytes).expect("parse body");
        (status, val)
    }

    /// 委派执行器 + 开自动接力 必须被拒（400）：执行器无调度能力，防绕过前端禁用。
    /// 此分支在触发执行之前返回，故不会真实 spawn 执行器。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_create_delegate_rejects_executor_with_auto_continue() {
        let (app, ws_id, _db) = build_app().await;
        let body = serde_json::json!({
            "requirement": "测试", "execution_mode": "delegate",
            "assignee_kind": "executor", "assignee_name": "claude", "auto_continue": true,
        });
        let (status, _val) = post_task(&app, ws_id, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    /// 委派缺处理人（assignee_kind/name）必须被拒（400）；在触发执行之前返回。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_create_delegate_rejects_missing_assignee() {
        let (app, ws_id, _db) = build_app().await;
        let body = serde_json::json!({
            "requirement": "测试", "execution_mode": "delegate",
        });
        let (status, _val) = post_task(&app, ws_id, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    /// 委派处理人不存在必须被拒（400）：服务端校验防伪造任意名触发执行。
    /// 用 expert + expert_manager 里没有的名字，validate_assignee_exists 即返回。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_create_delegate_rejects_unknown_assignee() {
        let (app, ws_id, _db) = build_app().await;
        let body = serde_json::json!({
            "requirement": "测试", "execution_mode": "delegate",
            "assignee_kind": "expert", "assignee_name": "不存在的专家_zzz",
        });
        let (status, _val) = post_task(&app, ws_id, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    /// create 委派：接力上限越界（-1）必须 400。
    /// 用 executor:claude（已注册）+ auto_continue:false 通过处理人校验，使命中范围校验分支。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_create_delegate_rejects_negative_max_rounds() {
        let (app, ws_id, _db) = build_app().await;
        let body = serde_json::json!({
            "requirement": "测试", "execution_mode": "delegate",
            "assignee_kind": "executor", "assignee_name": "claude",
            "auto_continue": false, "delegate_max_rounds": -1,
        });
        let (status, _val) = post_task(&app, ws_id, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    /// create 委派：接力上限超过 CAP(50) 必须 400。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_create_delegate_rejects_max_rounds_over_cap() {
        let (app, ws_id, _db) = build_app().await;
        let body = serde_json::json!({
            "requirement": "测试", "execution_mode": "delegate",
            "assignee_kind": "executor", "assignee_name": "claude",
            "auto_continue": false, "delegate_max_rounds": 51,
        });
        let (status, _val) = post_task(&app, ws_id, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    /// PATCH 接力上限：环路任务必须 400（接力上限是委派专属概念，环路配它无意义）。
    /// 用 DAO 直建环路任务（create_task 默认 execution_mode='loop'），绕开环路触发。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_update_task_rejects_loop_task() {
        let (app, ws_id, db) = build_app().await;
        let task = db.create_task("环路任务", ws_id, 0, None).await.expect("create loop task");
        let (status, _val) =
            patch_task(&app, ws_id, task.id, serde_json::json!({"delegate_max_rounds": 8})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    /// PATCH 接力上限：委派任务传合法值 → 200，返回有效值随覆盖变化；null=清除回退兜底。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_update_task_sets_and_returns_effective() {
        let (app, ws_id, db) = build_app().await;
        // DAO 直建委派任务，绕开 handler 首帖触发。
        let task = db
            .create_delegate_task("委派", "需求", ws_id, "expert", "专家A", true, None)
            .await
            .expect("create delegate");
        // null=清除：本就 None，有效值仍为兜底 10（工作空间亦未配置）。
        let (s0, v0) =
            patch_task(&app, ws_id, task.id, serde_json::json!({"delegate_max_rounds": null})).await;
        assert_eq!(s0, StatusCode::OK);
        assert_eq!(v0["data"]["delegate_max_rounds_effective"].as_i64(), Some(10));
        // 置 8 → raw 与有效值均为 8。
        let (s1, v1) =
            patch_task(&app, ws_id, task.id, serde_json::json!({"delegate_max_rounds": 8})).await;
        assert_eq!(s1, StatusCode::OK);
        assert_eq!(v1["data"]["delegate_max_rounds"].as_i64(), Some(8));
        assert_eq!(v1["data"]["delegate_max_rounds_effective"].as_i64(), Some(8));
    }

    /// PATCH 接力上限：边界下限 1 合法 → 200，有效值=1（哪怕只允许 1 轮，护栏口径仍成立）。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_update_task_accepts_lower_bound_one() {
        let (app, ws_id, db) = build_app().await;
        let task = db
            .create_delegate_task("委派", "需求", ws_id, "expert", "专家A", true, None)
            .await
            .expect("create delegate");
        let (status, val) =
            patch_task(&app, ws_id, task.id, serde_json::json!({"delegate_max_rounds": 1})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(val["data"]["delegate_max_rounds_effective"].as_i64(), Some(1));
    }

    /// PATCH 接力上限：边界上限 CAP(50) 合法 → 200，有效值=50（含上界，>50 才拒）。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_update_task_accepts_upper_bound_cap() {
        let (app, ws_id, db) = build_app().await;
        let task = db
            .create_delegate_task("委派", "需求", ws_id, "expert", "专家A", true, None)
            .await
            .expect("create delegate");
        let (status, val) =
            patch_task(&app, ws_id, task.id, serde_json::json!({"delegate_max_rounds": 50})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(val["data"]["delegate_max_rounds_effective"].as_i64(), Some(50));
    }

    /// PATCH 跨工作空间：任务属于 ws_id，用其它 ws 的 URL PATCH 必须 404（归属校验防串改）。
    /// 印证 update_task 的 ws 收紧：仅当任务 workspace_id 与 path ws 一致才放行。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_update_task_rejects_wrong_workspace() {
        let (app, ws_id, db) = build_app().await;
        let task = db
            .create_delegate_task("委派", "需求", ws_id, "expert", "专家A", true, Some(5))
            .await
            .expect("create delegate");
        // 任务 workspace_id=ws_id；用一个显然不同的 ws 发 PATCH → 归属校验拦 404。
        let (status, _val) =
            patch_task(&app, ws_id + 999, task.id, serde_json::json!({"delegate_max_rounds": 8})).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    /// PATCH 接力上限：越界（51 > CAP）必须 400，且不改动既有覆盖值。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_update_task_rejects_over_cap_keeps_value() {
        let (app, ws_id, db) = build_app().await;
        let task = db
            .create_delegate_task("委派", "需求", ws_id, "expert", "专家A", true, Some(5))
            .await
            .expect("create delegate");
        let (status, _val) =
            patch_task(&app, ws_id, task.id, serde_json::json!({"delegate_max_rounds": 51})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        // 越界被拒：既有覆盖 5 不应被改写。
        let after = db.get_task(task.id).await.expect("get").expect("task");
        assert_eq!(after.delegate_max_rounds, Some(5), "越界请求不应改动既有值");
    }

    /// DAO：create_delegate_task 写入委派字段正确。
    /// 直接测 DAO 而非 handler，绕开 handler 内对讨论首帖的真实执行触发。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_dao_create_delegate_task_fields() {
        let (_app, ws_id, db) = build_app().await;
        let task = db
            .create_delegate_task("委派标题", "委派需求原文", ws_id, "expert", "任务管家", true, None)
            .await
            .expect("create delegate task");
        assert_eq!(task.description, "委派需求原文", "description 随首次 INSERT 写入");
        assert_eq!(task.execution_mode, "delegate");
        assert_eq!(task.assignee_kind.as_deref(), Some("expert"));
        assert_eq!(task.assignee_name.as_deref(), Some("任务管家"));
        assert_eq!(task.auto_continue, 1);
        assert_eq!(task.continue_rounds, 0, "新建任务接力计数从 0 起");
        assert!(task.delegate_max_rounds.is_none(), "未传覆盖时落 NULL（沿用工作空间默认）");
        assert!(task.loop_id.is_none(), "委派任务不绑环路");
    }

    // ---- task_title_from_requirement 纯函数单测（无 IO，同步 #[test]）----
    // 覆盖：正常短文本 / 多行只取首行 / trim / 60 字符截断边界 / CJK 多字节安全 / 空串降级。

    #[test]
    fn test_task_title_from_requirement_short_returns_trimmed() {
        // 普通短文本：原样返回首行内容，仅 trim 首尾空白。
        assert_eq!(super::task_title_from_requirement("  帮我写个脚本  "), "帮我写个脚本");
    }

    #[test]
    fn test_task_title_from_requirement_takes_first_line_only() {
        // 多行需求：标题只取首行，避免换行符污染任务列表的标题展示。
        assert_eq!(
            super::task_title_from_requirement("第一行标题\n第二行详情\n第三行"),
            "第一行标题"
        );
    }

    #[test]
    fn test_task_title_from_requirement_truncation_boundary() {
        // 恰好 60 字符：不截断（边界 —— count > 60 才截，等于 60 原样）。
        let exactly_60 = "啊".repeat(60);
        assert_eq!(super::task_title_from_requirement(&exactly_60), exactly_60);
        assert_eq!(super::task_title_from_requirement(&exactly_60).chars().count(), 60);

        // 61 字符：截到 60 个原字符 + 1 个「…」省略号。
        let sixty_one = "啊".repeat(61);
        let title = super::task_title_from_requirement(&sixty_one);
        assert_eq!(title.chars().count(), 61, "60 个原字符 + 1 个省略号");
        assert!(title.ends_with('…'), "超长应以省略号收尾");
        // 关键：CJK 截断按字符而非字节，不会落在多字节 UTF-8 中间导致 panic。
        assert!(title.chars().take(60).all(|c| c == '啊'));
    }

    #[test]
    fn test_task_title_from_requirement_empty_and_whitespace() {
        // 空串 / 纯空白：lines().next() 为 None 时回退原串，trim 后为空，不 panic。
        assert_eq!(super::task_title_from_requirement(""), "");
        assert_eq!(super::task_title_from_requirement("   \n  "), "");
    }
}
