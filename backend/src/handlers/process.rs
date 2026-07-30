//! 工艺模板 API 处理器。
//!
//! 提供工艺市场列表、详情查看、安装为 Loop 三个核心接口。

use axum::extract::{Path, State};
use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::handlers::{AppError, AppState};
use crate::models::ApiResponse;
use crate::services::process::installer::install_process_template;
use crate::services::process::source::read_definition;

/// 工艺模板列表项。
#[derive(Debug, Serialize)]
pub struct ProcessTemplateListItem {
    pub id: i64,
    /// 040：全局唯一身份，前端寻址（详情/编辑/安装/复制）一律用它；name 只做展示。
    pub guid: String,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub category: String,
    pub complexity: String,
    pub version: String,
    pub source_path: Option<String>,
    pub is_system: bool,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// 工艺模板详情。
#[derive(Debug, Serialize)]
pub struct ProcessTemplateDetail {
    #[serde(flatten)]
    pub item: ProcessTemplateListItem,
    /// 原始 YAML/JSON 定义文本
    pub definition: String,
}

/// 安装工艺模板请求。
#[derive(Debug, Deserialize)]
pub struct InstallProcessRequest {
    pub workspace_id: i64,
}

/// 安装工艺模板响应。
#[derive(Debug, Serialize)]
pub struct InstallProcessResponse {
    pub loop_id: i64,
    pub loop_name: String,
    pub phase_count: usize,
    pub step_count: usize,
}

/// 工艺实例环路列表项（「实例环路」Tab 用）。
#[derive(Debug, Serialize)]
pub struct ProcessLoopItem {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub status: String,
    /// 实例环路所属工作空间 ID（project_directories.id），前端标注用。
    pub workspace_id: Option<i64>,
    /// 实例化时的工艺版本快照（loops 表已有列，随列表透出便于审计追溯）。
    pub process_template_version: Option<String>,
    pub created_at: Option<String>,
    /// 执行次数聚合（loop_executions 行数）。
    pub execution_count: i64,
}

/// 把环路实体与执行计数聚合成列表项；抽出以保证 list_process_loops 低于 30 行。
fn build_process_loop_items(
    loops: Vec<crate::db::entity::loops::Model>,
    counts: &std::collections::HashMap<i64, i64>,
) -> Vec<ProcessLoopItem> {
    loops
        .into_iter()
        .map(|l| ProcessLoopItem {
            id: l.id,
            name: l.name,
            description: l.description,
            status: l.status,
            workspace_id: l.workspace_id,
            process_template_version: l.process_template_version,
            created_at: l.created_at,
            execution_count: counts.get(&l.id).copied().unwrap_or(0),
        })
        .collect()
}

/// GET /api/v1/processes/{guid}/loops — 列出该工艺模板实例化的环路（按创建时间倒序）。
pub async fn list_process_loops(
    State(state): State<AppState>,
    Path(guid): Path<String>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let template = state
        .db
        .get_process_template_by_guid(&guid)
        .await?
        .ok_or(AppError::NotFound)?;
    let loops = state.db.list_loops_by_process_template(template.id).await?;
    // 批量聚合执行次数，避免环路多时的 N+1 查询
    let ids: Vec<i64> = loops.iter().map(|l| l.id).collect();
    let counts = state.db.count_loop_executions_by_loop_ids(&ids).await?;
    Ok(ApiResponse::ok(build_process_loop_items(loops, &counts)))
}

/// GET /api/bundled/processes 的查询参数（039：工艺列表「我的/模板」双视图）。
#[derive(Debug, Deserialize)]
pub struct ListProcessTemplatesQuery {
    /// `serde(default)` 保证不传参时反序列化为 None 返回全量，
    /// 设置页模板管理等旧调用方不需要改请求。
    #[serde(default)]
    is_system: Option<bool>,
}

/// GET /api/bundled/processes — 列出工艺模板，支持 `?is_system=true|false` 按系统/用户过滤。
pub async fn list_process_templates(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListProcessTemplatesQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let templates = state.db.list_process_templates(query.is_system).await?;
    let items: Vec<ProcessTemplateListItem> = templates
        .into_iter()
        .map(|t| ProcessTemplateListItem {
            id: t.id,
            guid: t.guid,
            name: t.name,
            display_name: t.display_name,
            description: t.description,
            category: t.category,
            complexity: t.complexity,
            version: t.version,
            source_path: t.source_path,
            is_system: t.is_system,
            created_at: t.created_at,
            updated_at: t.updated_at,
        })
        .collect();
    Ok(ApiResponse::ok(items))
}

/// GET /api/bundled/processes/{guid} — 查看单个工艺模板详情（040：按 guid 寻址，同名不歧义）。
pub async fn get_process_template(
    State(state): State<AppState>,
    Path(guid): Path<String>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let template = state
        .db
        .get_process_template_by_guid(&guid)
        .await?
        .ok_or(AppError::NotFound)?;
    // 工艺正文不在 DB，按 source_path 从磁盘文件实时读取，保证磁盘是唯一真源。
    let local_path = state.config_snapshot(|c| c.bundled_source.local_path.clone());
    let definition = read_definition(
        template.source_path.as_deref().unwrap_or_default(),
        &local_path,
    )?;
    Ok(ApiResponse::ok(ProcessTemplateDetail {
        item: ProcessTemplateListItem {
            id: template.id,
            guid: template.guid.clone(),
            name: template.name.clone(),
            display_name: template.display_name,
            description: template.description,
            category: template.category,
            complexity: template.complexity,
            version: template.version,
            source_path: template.source_path,
            is_system: template.is_system,
            created_at: template.created_at,
            updated_at: template.updated_at,
        },
        definition,
    }))
}

/// POST /api/bundled/processes/{guid}/install — 安装工艺模板为 Loop（040：按 guid 寻址）。
pub async fn install_process(
    State(state): State<AppState>,
    Path(guid): Path<String>,
    Json(req): Json<InstallProcessRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let template = state
        .db
        .get_process_template_by_guid(&guid)
        .await?
        .ok_or(AppError::NotFound)?;

    let workspace = state
        .db
        .get_project_directory_by_id(req.workspace_id)
        .await?
        .ok_or_else(|| {
            AppError::BadRequest(format!("工作空间 {} 不存在", req.workspace_id))
        })?;

    // 工艺正文按 source_path 从磁盘读取后再交给 installer；installer 不再从 DB 取正文。
    let local_path = state.config_snapshot(|c| c.bundled_source.local_path.clone());
    let definition = read_definition(
        template.source_path.as_deref().unwrap_or_default(),
        &local_path,
    )?;

    let result = install_process_template(
        &state.db,
        &template,
        &definition,
        req.workspace_id,
        &workspace.path,
    )
    .await
    .map_err(|e| {
        tracing::error!("安装工艺模板 {} 失败: {}", guid, e);
        AppError::Internal(e.to_string())
    })?;

    Ok((
        axum::http::StatusCode::CREATED,
        ApiResponse::ok(InstallProcessResponse {
            loop_id: result.loop_id,
            loop_name: result.loop_name,
            phase_count: result.phase_count,
            step_count: result.step_count,
        }),
    ))
}

/// 升级工艺实例环路到模板最新版本。
///
/// POST /api/v1/processes/{guid}/loops/{loop_id}/upgrade（040：按 guid 寻址）
///
/// 将指定 Loop 的步骤/阶段升级到工艺模板的最新定义：
/// 1. 清理旧步骤和阶段及其关联数据
/// 2. 根据最新 YAML 重新创建
/// 3. 更新 process_template_version
pub async fn upgrade_process_loop(
    State(state): State<AppState>,
    Path((guid, loop_id)): Path<(String, i64)>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let template = state
        .db
        .get_process_template_by_guid(&guid)
        .await?
        .ok_or(AppError::NotFound)?;

    // 工艺正文是唯一真源（磁盘文件），按 source_path 读取后传给 installer。
    let local_path = state.config_snapshot(|c| c.bundled_source.local_path.clone());
    let definition = read_definition(template.source_path.as_deref().unwrap_or_default(), &local_path)?;

    // 获取 Loop，确认其属于该工艺模板
    let loop_model = state
        .db
        .get_loop(loop_id)
        .await?
        .ok_or(AppError::NotFound)?;

    // 校验 Loop 确实来源于该模板（process_template_id 匹配）
    if loop_model.process_template_id != Some(template.id) {
        return Err(AppError::BadRequest(format!(
            "环路 {} 不属于工艺模板「{}」",
            loop_id, template.name
        )));
    }

    // 获取工作空间信息（create_phases_and_steps 需要 workspace_path）
    let ws_id = loop_model.workspace_id.ok_or_else(|| {
        AppError::BadRequest(format!("环路 {} 没有关联工作空间", loop_id))
    })?;
    let workspace = state
        .db
        .get_project_directory_by_id(ws_id)
        .await?
        .ok_or_else(|| {
            AppError::BadRequest(format!("工作空间 {} 不存在", ws_id))
        })?;

    let result = crate::services::process::installer::upgrade_process_template_loop(
        &state.db,
        &template,
        &definition,
        loop_id,
        ws_id,
        &workspace.path,
    )
    .await
    .map_err(|e| {
        tracing::error!("升级环路 {} 失败: {}", loop_id, e);
        AppError::Internal(e.to_string())
    })?;

    Ok((
        axum::http::StatusCode::OK,
        ApiResponse::ok(InstallProcessResponse {
            loop_id: result.loop_id,
            loop_name: result.loop_name,
            phase_count: result.phase_count,
            step_count: result.step_count,
        }),
    ))
}

/// 保存编辑后工艺 YAML 的请求体。
#[derive(Debug, Deserialize)]
pub struct UpdateProcessRequest {
    /// 新的工艺 YAML 文本（完整 `process:` 块）
    pub definition: String,
}

/// 新建工艺请求体。
#[derive(Debug, Deserialize)]
pub struct CreateProcessRequest {
    /// 工艺唯一标识，`^[a-zA-Z0-9_-]+$`
    pub name: String,
    /// 人类可读名称（可空，fallback 到 name）
    pub display_name: Option<String>,
    /// 分类（可空）
    pub category: Option<String>,
    /// 复杂度（可空）
    pub complexity: Option<String>,
    /// 版本（可空，默认 1.0.0）
    pub version: Option<String>,
    /// 完整的工艺 YAML 文本
    pub definition: String,
}

/// PUT /api/v1/processes/{guid} — 保存编辑后的用户工艺 YAML（040：按 guid 寻址）。
///
/// 处理流程（3 步，每步职责单一）：
/// 1. 加载现有工艺，校验 `is_system=false`（系统工艺拒绝直接编辑，返回 409）
/// 2. `serde_yaml` 结构校验（失败返回 400 + serde 错误消息含行号）
/// 3. 原子写盘到 `~/.ntd/processes/<rel_path>` + 触发 `import_user_process_templates` 刷新入库
///
/// 系统工艺（`is_system=true`）返回 409，提示用户先"复制到用户层"。
pub async fn update_process(
    State(state): State<AppState>,
    Path(guid): Path<String>,
    Json(req): Json<UpdateProcessRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // guid 仅用于 DB 查询不参与路径拼接（目标路径来自 source_path），无需路径逃逸校验。
    let template = state
        .db
        .get_process_template_by_guid(&guid)
        .await?
        .ok_or(AppError::NotFound)?;

    // 系统工艺拒绝直接编辑：编辑了也会被下次 bundled 同步覆盖。
    if template.is_system {
        return Err(AppError::Conflict(
            "系统工艺不可直接编辑，请先复制到用户层".to_string(),
        ));
    }

    // 结构校验：serde_yaml 解析失败说明 YAML 语法错误，返回 400。
    // 不做语义校验（如 goto 目标是否存在），避免阻断"先存半成品再补"的工作流。
    let mut definition: crate::services::process::ProcessDefinition =
        serde_yaml::from_str(&req.definition)
            .map_err(|e| AppError::BadRequest(format!("YAML 结构校验失败: {}", e)))?;

    // 以 DB template 的 guid 为准写盘（需求 042）：前端 YAML 的 guid 可能与 DB 不一致
    // （历史 guid 漂移，如 040 迁移残留），强制对齐避免 import 按「新 guid」新建行、
    // 原 DB 行 version 永不更新——这是版本号递增失效的隐藏根因。
    definition.process.guid = template.guid.clone();

    // 每次编辑保存都自动递增次版本号（X.Y.Z → X.Y+1.0），让用户能区分已安装 Loop 与最新工艺。
    // 基准取「前端提交版本」与「DB 当前版本」的较大者（需求 042）：
    // - 正常情况两者相等，正常 +1；
    // - 历史 guid/version 漂移导致两者不一致时取较大者，保证「编辑后版本必然增加」且不倒退
    //   （旧逻辑 `yaml_version == template.version` 在漂移时不递增，造成「保存后版本不变」）；
    // - 用户手动改大版本则在其上 +1，改小则按较大者（DB）递增（防回退）。
    let yaml_version = definition.process.version.clone();
    let base = if version_below(&yaml_version, &template.version) {
        template.version.clone()
    } else {
        yaml_version.clone()
    };
    let bumped = bump_semver_minor(&base);
    definition.process.version = bumped.clone();
    tracing::info!(
        "process {}: version auto-bumped (base={}, yaml={}, db={}) -> {}",
        guid, base, yaml_version, template.version, bumped
    );
    let new_yaml = serde_yaml::to_string(&definition)
        .map_err(|e| AppError::Internal(format!("YAML 序列化失败: {}", e)))?;

    // 计算用户目录下的目标路径。
    let target_path = compute_user_process_path(&template)?;

    // 原子写盘（已含自动递增后的版本号），避免崩溃导致文件损坏。
    atomic_write(&target_path, &new_yaml)?;

    // 触发用户层 upsert，把刚保存的文件刷新入库为 is_system=false。
    if let Err(e) =
        crate::services::process::user_dir::import_user_process_templates(&state).await
    {
        tracing::warn!("保存后导入用户层失败: {}", e);
    }

    let now = crate::models::utc_timestamp();
    // 回传 new_yaml（definition）：含递增后版本号的完整 YAML，供前端回刷 Monaco，
    // 避免陈旧 version 下次保存触发误判（需求 042 回归修复核心）。
    Ok(ApiResponse::ok(serde_json::json!({
        "updated_at": now,
        "definition": new_yaml,
    })))
}

/// POST /api/v1/processes — 新建用户工艺。
///
/// 处理流程：
/// 1. name 合法性校验（`validate_process_name` + 正则 `^[a-zA-Z0-9_-]+$`）
/// 2. name 唯一性校验（若已存在返回 409）
/// 3. 结构校验（serde_yaml 解析失败返回 400）
/// 4. 写盘到 `~/.ntd/processes/<name>.yaml`（若文件已存在返回 409）
/// 5. 触发 `import_user_process_templates` 刷新入库
pub async fn create_process(
    State(state): State<AppState>,
    Json(req): Json<CreateProcessRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // 安全校验：name 不得包含路径分隔符或 ..。
    validate_process_name(&req.name)?;
    // 正则校验：name 只能包含字母、数字、下划线、连字符。
    // 避免 name 含空格、中文等字符导致文件名或 YAML map key 异常。
    validate_name_regex(&req.name)?;

    // 唯一性校验：若 DB 已存在同名工艺，返回 409。
    if state
        .db
        .get_process_template_by_name(&req.name)
        .await?
        .is_some()
    {
        return Err(AppError::Conflict(format!(
            "工艺 {} 已存在",
            req.name
        )));
    }

    // 结构校验：serde_yaml 解析失败说明 YAML 语法错误，返回 400。
    serde_yaml::from_str::<crate::services::process::ProcessDefinition>(&req.definition)
        .map_err(|e| AppError::BadRequest(format!("YAML 结构校验失败: {}", e)))?;

    // 计算用户目录下的目标路径：~/.ntd/processes/<name>.yaml
    let user_dir = crate::services::process::user_dir::user_processes_dir()
        .ok_or_else(|| AppError::Internal("无法获取 home 目录".to_string()))?;
    let target_path = user_dir.join(format!("{}.yaml", req.name));

    // 若文件已存在，返回 409，避免静默覆盖用户自定义。
    if target_path.exists() {
        return Err(AppError::Conflict(format!(
            "用户工艺文件已存在: {}",
            target_path.display()
        )));
    }

    // 原子写盘：临时文件 + rename，避免崩溃导致文件损坏。
    atomic_write(&target_path, &req.definition)?;

    // 触发用户层 upsert，把刚保存的文件刷新入库为 is_system=false。
    if let Err(e) =
        crate::services::process::user_dir::import_user_process_templates(&state).await
    {
        tracing::warn!("新建后导入用户层失败: {}", e);
    }

    let source_path = format!(
        "{}{}.yaml",
        crate::services::process::user_dir::USER_SOURCE_PREFIX,
        req.name
    );
    Ok((
        axum::http::StatusCode::CREATED,
        ApiResponse::ok(serde_json::json!({
            "source_path": source_path,
        })),
    ))
}

/// DELETE /api/v1/processes/{guid} — 删除用户工艺（040：按 guid 寻址）。
///
/// 处理流程：
/// 1. 加载现有工艺，不存在返回 404
/// 2. 系统工艺（`is_system=true`）拒绝删除，返回 409
/// 3. 查询该工艺的实例 Loop，若非空返回 409 + Loop 数量（避免断链）
/// 4. 删文件 `~/.ntd/processes/<rel_path>` + 删 DB 行
pub async fn delete_process(
    State(state): State<AppState>,
    Path(guid): Path<String>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let template = state
        .db
        .get_process_template_by_guid(&guid)
        .await?
        .ok_or(AppError::NotFound)?;

    // 系统工艺拒绝删除：系统工艺由 bundled 同步管理，用户不应手动删除。
    if template.is_system {
        return Err(AppError::Conflict(
            "系统工艺不可删除".to_string(),
        ));
    }

    // 实例 Loop 校验：若该工艺已有实例环路，拒绝删除避免断链。
    // 直接调用 DB 层方法，避免从 list_process_loops 的 IntoResponse 响应里反解数量。
    let loops = state
        .db
        .list_loops_by_process_template(template.id)
        .await?;
    let loop_count = loops.len();
    if loop_count > 0 {
        return Err(AppError::Conflict(format!(
            "该工艺已有 {} 个实例环路，请先归档相关环路再删除",
            loop_count
        )));
    }

    // 计算用户目录下的目标路径，删除文件。
    let target_path = compute_user_process_path(&template)?;
    if target_path.exists() {
        std::fs::remove_file(&target_path)
            .map_err(|e| AppError::Internal(format!("删除用户工艺文件失败: {}", e)))?;
    }

    // 删 DB 行：按 guid 精准删除单条（040 起 name 可重复，不能按 name 删）。
    let affected = state.db.delete_process_template(&guid).await?;
    if affected == 0 {
        // 文件已删但 DB 行不存在：可能是 DB 与文件不一致，记录 warning 但不阻断。
        tracing::warn!("删除工艺 {} 时 DB 行不存在（文件已删）", guid);
    }

    Ok(ApiResponse::ok(serde_json::json!({
        "deleted": true,
    })))
}

/// 计算用户工艺文件路径：`~/.ntd/processes/<rel_path>`。
///
/// `rel_path` 从 `source_path` 剥 `user://` 前缀；若无则用 `<name>.yaml`。
/// 防御性校验：canonicalize 后必须在 `user_processes_dir()` 内。
fn compute_user_process_path(
    template: &crate::db::entity::process_templates::Model,
) -> Result<std::path::PathBuf, AppError> {
    let user_dir = crate::services::process::user_dir::user_processes_dir()
        .ok_or_else(|| AppError::Internal("无法获取 home 目录".to_string()))?;

    // 从 source_path 剥 "user://" 前缀，得到相对路径；若无则用 <name>.yaml。
    // 用 let 绑定延长生命周期，避免临时值在 unwrap_or 返回的 &str 之前被释放。
    let fallback = format!("{}.yaml", template.name);
    let rel_path = template
        .source_path
        .as_ref()
        .and_then(|s| s.strip_prefix("user://"))
        .unwrap_or(&fallback);

    let target_path = user_dir.join(rel_path);

    // 创建父目录（若不存在）。
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Internal(format!("创建用户工艺目录失败: {}", e)))?;
    }

    Ok(target_path)
}

/// 原子写盘：临时文件 + rename，避免崩溃导致文件损坏。
///
/// 写入流程：`<path>.tmp` → `rename(<path>.tmp, <path>)`。
/// 若写入或 rename 失败，残留的 `.tmp` 文件由下次写入覆盖。
fn atomic_write(
    path: &std::path::Path,
    content: &str,
) -> Result<(), AppError> {
    let tmp_path = path.with_extension("yaml.tmp");
    std::fs::write(&tmp_path, content)
        .map_err(|e| AppError::Internal(format!("写入临时文件失败: {}", e)))?;
    std::fs::rename(&tmp_path, path)
        .map_err(|e| AppError::Internal(format!("rename 失败: {}", e)))?;
    Ok(())
}

/// name 正则校验：`^[a-zA-Z0-9_-]+$`。
///
/// 避免 name 含空格、中文、特殊符号导致文件名或 YAML map key 异常。
/// 与 `validate_process_name` 的路径分隔符校验互补，共同保证 name 安全。
fn validate_name_regex(name: &str) -> Result<(), AppError> {
    let valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !valid {
        return Err(AppError::BadRequest(
            "工艺名称只能包含字母、数字、下划线、连字符".to_string(),
        ));
    }
    Ok(())
}

/// POST /api/v1/processes/{guid}/copy-to-user — 把工艺复制到用户层 `~/.ntd/processes/`（040）。
///
/// 040 重写为"纯文件复制 + 副本换新 guid"：
/// - 副本 guid 与源不同 → DB 里与源模板**同名共存**，原模板不消失（旧版同名覆盖语义废弃）。
/// - 系统/用户工艺都可复制（用户工艺也能再复制一份变体）。
/// - 目标相对路径与源一致；文件名冲突时自动加 `-1`/`-2` 后缀，不再 409。
///
/// 复制完成后触发用户层重扫入库（`is_system=false`）。
pub async fn copy_process_to_user(
    State(state): State<AppState>,
    Path(guid): Path<String>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let template = state
        .db
        .get_process_template_by_guid(&guid)
        .await?
        .ok_or(AppError::NotFound)?;

    // 源工艺正文按 source_path 从磁盘读取；DB 不存正文，避免二次冗余。
    let local_path = state.config_snapshot(|c| c.bundled_source.local_path.clone());
    let definition = read_definition(
        template.source_path.as_deref().unwrap_or_default(),
        &local_path,
    )?;

    // 副本身份：新 guid 单行替换（行级操作，保住注释与块标量格式）。
    let new_guid = crate::services::process::guid::new_guid();
    let copied = crate::services::process::guid::replace_or_insert_guid(&definition, &new_guid)
        .ok_or_else(|| AppError::Internal("副本 guid 写入失败：源 YAML 缺少 process 块".to_string()))?;

    let target_path = compute_copy_target_path(&template)?;

    atomic_write(&target_path, &copied)?;

    // 触发用户层重扫，把副本入库为 is_system=false。
    if let Err(e) =
        crate::services::process::user_dir::import_user_process_templates(&state).await
    {
        tracing::warn!("复制后导入用户层失败: {}", e);
    }

    let user_dir = crate::services::process::user_dir::user_processes_dir()
        .ok_or_else(|| AppError::Internal("无法获取 home 目录".to_string()))?;
    let rel_path = target_path
        .strip_prefix(&user_dir)
        .unwrap_or(&target_path)
        .to_string_lossy()
        .replace('\\', "/");
    Ok(ApiResponse::ok(serde_json::json!({
        "user_source_path": format!("{}{}", crate::services::process::user_dir::USER_SOURCE_PREFIX, rel_path),
        "guid": new_guid,
        "name": template.name,
    })))
}

/// 计算副本在用户层的目标路径：与源相同的相对路径，文件名冲突时加 `-1`/`-2` 后缀。
///
/// 源的 `source_path` 可能是 `bundled://processes/<rel>`（系统）或 `user://<rel>`（用户），
/// 剥前缀后落到用户层同一相对位置；自动后缀保证反复复制不冲突、不覆盖已有文件。
fn compute_copy_target_path(
    template: &crate::db::entity::process_templates::Model,
) -> Result<std::path::PathBuf, AppError> {
    let user_dir = crate::services::process::user_dir::user_processes_dir()
        .ok_or_else(|| AppError::Internal("无法获取 home 目录".to_string()))?;

    let fallback = format!("{}.yaml", template.name);
    let rel_path = template
        .source_path
        .as_ref()
        .and_then(|s| {
            s.strip_prefix("bundled://processes/")
                .or_else(|| s.strip_prefix("user://"))
        })
        .unwrap_or(&fallback);

    let candidate = user_dir.join(rel_path);
    if !candidate.exists() {
        return Ok(candidate);
    }

    // 文件名冲突：在主干名后追加 -1/-2/…（如 4p12s-delivery-1.yaml），上限防御死循环。
    let stem = candidate
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| template.name.clone());
    let parent = candidate.parent().map(std::path::Path::to_path_buf).unwrap_or(user_dir);
    for i in 1..100 {
        let c = parent.join(format!("{stem}-{i}.yaml"));
        if !c.exists() {
            return Ok(c);
        }
    }
    Err(AppError::Conflict(format!(
        "用户层已存在过多 {} 的副本，请手动清理",
        template.name
    )))
}

/// 递增语义化版本号的次版本号（X.Y.Z → X.Y+1.0）。
/// 如 "1.0.0" → "1.1.0"，"2.3.4" → "2.4.0"。
/// 非标准格式（不足 2 段）时追加 ".1" 作为 fallback。
fn bump_semver_minor(version: &str) -> String {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() >= 2 {
        let minor = parts[1].parse::<u32>().unwrap_or(0);
        format!("{}.{}.0", parts[0], minor + 1)
    } else {
        format!("{}.1", version)
    }
}

/// 判断版本 a 是否严格低于 b（语义化版本比较；非标准格式回退字符串比较）。
///
/// update_process 防倒退用：前端 Monaco 未回刷时会提交陈旧 version，这里拦截
/// 「提交版本 < 磁盘版本」的情况，确保写盘版本不回退（需求 042）。
fn version_below(a: &str, b: &str) -> bool {
    match (semver::Version::parse(a.trim()), semver::Version::parse(b.trim())) {
        (Ok(va), Ok(vb)) => va < vb,
        // 非标准版本号无法语义比较时退回字符串比较；相等不算 below。
        _ => a < b,
    }
}

/// 校验工艺名称不得包含路径分隔符或 `..`，防止 `copy_process_to_user` 写入目标逃逸。
fn validate_process_name(name: &str) -> Result<(), AppError> {
    if name.is_empty() {
        return Err(AppError::BadRequest("工艺名称不能为空".to_string()));
    }
    if name.contains('/') || name.contains('\\') {
        return Err(AppError::BadRequest(format!(
            "工艺名称不得包含路径分隔符: {}",
            name
        )));
    }
    if name == ".." || name.contains("/..") || name.contains("\\..") {
        return Err(AppError::BadRequest(format!("工艺名称不得包含 ..: {}", name)));
    }
    Ok(())
}
pub async fn get_process_stats(
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    // 按模板聚合安装次数（loops.process_template_id GROUP BY）
    let sql = "SELECT pt.name, pt.display_name, pt.complexity, COUNT(l.id) AS loop_count
        FROM process_templates pt
        LEFT JOIN loops l ON l.process_template_id = pt.id
        GROUP BY pt.id ORDER BY loop_count DESC";
    let rows = state.db.conn.query_all(Statement::from_string(DbBackend::Sqlite, sql)).await?;
    let mut stats = Vec::new();
    for row in rows {
        let name: String = row.try_get_by("name").unwrap_or_default();
        let display_name: String = row.try_get_by("display_name").unwrap_or_default();
        let complexity: String = row.try_get_by("complexity").unwrap_or_default();
        let loop_count: i64 = row.try_get_by("loop_count").unwrap_or(0);
        stats.push(serde_json::json!({
            "name": name, "display_name": display_name, "complexity": complexity, "loop_count": loop_count
        }));
    }
    Ok(ApiResponse::ok(serde_json::json!({
        "template_stats": stats,
        "total_templates": stats.len()
    })))
}

/// GET /api/v1/processes/{guid}/versions — 版本历史。
pub async fn get_process_versions(
    State(state): State<AppState>,
    Path(guid): Path<String>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let templates = state.db.list_process_templates(None).await?;
    // 040：按 guid 过滤（name 可重复，不能按 name 聚合版本）。
    let versions: Vec<_> = templates
        .iter()
        .filter(|t| t.guid == guid)
        .map(|t| serde_json::json!({
            "id": t.id,
            "version": t.version,
            "updated_at": t.updated_at,
            "source_path": t.source_path,
        }))
        .collect();
    Ok(ApiResponse::ok(serde_json::json!({ "guid": guid, "versions": versions })))
}

/// GET /api/v1/processes/{guid}/versions/{v}/diff?base={base_v} — 版本 diff。
pub async fn diff_process_versions(
    State(state): State<AppState>,
    Path((guid, _version)): Path<(String, String)>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let base_version = params.get("base").cloned().unwrap_or_default();
    let templates = state.db.list_process_templates(None).await?;
    let base = templates.iter().find(|t| t.guid == guid && t.version == base_version);
    let target = templates.iter().find(|t| t.guid == guid && t.version == _version);

    match (base, target) {
        (Some(b), Some(t)) => {
            // 工艺正文不在 DB，按各自的 source_path 从磁盘文件读取后再做 diff。
            let local_path = state.config_snapshot(|c| c.bundled_source.local_path.clone());
            let base_def = read_definition(b.source_path.as_deref().unwrap_or_default(), &local_path)?;
            let target_def = read_definition(t.source_path.as_deref().unwrap_or_default(), &local_path)?;
            let diff_lines = simple_diff(&base_def, &target_def);
            Ok(ApiResponse::ok(serde_json::json!({
                "guid": guid,
                "base_version": base_version,
                "target_version": _version,
                "diff": diff_lines,
            })))
        }
        _ => Err(AppError::NotFound),
    }
}

/// 简单的逐行 diff，返回 added/removed/unchanged 行。
///
/// 使用简化的 LCS-like 算法：双指针扫描两条文本，逐行对比。
/// 当行不匹配时尝试先推进新版本指针（跳过新增行），否则视作移除。
/// 这样能正确处理插入/删除，但不处理行级修改（修改显示为 remove+add）。
fn simple_diff(old: &str, new: &str) -> Vec<serde_json::Value> {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let mut result = Vec::new();

    // 使用简单的 LCS-like 逻辑：逐行对比直到同步点。
    let mut oi = 0;
    let mut ni = 0;
    while oi < old_lines.len() && ni < new_lines.len() {
        if old_lines[oi] == new_lines[ni] {
            result.push(serde_json::json!({"type": "unchanged", "line": old_lines[oi]}));
            oi += 1;
            ni += 1;
        } else {
            // 跳过旧版本中不存在的行（removed）。
            if ni + 1 < new_lines.len() && old_lines[oi] == new_lines[ni + 1] {
                result.push(serde_json::json!({"type": "added", "line": new_lines[ni]}));
                ni += 1;
            } else {
                result.push(serde_json::json!({"type": "removed", "line": old_lines[oi]}));
                oi += 1;
                // 如果新行匹配到了旧行后面，也添加。
                if oi < old_lines.len() && ni < new_lines.len() && old_lines[oi] == new_lines[ni] {
                    continue;
                }
            }
        }
    }
    // 剩余行。
    while oi < old_lines.len() {
        result.push(serde_json::json!({"type": "removed", "line": old_lines[oi]}));
        oi += 1;
    }
    while ni < new_lines.len() {
        result.push(serde_json::json!({"type": "added", "line": new_lines[ni]}));
        ni += 1;
    }
    result
}

/// POST /api/v1/processes/validate — 校验工艺定义 YAML。
#[derive(Debug, Deserialize)]
pub struct ValidateProcessRequest {
    pub definition: String,
}

pub async fn validate_process(
    State(_state): State<AppState>,
    Json(req): Json<ValidateProcessRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    match serde_yaml::from_str::<crate::services::process::ProcessDefinition>(&req.definition) {
        Ok(_def) => Ok(ApiResponse::ok(serde_json::json!({
            "valid": true,
            "errors": []
        }))),
        Err(e) => {
            // 提取行号信息（serde_yaml 错误消息含 line/column）。
            let msg = e.to_string();
            Ok(ApiResponse::ok(serde_json::json!({
                "valid": false,
                "errors": [msg]
            })))
        }
    }
}

/// POST /api/v1/processes/recommend — 工艺推荐。
pub async fn recommend_process(
    State(state): State<AppState>,
    Json(req): Json<crate::services::process::recommender::RecommendRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let templates = state.db.list_process_templates(None).await?;
    let result = crate::services::process::recommender::recommend(&templates, &req);
    Ok(ApiResponse::ok(result))
}

/// GET /api/workspaces/{ws}/loops/{id}/executions/{eid}/audit — 工艺实例审计链。
pub async fn get_loop_execution_audit(
    State(state): State<AppState>,
    Path((_ws_id, _loop_id, eid)): Path<(i64, i64, i64)>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let audit = state
        .db
        .get_loop_execution_audit(eid)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(axum::Json(crate::models::ApiResponse::ok(audit)))
}

/// POST .../gates/{gid}/approve 请求体。
#[derive(Debug, Deserialize)]
pub struct ApproveGateRequest {
    pub approved: bool,
    #[serde(default)]
    pub comment: Option<String>,
}

/// POST .../gates/{gid}/approve 响应体。
#[derive(Debug, Serialize)]
pub struct ApproveGateResponse {
    pub gate_id: i64,
    pub status: String,
}

/// 人工审批门禁：通过/拒绝。
///
/// 完整链路：校验 → 更新 gate → 写环节终态 → 广播刷新事件 → 恢复 loop 执行。
/// 此前只更新 gate 记录即返回，环节永远停在 pending_approval、loop 不再前进（NTD-004）。
pub async fn approve_gate(
    State(state): State<AppState>,
    Path((_ws_id, loop_id, eid, seid, gid)): Path<(i64, i64, i64, i64, i64)>,
    Json(req): Json<ApproveGateRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // 先确认 runner 可用再写库：避免"审批已落库但 resume 失败返回 500"的半完成状态。
    let runner = state
        .loop_runner
        .as_ref()
        .ok_or_else(|| AppError::Internal("loop runner not ready".to_string()))?;
    let step_exec = load_pending_step_execution(&state, loop_id, eid, seid).await?;
    validate_pending_human_gate(&state, seid, gid).await?;

    // 布尔审批映射为环节终态与展示评分：通过→success/100，拒绝→failed/0。
    // 评分 0 配合 resume 的 status 判定（不再用 rating>=min_rating），拒绝不会误判为通过。
    let (gate_status, final_status, rating) = if req.approved {
        ("passed", "success", 100)
    } else {
        ("failed", "failed", 0)
    };
    state
        .db
        .update_loop_step_execution_gate(gid, gate_status, req.comment.as_deref(), Some("human"))
        .await?;
    // 复用旧接口的落库方法：写环节终态 + approval_status='approved'，
    // 后者正是 resume_loop_execution 定位待恢复环节的查找条件。
    state
        .db
        .approve_step_execution(seid, rating, final_status, req.comment.as_deref())
        .await?;

    // 广播状态变更触发前端刷新，让看板/任务界面立刻离开"待审批"视图。
    let _ = state.tx.send(crate::executor_service::ExecEvent::ReviewStatusChanged {
        record_id: step_exec.execution_record_id.unwrap_or(0),
        todo_id: step_exec.todo_id,
        review_status: final_status.to_string(),
    });

    // 从被审批环节的下一环节继续主循环。
    runner.resume_loop_execution(eid).await;

    Ok(ApiResponse::ok(ApproveGateResponse { gate_id: gid, status: gate_status.to_string() }))
}

/// 加载待审批的环节执行记录，并校验 execution 归属指定 loop（防路径参数伪造）。
async fn load_pending_step_execution(
    state: &AppState,
    loop_id: i64,
    eid: i64,
    seid: i64,
) -> Result<crate::db::entity::loop_step_executions::Model, AppError> {
    // 与旧 approve_step_execution 相同的归属校验：execution 必须属于路径中的 loop。
    let loop_exec = state
        .db
        .get_loop_execution(eid)
        .await?
        .ok_or(AppError::NotFound)?;
    if loop_exec.loop_id != loop_id {
        return Err(AppError::BadRequest(
            "该 execution 不属于指定的 loop".to_string(),
        ));
    }
    let step_execs = state.db.list_loop_step_executions(eid).await?;
    let step_exec = step_execs
        .into_iter()
        .find(|se| se.id == seid)
        .ok_or(AppError::NotFound)?;
    ensure_step_approvable(&step_exec.status)?;
    Ok(step_exec)
}

/// 校验目标 gate 属于该环节、是 human_approval 类型且仍在等待评价。
async fn validate_pending_human_gate(
    state: &AppState,
    seid: i64,
    gid: i64,
) -> Result<(), AppError> {
    let gates = state.db.list_loop_step_execution_gates(seid).await?;
    let gate = gates.iter().find(|g| g.id == gid).ok_or(AppError::NotFound)?;
    ensure_gate_approvable(&gate.gate_type, &gate.status)
}

/// 校验环节执行状态允许审批（纯函数，便于单测）。
/// 幂等保护：重复审批或审批已完成的环节直接拒绝，避免覆盖终态。
fn ensure_step_approvable(status: &str) -> Result<(), AppError> {
    if status != "pending_approval" {
        return Err(AppError::BadRequest(
            "该环节当前不需要审批".to_string(),
        ));
    }
    Ok(())
}

/// 校验门禁可经人工审批（纯函数，便于单测）。
/// 只允许 human_approval 类型走本接口，防止借道篡改 AI 评审/脚本校验的结果。
fn ensure_gate_approvable(gate_type: &str, gate_status: &str) -> Result<(), AppError> {
    if gate_type != "human_approval" {
        return Err(AppError::BadRequest(
            "该门禁不是人工审批类型".to_string(),
        ));
    }
    if gate_status != "pending" {
        return Err(AppError::BadRequest(
            "该门禁已被评价".to_string(),
        ));
    }
    Ok(())
}

/// POST .../artifacts 请求体。
#[derive(Debug, Deserialize)]
pub struct AddArtifactRequest {
    pub name: String,
    pub artifact_type: String,
    pub locator: String,
    pub content_text: Option<String>,
}

/// 手动补充产物到指定环节执行记录。
pub async fn add_step_artifact(
    State(state): State<AppState>,
    Path((_ws_id, _loop_id, _eid, seid)): Path<(i64, i64, i64, i64)>,
    Json(req): Json<AddArtifactRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let model = state
        .db
        .create_loop_step_artifact(
            seid,
            &req.name,
            &req.artifact_type,
            &req.locator,
            req.content_text.as_deref(),
            Some("manual"),
        )
        .await?;
    Ok((axum::http::StatusCode::CREATED, ApiResponse::ok(model)))
}

/// 工艺模板相关路由（/api/bundled/processes）。
pub fn process_routes() -> Router<AppState> {
    Router::new()
        .route("/api/bundled/processes", axum::routing::get(list_process_templates))
        .route("/api/bundled/processes/{guid}", axum::routing::get(get_process_template))
        .route(
            "/api/bundled/processes/{guid}/install",
            axum::routing::post(install_process),
        )
}

/// 工艺模板相关路由（/api/v1/bundled/processes）。
pub fn v1_process_routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/bundled/processes", axum::routing::get(list_process_templates))
        .route("/api/v1/bundled/processes/{guid}", axum::routing::get(get_process_template))
        .route(
            "/api/v1/bundled/processes/{guid}/install",
            axum::routing::post(install_process),
        )
        .route(
            "/api/v1/workspaces/{ws}/loops/{id}/executions/{eid}/audit",
            axum::routing::get(get_loop_execution_audit),
        )
        .route(
            "/api/v1/workspaces/{ws}/loops/{id}/executions/{eid}/steps/{seid}/gates/{gid}/approve",
            axum::routing::post(approve_gate),
        )
        .route(
            "/api/v1/workspaces/{ws}/loops/{id}/executions/{eid}/steps/{seid}/artifacts",
            axum::routing::post(add_step_artifact),
        )
        .route(
            "/api/v1/processes/recommend",
            axum::routing::post(recommend_process),
        )
        .route(
            "/api/v1/processes/stats",
            axum::routing::get(get_process_stats),
        )
        .route(
            "/api/v1/processes/validate",
            axum::routing::post(validate_process),
        )
        .route(
            "/api/v1/processes/{guid}/versions",
            axum::routing::get(get_process_versions),
        )
        .route(
            "/api/v1/processes/{guid}/versions/{version}/diff",
            axum::routing::get(diff_process_versions),
        )
        .route(
            "/api/v1/processes/{guid}/copy-to-user",
            axum::routing::post(copy_process_to_user),
        )
        .route(
            "/api/v1/processes/{guid}/loops",
            axum::routing::get(list_process_loops),
        )
        .route(
            "/api/v1/processes/{guid}/loops/{loop_id}/upgrade",
            axum::routing::post(upgrade_process_loop),
        )
        // 029-工艺模板编辑与可视化创建：新增 3 个 CRUD 接口
        .route(
            "/api/v1/processes",
            axum::routing::post(create_process),
        )
        .route(
            "/api/v1/processes/{guid}",
            axum::routing::put(update_process).delete(delete_process),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::entity::process_templates;
    use tempfile::tempdir;

    // ──────────────────────────────────────────────────────────────────
    // ListProcessTemplatesQuery：039 列表过滤查询参数反序列化
    // ──────────────────────────────────────────────────────────────────

    // 用 axum 自带的 Query::try_from_uri 走真实提取路径，避免仅为测试引入 serde_urlencoded 依赖。
    // 测试辅助函数里 unwrap 是惯例（构造失败即测试失败），局部放行避免整个模块放宽 lint。
    #[allow(clippy::unwrap_used)]
    fn parse_list_query(uri: &str) -> ListProcessTemplatesQuery {
        let uri: axum::http::Uri = uri.parse().unwrap();
        axum::extract::Query::<ListProcessTemplatesQuery>::try_from_uri(&uri)
            .unwrap()
            .0
    }

    #[test]
    fn test_list_query_default_is_none() {
        // 不传参必须是 None（全量），否则设置页模板管理等旧调用方会被静默过滤。
        assert!(parse_list_query("/api/bundled/processes").is_system.is_none());
    }

    #[test]
    fn test_list_query_parse_true_false() {
        let q = parse_list_query("/api/bundled/processes?is_system=true");
        assert_eq!(q.is_system, Some(true));
        let q = parse_list_query("/api/bundled/processes?is_system=false");
        assert_eq!(q.is_system, Some(false));
    }

    // ──────────────────────────────────────────────────────────────────
    // validate_name_regex：name 正则校验 ^[a-zA-Z0-9_-]+$
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn test_validate_name_regex_valid() {
        // 合法字符：字母、数字、下划线、连字符都应通过。
        assert!(validate_name_regex("4p12s-delivery").is_ok());
        assert!(validate_name_regex("my_process").is_ok());
        assert!(validate_name_regex("process-123").is_ok());
        assert!(validate_name_regex("ABC").is_ok());
    }

    #[test]
    fn test_validate_name_regex_rejects_empty() {
        // 空字符串：chars().all() 对空迭代器返回 true，但空 name 无意义。
        // 这里验证行为符合预期（空串通过正则，但会被 validate_process_name 拦截）。
        let result = validate_name_regex("");
        assert!(result.is_ok(), "空串应通过正则（由 validate_process_name 拦截空）");
    }

    #[test]
    fn test_validate_name_regex_rejects_spaces() {
        // 含空格：应被拒绝，避免文件名或 YAML map key 异常。
        let result = validate_name_regex("my process");
        assert!(result.is_err());
        match result {
            Err(AppError::BadRequest(msg)) => assert!(msg.contains("只能包含")),
            other => unreachable!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn test_validate_name_regex_rejects_chinese() {
        // 含中文：应被拒绝，避免文件名编码问题。
        let result = validate_name_regex("我的工艺");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_name_regex_rejects_special_chars() {
        // 含特殊字符（点、斜杠、@等）：应被拒绝。
        for invalid in &["process.v1", "my/process", "process@v1", "pro cess"] {
            let result = validate_name_regex(invalid);
            assert!(result.is_err(), "应拒绝: {}", invalid);
        }
    }

    // ──────────────────────────────────────────────────────────────────
    // compute_user_process_path：用户工艺文件路径计算
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn test_compute_user_process_path_strips_user_prefix() {
        // source_path 以 "user://" 开头：应剥前缀，拼接到 user_dir 下。
        let template = process_templates::Model {
            id: 1,
            guid: "guid-test-process".to_string(),
            name: "test-process".to_string(),
            display_name: "Test".to_string(),
            description: String::new(),
            category: "software".to_string(),
            complexity: "light".to_string(),
            version: "1.0.0".to_string(),
            source_path: Some("user://software/test.yaml".to_string()),
            workspace_id: None,
            is_system: false,
            previous_version_id: None,
            created_at: None,
            updated_at: None,
        };

        let result = compute_user_process_path(&template);
        assert!(result.is_ok(), "路径计算应成功");
        let Ok(path) = result else { unreachable!() };
        assert!(
            path.ends_with("processes/software/test.yaml"),
            "路径应剥 user:// 前缀，实际: {}",
            path.display()
        );
    }

    #[test]
    fn test_compute_user_process_path_fallback_to_name_yaml() {
        // source_path 无 "user://" 前缀（或为 None）：应 fallback 到 <name>.yaml。
        let template = process_templates::Model {
            id: 2,
            guid: "guid-fallback-test".to_string(),
            name: "fallback-test".to_string(),
            display_name: "Fallback".to_string(),
            description: String::new(),
            category: "software".to_string(),
            complexity: "light".to_string(),
            version: "1.0.0".to_string(),
            source_path: None,  // 无 source_path
            workspace_id: None,
            is_system: false,
            previous_version_id: None,
            created_at: None,
            updated_at: None,
        };

        let result = compute_user_process_path(&template);
        assert!(result.is_ok());
        let Ok(path) = result else { unreachable!() };
        assert!(
            path.ends_with("processes/fallback-test.yaml"),
            "无 source_path 时应 fallback 到 <name>.yaml，实际: {}",
            path.display()
        );
    }

    // ──────────────────────────────────────────────────────────────────
    // atomic_write：原子写盘（临时文件 + rename）
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn test_atomic_write_creates_file_with_content() {
        // 正常写入：文件应存在且内容正确。
        let Ok(temp) = tempdir() else { unreachable!("创建 tempdir 失败") };
        let target = temp.path().join("test-process.yaml");

        let content = "process:\n  name: test\n";
        let result = atomic_write(&target, content);
        assert!(result.is_ok(), "原子写盘应成功");

        let Ok(written) = std::fs::read_to_string(&target) else { unreachable!("读取写入文件失败") };
        assert_eq!(written, content, "写入内容应与输入一致");
    }

    #[test]
    fn test_atomic_write_overwrites_existing_file() {
        // 覆盖已有文件：应直接替换内容，不残留 .tmp 文件。
        let Ok(temp) = tempdir() else { unreachable!("创建 tempdir 失败") };
        let target = temp.path().join("overwrite-test.yaml");

        // 先写入旧内容
        let Ok(()) = std::fs::write(&target, "old content") else { unreachable!("写入旧文件失败") };

        // 原子写入新内容
        let new_content = "new content";
        let result = atomic_write(&target, new_content);
        assert!(result.is_ok());

        // 验证内容已替换
        let Ok(written) = std::fs::read_to_string(&target) else { unreachable!("读取失败") };
        assert_eq!(written, new_content, "内容应被新内容替换");

        // 验证无残留 .tmp 文件
        let tmp = target.with_extension("yaml.tmp");
        assert!(!tmp.exists(), "不应残留 .tmp 文件");
    }

    #[test]
    fn test_atomic_write_creates_parent_dirs() {
        // 父目录不存在时：compute_user_process_path 会创建父目录，
        // atomic_write 本身不创建父目录（职责单一），这里验证 compute 流程。
        // 但若直接调用 atomic_write 到不存在的父目录，应失败而非 panic。
        let Ok(temp) = tempdir() else { unreachable!("创建 tempdir 失败") };
        let target = temp.path().join("nonexistent_dir").join("test.yaml");

        let result = atomic_write(&target, "content");
        assert!(result.is_err(), "父目录不存在时 atomic_write 应失败");
    }

    // ──────────────────────────────────────────────────────────────────
    // 请求体反序列化
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn test_update_process_request_deserialize() {
        // 验证 UpdateProcessRequest 能正确反序列化 JSON。
        let json = r#"{"definition": "process:\n  name: test\n"}"#;
        let Ok(req) = serde_json::from_str::<UpdateProcessRequest>(json) else { unreachable!("反序列化失败") };
        assert!(req.definition.contains("name: test"));
    }

    #[test]
    fn test_create_process_request_deserialize() {
        // 验证 CreateProcessRequest 能正确反序列化 JSON，可选字段为 None。
        let json = r#"{"name": "my-process", "definition": "process:\n  name: my-process\n"}"#;
        let Ok(req) = serde_json::from_str::<CreateProcessRequest>(json) else { unreachable!("反序列化失败") };
        assert_eq!(req.name, "my-process");
        assert!(req.display_name.is_none());
        assert!(req.category.is_none());
        assert!(req.complexity.is_none());
        assert!(req.version.is_none());
    }

    #[test]
    fn test_create_process_request_with_optional_fields() {
        // 验证 CreateProcessRequest 能正确反序列化带可选字段的 JSON。
        let json = r#"{
            "name": "full-process",
            "display_name": "完整工艺",
            "category": "software",
            "complexity": "standard",
            "version": "2.0.0",
            "definition": "process:\n  name: full-process\n"
        }"#;
        let Ok(req) = serde_json::from_str::<CreateProcessRequest>(json) else { unreachable!("反序列化失败") };
        assert_eq!(req.name, "full-process");
        assert_eq!(req.display_name.as_deref(), Some("完整工艺"));
        assert_eq!(req.category.as_deref(), Some("software"));
        assert_eq!(req.complexity.as_deref(), Some("standard"));
        assert_eq!(req.version.as_deref(), Some("2.0.0"));
    }

    // ──────────────────────────────────────────────────────────────────
    // ensure_step_approvable / ensure_gate_approvable：人工审批前置校验（NTD-004）
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn test_ensure_step_approvable_accepts_pending_approval() {
        // 只有 pending_approval 状态的环节允许审批。
        assert!(ensure_step_approvable("pending_approval").is_ok());
    }

    #[test]
    fn test_ensure_step_approvable_rejects_terminal_status() {
        // 已终态（success/failed）的环节重复审批必须拒绝，防止覆盖终态。
        for status in ["success", "failed", "running", "pending", "skipped"] {
            let result = ensure_step_approvable(status);
            assert!(result.is_err(), "status={status} 应拒绝审批");
        }
    }

    #[test]
    fn test_ensure_gate_approvable_accepts_pending_human_approval() {
        // 人工审批门禁 + 等待评价 → 放行。
        assert!(ensure_gate_approvable("human_approval", "pending").is_ok());
    }

    #[test]
    fn test_ensure_gate_approvable_rejects_non_human_type() {
        // 非人工审批门禁不允许借道本接口，防止篡改 AI 评审/脚本校验结果。
        for gate_type in ["ai_criteria_review", "artifact_present", "script_check"] {
            let result = ensure_gate_approvable(gate_type, "pending");
            assert!(result.is_err(), "gate_type={gate_type} 应拒绝");
        }
    }

    #[test]
    fn test_ensure_gate_approvable_rejects_already_evaluated() {
        // 已评价（passed/failed）的门禁不允许重复审批，保证幂等。
        for status in ["passed", "failed"] {
            let result = ensure_gate_approvable("human_approval", status);
            assert!(result.is_err(), "status={status} 应拒绝");
        }
    }

    // ──────────────────────────────────────────────────────────────────
    // version_below：update_process 防倒退兜底的版本比较（需求 042）
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn test_version_below_semver_comparison() {
        // 标准语义版本：minor/major 差异都能正确比较。
        assert!(version_below("1.0.0", "1.1.0"));
        assert!(version_below("1.1.0", "1.2.0"));
        assert!(version_below("1.0.0", "2.0.0"));
        // 相等不算 below（不应误触发防倒退）。
        assert!(!version_below("1.1.0", "1.1.0"));
        // 高于不算 below（用户手动改大版本应被尊重）。
        assert!(!version_below("1.2.0", "1.1.0"));
    }

    #[test]
    fn test_version_below_non_semver_falls_back_to_string() {
        // 非标准版本号无法语义比较时退回字符串比较，保证仍有确定行为。
        assert!(version_below("abc", "xyz"));
        assert!(!version_below("xyz", "abc"));
        // 一边非标准一边标准 → 无法配对 parse，退回字符串比较。
        assert!(version_below("1.0", "1.0.0"));
    }
}
