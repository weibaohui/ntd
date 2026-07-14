//! 专家 API 路由处理函数
//!
//! 提供专家列表查询、详情查询、Agent MD 内容获取、头像资源访问、导入导出等接口。

use axum::extract::{Json, Multipart, Path as AxumPath, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::Router;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::write::FileOptions;
use zip::ZipArchive;

use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::ApiResponse;
use crate::expert::{
    ExpertMetadata, ExpertType, PluginJson, experts_dir, parse_plugin_json,
};
use crate::expert::loader::resolve_within;

/// `GET /api/experts`：获取所有专家列表
///
/// 返回所有已加载的专家元数据，按分类分组。前端可用于专家选择面板。
pub async fn get_experts(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<ExpertMetadata>>, AppError> {
    let experts = state.expert_manager.get_all_experts();
    Ok(ApiResponse::ok(experts))
}

/// `GET /api/experts/:name`：获取单个专家详情
///
/// 返回指定名称的专家完整元数据。
pub async fn get_expert(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
) -> Result<ApiResponse<ExpertMetadata>, AppError> {
    let expert = state
        .expert_manager
        .get_expert_by_name(&name)
        .ok_or(AppError::NotFound)?;
    Ok(ApiResponse::ok(expert))
}

/// `GET /api/experts/:name/agent-md`：获取专家的 Agent MD 内容
///
/// 根据专家类型自动定位：
/// - 单个专家：使用 agent_name 字段定位
/// - 专家团队：使用 lead_agent 字段定位
///
/// 返回完整的 MD 文件内容，用于执行时注入 prompt。
pub async fn get_expert_agent_md(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
) -> Result<ApiResponse<String>, AppError> {
    let expert = state
        .expert_manager
        .get_expert_by_name(&name)
        .ok_or(AppError::NotFound)?;

    // 根据专家类型确定要加载的 Agent：team 用 lead_agent、agent 用 agent_name
    // （统一走 resolve_agent_name，与执行注入路径保持一致）。
    let agent_name = expert.resolve_agent_name().ok_or(AppError::NotFound)?;

    let md_content = state
        .expert_manager
        .get_agent_md_content(&expert.name, agent_name)
        .map_err(|e| match e {
            crate::expert::ExpertError::AgentNotFound(_) => AppError::NotFound,
            _ => AppError::Internal("加载 Agent MD 内容失败".to_string()),
        })?;

    Ok(ApiResponse::ok(md_content))
}

/// `GET /api/experts/:name/skills`：获取专家关联的所有 Skill 元数据
///
/// 返回专家绑定的技能列表，用于前端展示可用技能。
pub async fn get_expert_skills(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
) -> Result<ApiResponse<Vec<crate::expert::SkillMetadata>>, AppError> {
    let _ = state
        .expert_manager
        .get_expert_by_name(&name)
        .ok_or(AppError::NotFound)?;

    let skills = state.expert_manager.get_expert_skills(&name);
    Ok(ApiResponse::ok(skills))
}

/// `GET /api/experts/:name/avatar`：获取专家头像
///
/// 根据专家的 avatar_path 字段定位头像文件并返回。
/// 如果头像不存在，返回 404。
pub async fn get_expert_avatar(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
) -> Result<impl IntoResponse, AppError> {
    let expert = state
        .expert_manager
        .get_expert_by_name(&name)
        .ok_or(AppError::NotFound)?;

    let avatar_rel = expert.avatar_path.as_deref().ok_or(AppError::NotFound)?;
    // 头像路径同样经 resolve_within 校验，防止 plugin.json 里 .. 越界读取。
    // resolve_within 返回 canonicalize 后的路径，且不存在时返回 None → 404。
    let full_path = resolve_within(
        std::path::Path::new(&expert.definition_dir),
        avatar_rel,
    )
    .ok_or(AppError::NotFound)?;

    let content = std::fs::read(&full_path).map_err(|e| AppError::Internal(format!("读取头像文件失败: {}", e)))?;

    // 根据文件扩展名推断 MIME 类型
    let ext = full_path.extension().and_then(|e| e.to_str()).unwrap_or("png");
    let mime_type = match ext.to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    };

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, mime_type)],
        content,
    ))
}

/// `DELETE /api/experts/:name`：删除专家
///
/// 删除指定专家：
/// 1. 从内存索引中移除专家及其所有关联数据（agent_files、skills 等）
/// 2. 删除磁盘上的专家定义目录（~/.ntd/experts/{name}/）
///
/// 删除后该专家不再出现在列表中，也无法被选择使用。
pub async fn delete_expert(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
) -> Result<ApiResponse<String>, AppError> {
    // 先从索引中移除，获取专家元数据（包含定义目录路径）
    let expert = state
        .expert_manager
        .remove_expert(&name)
        .ok_or(AppError::NotFound)?;

    // 删除磁盘上的专家目录
    let expert_dir = std::path::Path::new(&expert.definition_dir);
    if expert_dir.exists() {
        std::fs::remove_dir_all(expert_dir)
            .map_err(|e| AppError::Internal(format!("删除专家目录失败: {}", e)))?;
    }

    tracing::info!("专家已删除: name={}, dir={}", name, expert.definition_dir);
    Ok(ApiResponse::ok(format!("专家 \"{}\" 已删除", name)))
}

/// `GET /api/experts/:name/members/:member_id/avatar`：获取团队成员头像
///
/// 团队成员的头像路径存储在成员的 avatar_path 字段中（相对专家定义目录）。
/// 通过 expert_name 定位专家，再在 members 中按 member_id 查找对应成员，
/// 拼接 definition_dir + member.avatar_path 读取头像文件。
/// 这样避免前端直接传相对路径造成的越权风险，所有路径都经后端校验。
pub async fn get_expert_member_avatar(
    State(state): State<AppState>,
    AxumPath((name, member_id)): AxumPath<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    // 先按专家名取出专家元数据，拿不到说明专家不存在
    let expert = state
        .expert_manager
        .get_expert_by_name(&name)
        .ok_or(AppError::NotFound)?;

    // 在 members 中按 id 精确匹配成员，避免任意路径访问
    let member = expert
        .members
        .iter()
        .find(|m| m.id == member_id)
        .ok_or(AppError::NotFound)?;

    // 成员未配置头像字段时直接 404，让前端走兜底图标
    let avatar_rel = member.avatar_path.as_deref().ok_or(AppError::NotFound)?;

    // 同主头像校验：限制在 definition_dir 内，越界或不存在走 404
    let full_path = resolve_within(
        std::path::Path::new(&expert.definition_dir),
        avatar_rel,
    )
    .ok_or(AppError::NotFound)?;

    // 读取文件内容，失败统一转 Internal 错误
    let content = std::fs::read(&full_path)
        .map_err(|e| AppError::Internal(format!("读取成员头像文件失败: {}", e)))?;

    // 根据扩展名推断 MIME 类型
    let mime_type = infer_image_mime(&full_path);

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, mime_type)],
        content,
    ))
}

/// 根据文件扩展名推断图片 MIME 类型
///
/// 默认返回 `application/octet-stream`，保证未知扩展名时浏览器仍能兜底渲染。
fn infer_image_mime(path: &Path) -> &'static str {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png");
    match ext.to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

/// `POST /api/experts/create`：创建新专家
///
/// 根据传入的 plugin_json 和 agent_md 内容创建新专家。
/// 流程：
/// 1. 校验 plugin_json 必须包含 name 和 expertType 字段
/// 2. 创建专家目录结构：~/.ntd/experts/{name}/.codebuddy-plugin/plugin.json
/// 3. 创建 agents 目录并写入 agent.md 文件
/// 4. 重新加载专家索引，使新专家立即生效
///
/// 创建成功后，该专家会自动出现在列表中，可立即使用。
#[derive(Debug, Deserialize)]
pub struct CreateExpertRequest {
    /// plugin.json 内容（JSON 字符串）
    plugin_json: String,
    /// agent.md 内容（Markdown 字符串）
    agent_md: String,
}

pub async fn create_expert(
    State(state): State<AppState>,
    Json(req): Json<CreateExpertRequest>,
) -> Result<ApiResponse<String>, AppError> {
    // 解析 plugin_json 获取专家基本信息
    let plugin: PluginJson = serde_json::from_str(&req.plugin_json)
        .map_err(|e| AppError::BadRequest(format!("plugin_json 解析失败: {}", e)))?;

    // 保存专家名称供后续使用（避免 move 后无法访问）
    let expert_name = plugin.name.clone();

    // 校验必填字段
    if expert_name.is_empty() {
        return Err(AppError::BadRequest("专家名称不能为空".to_string()));
    }
    if !matches!(plugin.expert_type, ExpertType::Agent | ExpertType::Team) {
        return Err(AppError::BadRequest("expertType 必须为 agent 或 team".to_string()));
    }

    // 获取专家根目录
    let experts_dir = experts_dir()
        .ok_or_else(|| AppError::Internal("无法获取 home 目录".to_string()))?;
    std::fs::create_dir_all(&experts_dir)
        .map_err(|e| AppError::Internal(format!("创建专家目录失败: {}", e)))?;

    // 创建专家目录
    let expert_dir = experts_dir.join(&expert_name);
    if expert_dir.exists() {
        return Err(AppError::BadRequest(format!(
            "专家 \"{}\" 已存在",
            expert_name
        )));
    }
    std::fs::create_dir_all(&expert_dir)
        .map_err(|e| AppError::Internal(format!("创建专家目录失败: {}", e)))?;

    // 创建 .codebuddy-plugin 目录并写入 plugin.json
    let plugin_dir = expert_dir.join(".codebuddy-plugin");
    std::fs::create_dir_all(&plugin_dir)
        .map_err(|e| AppError::Internal(format!("创建 plugin 目录失败: {}", e)))?;
    let plugin_json_path = plugin_dir.join("plugin.json");
    std::fs::write(&plugin_json_path, &req.plugin_json)
        .map_err(|e| AppError::Internal(format!("写入 plugin.json 失败: {}", e)))?;

    // 创建 agents 目录并写入 agent.md
    if !req.agent_md.is_empty() {
        let agents_dir = expert_dir.join("agents");
        std::fs::create_dir_all(&agents_dir)
            .map_err(|e| AppError::Internal(format!("创建 agents 目录失败: {}", e)))?;

        // 从 plugin 中获取 agent_name，若无则用专家 name 兜底
        let agent_name = plugin.agent_name.clone().unwrap_or_else(|| expert_name.clone());
        let agent_md_path = agents_dir.join(format!("{}.md", agent_name));
        std::fs::write(&agent_md_path, &req.agent_md)
            .map_err(|e| AppError::Internal(format!("写入 agent.md 失败: {}", e)))?;

        // 如果 plugin 中没有 agents 字段，需要补全
        if plugin.agents.is_none() {
            let mut plugin_mut = plugin;
            plugin_mut.agents = Some(vec![format!("./agents/{}.md", agent_name)]);
            let updated_json = serde_json::to_string_pretty(&plugin_mut)
                .map_err(|e| AppError::Internal(format!("更新 plugin.json 失败: {}", e)))?;
            std::fs::write(&plugin_json_path, updated_json)
                .map_err(|e| AppError::Internal(format!("写入 plugin.json 失败: {}", e)))?;
        }
    }

    // 重新加载该专家到索引
    match state.expert_manager.reload_expert(&expert_dir) {
        Ok(_) => {
            tracing::info!("专家创建成功: name={}", expert_name);
            Ok(ApiResponse::ok(format!("专家 \"{}\" 创建成功", expert_name)))
        }
        Err(e) => {
            // 创建失败时清理已创建的目录
            let _ = std::fs::remove_dir_all(&expert_dir);
            Err(AppError::Internal(format!(
                "加载新专家失败: {}",
                e
            )))
        }
    }
}

/// `POST /api/experts/reload`：重新加载所有专家定义
///
/// 清空现有索引，重新扫描 ~/.ntd/experts/ 目录加载专家定义。
/// 返回加载结果（成功数量和错误列表）。
pub async fn reload_experts(
    State(state): State<AppState>,
) -> Result<ApiResponse<crate::expert::LoadResult>, AppError> {
    if let Some(experts_dir) = crate::expert::experts_dir() {
        if experts_dir.exists() {
            state.expert_manager.clear();
            let load_result = crate::expert::load_experts_from_directory(&experts_dir, &state.expert_manager);
            Ok(ApiResponse::ok(load_result))
        } else {
            Ok(ApiResponse::err(400, "专家定义目录不存在"))
        }
    } else {
        Ok(ApiResponse::err(400, "无法获取 home 目录"))
    }
}

// ── 导入导出相关类型 ──────────────────────────────────────────────────

/// 专家导入结果
#[derive(Debug, Serialize)]
pub struct ExpertImportResult {
    /// 导入的专家名称
    pub expert_name: String,
    /// 导入的专家元数据（成功时返回）
    pub expert: Option<ExpertMetadata>,
    /// 错误列表
    pub errors: Vec<String>,
}

/// 从目录导入请求
#[derive(Debug, Deserialize)]
pub struct ImportFromDirectoryRequest {
    /// 专家目录的绝对路径
    pub path: String,
}

/// 从 WorkBuddy 批量导入结果
#[derive(Debug, Serialize)]
pub struct WorkbuddyImportResult {
    /// 成功导入的专家数量
    pub imported_count: usize,
    /// 跳过的专家（已存在）数量
    pub skipped_count: usize,
    /// 成功导入的专家名称列表
    pub imported: Vec<String>,
    /// 跳过的专家名称列表
    pub skipped: Vec<String>,
    /// 错误列表
    pub errors: Vec<String>,
}

// ── 导出 API ──────────────────────────────────────────────────────────

/// `GET /api/experts/:name/export`：导出专家为 zip 文件
///
/// 将指定专家的整个目录打包为 zip 文件下载。
/// 流式传输，不一次性加载整个 zip 到内存。
pub async fn export_expert(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
) -> Result<impl IntoResponse, AppError> {
    // 查找专家，获取其定义目录
    let expert = state
        .expert_manager
        .get_expert_by_name(&name)
        .ok_or(AppError::NotFound)?;

    let expert_dir = PathBuf::from(&expert.definition_dir);

    // 校验目录存在
    if !expert_dir.exists() || !expert_dir.is_dir() {
        return Err(AppError::NotFound);
    }

    let expert_name = expert.name.clone();
    let expert_name_for_zip = expert_name.clone();

    // 在 spawn_blocking 中构建 zip，避免阻塞 tokio worker
    let zip_data = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, AppError> {
        build_expert_zip(&expert_dir, &expert_name_for_zip)
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking join error: {}", e)))??;

    // 设置响应头
    let filename = format!("{}.zip", expert_name);
    let disposition = format!("attachment; filename=\"{}\"", filename);
    let content_disposition = HeaderValue::from_str(&disposition)
        .map_err(|e| AppError::Internal(format!("构造 Content-Disposition 失败: {}", e)))?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("application/zip")),
            (header::CONTENT_DISPOSITION, content_disposition),
        ],
        zip_data,
    ))
}

/// 构建专家目录的 zip 归档
///
/// 递归遍历专家目录，将所有文件添加到 zip 中。
/// zip 内的文件路径以专家名称作为顶层目录。
fn build_expert_zip(expert_dir: &Path, expert_name: &str) -> Result<Vec<u8>, AppError> {
    let mut zip_data = Vec::new();
    {
        let mut zip_writer = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_data));
        let options = FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);

        add_dir_to_zip(&mut zip_writer, expert_dir, expert_name, &options)
            .map_err(|e| AppError::Internal(format!("创建 zip 归档失败: {}", e)))?;

        zip_writer
            .finish()
            .map_err(|e| AppError::Internal(format!("完成 zip 归档失败: {}", e)))?;
    }
    Ok(zip_data)
}

/// 递归将目录添加到 zip 归档
///
/// 遍历源目录的所有条目，文件直接写入，子目录递归处理。
fn add_dir_to_zip<W: std::io::Write + std::io::Seek>(
    zip_writer: &mut zip::ZipWriter<W>,
    dir: &Path,
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

// ── 导入 API ──────────────────────────────────────────────────────────

/// `POST /api/experts/import`：从 zip 文件导入专家
///
/// 接收 multipart/form-data 上传的 zip 文件，解压并导入为新专家。
/// 如果专家已存在则返回错误，不覆盖。
pub async fn import_expert(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<ApiResponse<ExpertImportResult>, AppError> {
    // 从 multipart 中提取 zip 文件数据
    let mut zip_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("解析 multipart 请求失败: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            zip_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("读取上传文件失败: {}", e)))?
                    .to_vec(),
            );
            break;
        }
    }

    let zip_bytes = zip_bytes.ok_or_else(|| AppError::BadRequest("未找到上传的文件字段 'file'".to_string()))?;

    let experts_dir_path = experts_dir().ok_or_else(|| AppError::Internal("无法获取 home 目录".to_string()))?;
    let expert_manager = state.expert_manager.clone();

    // 在 spawn_blocking 中执行解压和导入逻辑
    let result = tokio::task::spawn_blocking(move || -> Result<ExpertImportResult, AppError> {
        import_expert_from_zip_bytes(&zip_bytes, &experts_dir_path, &expert_manager)
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking join error: {}", e)))??;

    Ok(ApiResponse::ok(result))
}

/// 从 zip 字节数据导入专家
///
/// 解压到临时目录 → 校验结构 → 读取 name → 移动到目标目录 → 重新加载索引
fn import_expert_from_zip_bytes(
    zip_bytes: &[u8],
    experts_dir: &Path,
    expert_manager: &crate::expert::ExpertIndexManager,
) -> Result<ExpertImportResult, AppError> {
    // 确保专家根目录存在
    std::fs::create_dir_all(experts_dir)
        .map_err(|e| AppError::Internal(format!("创建专家目录失败: {}", e)))?;

    // 创建临时目录用于解压
    let temp_dir = tempfile::Builder::new()
        .prefix("ntd-expert-import-")
        .tempdir()
        .map_err(|e| AppError::Internal(format!("创建临时目录失败: {}", e)))?;

    // 解压 zip 到临时目录
    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|e| AppError::BadRequest(format!("无效的 zip 文件: {}", e)))?;

    extract_zip_to_dir(&mut archive, temp_dir.path())?;

    // 查找包含 .codebuddy-plugin/plugin.json 的目录
    let expert_src_dir = find_expert_root_dir(temp_dir.path())?;

    // 解析 plugin.json 获取专家名称
    let plugin_json_path = expert_src_dir.join(".codebuddy-plugin/plugin.json");
    let plugin = parse_plugin_json(&plugin_json_path)
        .map_err(|e| AppError::BadRequest(format!("解析 plugin.json 失败: {}", e)))?;

    let expert_name = plugin.name;

    // 校验专家名称安全性
    if !is_safe_expert_name(&expert_name) {
        return Err(AppError::BadRequest(format!("无效的专家名称: {}", expert_name)));
    }

    // 检查目标目录是否已存在
    let target_dir = experts_dir.join(&expert_name);
    if target_dir.exists() {
        return Err(AppError::BadRequest(format!("专家 '{}' 已存在，不能覆盖", expert_name)));
    }

    // 移动目录到目标位置
    move_directory(&expert_src_dir, &target_dir)
        .map_err(|e| AppError::Internal(format!("移动专家目录失败: {}", e)))?;

    // 重新加载该专家到索引
    let load_result = expert_manager.reload_expert(&target_dir);
    let mut errors = Vec::new();
    if let Err(e) = load_result {
        errors.push(format!("加载专家索引失败: {}", e));
    }

    // 获取导入后的专家元数据
    let expert = expert_manager.get_expert_by_name(&expert_name);

    Ok(ExpertImportResult {
        expert_name,
        expert,
        errors,
    })
}

/// 解压 zip 到指定目录
///
/// 包含路径遍历防护和解压炸弹保护。
fn extract_zip_to_dir<R: std::io::Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    target_dir: &Path,
) -> Result<(), AppError> {
    const MAX_FILE_SIZE: u64 = 100 * 1024 * 1024;
    const MAX_TOTAL_SIZE: u64 = 500 * 1024 * 1024;
    let mut total_extracted: u64 = 0;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| AppError::Internal(format!("读取 zip 条目失败: {}", e)))?;

        let outpath = file.mangled_name();

        // 拒绝绝对路径和包含父级引用的路径
        if outpath.is_absolute() || outpath.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err(AppError::BadRequest(format!(
                "zip 中包含不安全的路径: {}",
                outpath.display()
            )));
        }

        // mangled_name() 已剥离绝对路径与 .. 组件，上面的 is_absolute/ParentDir
        // 检查是第二重防护，足以挡住 zip 路径遍历。这里不再做 canonicalize+starts_with
        // 校验：它在文件创建前调用，路径尚不存在必然 canonicalize 失败而被跳过，
        // 属于无效的死代码。
        let dest_path = target_dir.join(&outpath);

        if file.is_dir() {
            std::fs::create_dir_all(&dest_path)
                .map_err(|e| AppError::Internal(format!("创建目录失败: {}", e)))?;
        } else {
            // 检查声明大小
            if file.size() > MAX_FILE_SIZE {
                return Err(AppError::BadRequest(format!(
                    "zip 中文件过大: {} ({} bytes)",
                    outpath.display(),
                    file.size()
                )));
            }

            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| AppError::Internal(format!("创建目录失败: {}", e)))?;
            }

            let mut outfile = std::fs::File::create(&dest_path)
                .map_err(|e| AppError::Internal(format!("创建文件失败: {}", e)))?;

            // 使用 take 限制单文件大小，防止解压炸弹
            let mut reader = file.by_ref().take(MAX_FILE_SIZE + 1);
            let written = std::io::copy(&mut reader, &mut outfile)?;
            if written > MAX_FILE_SIZE {
                std::fs::remove_file(&dest_path).ok();
                return Err(AppError::BadRequest(format!(
                    "解压时文件超过大小限制: {} ({} bytes)",
                    outpath.display(),
                    written
                )));
            }
            total_extracted += written;
            if total_extracted > MAX_TOTAL_SIZE {
                return Err(AppError::BadRequest(format!(
                    "解压总大小超过限制 ({} bytes)",
                    MAX_TOTAL_SIZE
                )));
            }
        }
    }

    Ok(())
}

/// 在解压后的目录中查找专家根目录
///
/// 递归查找包含 .codebuddy-plugin/plugin.json 的目录。
/// 如果根目录直接包含，则返回根目录；否则返回第一个找到的子目录。
fn find_expert_root_dir(base_dir: &Path) -> Result<PathBuf, AppError> {
    // 先检查根目录本身
    let plugin_json = base_dir.join(".codebuddy-plugin/plugin.json");
    if plugin_json.exists() && plugin_json.is_file() {
        return Ok(base_dir.to_path_buf());
    }

    // 遍历一级子目录
    let entries = std::fs::read_dir(base_dir)
        .map_err(|e| AppError::Internal(format!("读取目录失败: {}", e)))?;

    for entry in entries {
        let entry = entry.map_err(|e| AppError::Internal(format!("读取目录条目失败: {}", e)))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let plugin_json = path.join(".codebuddy-plugin/plugin.json");
        if plugin_json.exists() && plugin_json.is_file() {
            return Ok(path);
        }

        // 递归查找更深层级（zip 可能有多层包装）
        if let Ok(found) = find_expert_root_dir(&path) {
            return Ok(found);
        }
    }

    Err(AppError::BadRequest(
        "未找到有效的专家目录（缺少 .codebuddy-plugin/plugin.json）".to_string(),
    ))
}

/// 校验专家名称是否安全
///
/// 防止路径遍历攻击：不允许路径分隔符、父级引用、控制字符等。
fn is_safe_expert_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    // 不允许路径分隔符和父级引用
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return false;
    }
    // 不允许控制字符
    if name.chars().any(|c| c.is_control()) {
        return false;
    }
    true
}

/// 移动目录（跨文件系统时使用复制+删除）
///
/// 优先使用 rename（原子操作），如果跨设备失败则回退到复制+删除。
fn move_directory(src: &Path, dst: &Path) -> std::io::Result<()> {
    match std::fs::rename(src, dst) {
        Ok(_) => Ok(()),
        Err(e) => {
            // EXDEV 表示跨文件系统，回退到复制+删除
            if e.raw_os_error() == Some(18) {
                copy_dir_all(src, dst)?;
                std::fs::remove_dir_all(src)?;
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

/// 递归复制目录
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
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

// ── 从目录导入 API ──────────────────────────────────────────────────

/// `POST /api/experts/import-from-directory`：从本地目录导入专家
///
/// 接收 JSON body 指定的绝对路径，校验后复制到专家目录。
/// 如果专家已存在则返回错误，不覆盖。
pub async fn import_expert_from_directory(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<ImportFromDirectoryRequest>,
) -> Result<ApiResponse<ExpertImportResult>, AppError> {
    let src_dir = PathBuf::from(&req.path);

    // 校验源目录存在
    if !src_dir.exists() || !src_dir.is_dir() {
        return Err(AppError::BadRequest(format!(
            "源目录不存在或不是目录: {}",
            req.path
        )));
    }

    let experts_dir_path = experts_dir().ok_or_else(|| AppError::Internal("无法获取 home 目录".to_string()))?;
    let expert_manager = state.expert_manager.clone();

    // 在 spawn_blocking 中执行复制和导入逻辑
    let result = tokio::task::spawn_blocking(move || -> Result<ExpertImportResult, AppError> {
        import_expert_from_dir(&src_dir, &experts_dir_path, &expert_manager)
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking join error: {}", e)))??;

    Ok(ApiResponse::ok(result))
}

/// 从本地目录导入专家
///
/// 校验结构 → 读取 name → 复制到目标目录 → 重新加载索引
fn import_expert_from_dir(
    src_dir: &Path,
    experts_dir: &Path,
    expert_manager: &crate::expert::ExpertIndexManager,
) -> Result<ExpertImportResult, AppError> {
    // 校验源目录包含 plugin.json
    let plugin_json_path = src_dir.join(".codebuddy-plugin/plugin.json");
    if !plugin_json_path.exists() || !plugin_json_path.is_file() {
        return Err(AppError::BadRequest(
            "源目录中未找到 .codebuddy-plugin/plugin.json".to_string(),
        ));
    }

    // 解析 plugin.json 获取专家名称
    let plugin = parse_plugin_json(&plugin_json_path)
        .map_err(|e| AppError::BadRequest(format!("解析 plugin.json 失败: {}", e)))?;

    let expert_name = plugin.name;

    // 校验专家名称安全性
    if !is_safe_expert_name(&expert_name) {
        return Err(AppError::BadRequest(format!("无效的专家名称: {}", expert_name)));
    }

    // 确保专家根目录存在
    std::fs::create_dir_all(experts_dir)
        .map_err(|e| AppError::Internal(format!("创建专家目录失败: {}", e)))?;

    // 检查目标目录是否已存在
    let target_dir = experts_dir.join(&expert_name);
    if target_dir.exists() {
        return Err(AppError::BadRequest(format!("专家 '{}' 已存在，不能覆盖", expert_name)));
    }

    // 复制目录到目标位置
    copy_dir_all(src_dir, &target_dir)
        .map_err(|e| AppError::Internal(format!("复制专家目录失败: {}", e)))?;

    // 重新加载该专家到索引
    let load_result = expert_manager.reload_expert(&target_dir);
    let mut errors = Vec::new();
    if let Err(e) = load_result {
        errors.push(format!("加载专家索引失败: {}", e));
    }

    // 获取导入后的专家元数据
    let expert = expert_manager.get_expert_by_name(&expert_name);

    Ok(ExpertImportResult {
        expert_name,
        expert,
        errors,
    })
}

/// 专家 API 路由定义
pub fn expert_routes() -> axum::Router<AppState> {
    use axum::routing::{delete, get, post};

    Router::new()
        .route("/api/experts", get(get_experts))
        .route("/api/experts/create", post(create_expert))
        .route("/api/experts/{name}", get(get_expert))
        .route("/api/experts/{name}/agent-md", get(get_expert_agent_md))
        .route("/api/experts/{name}/skills", get(get_expert_skills))
        .route("/api/experts/{name}/avatar", get(get_expert_avatar))
        .route(
            "/api/experts/{name}/members/{member_id}/avatar",
            get(get_expert_member_avatar),
        )
        .route("/api/experts/{name}/export", get(export_expert))
        .route("/api/experts/{name}", delete(delete_expert))
        .route("/api/experts/reload", post(reload_experts))
        .route("/api/experts/import", post(import_expert))
        .route(
            "/api/experts/import-from-directory",
            post(import_expert_from_directory),
        )
        .route(
            "/api/experts/import-from-workbuddy",
            post(import_from_workbuddy),
        )
}

// ── 从 WorkBuddy 导入 API ──────────────────────────────────────────────

/// WorkBuddy 专家目录的默认相对路径
///
/// WorkBuddy 的专家存放在 ~/.workbuddy/plugins/marketplaces/experts/plugins/ 下，
/// 每个子目录代表一个专家或专家团队。
const WORKBUDDY_EXPERTS_RELATIVE_PATH: &str =
    ".workbuddy/plugins/marketplaces/experts/plugins";

/// `POST /api/experts/import-from-workbuddy`：从 WorkBuddy 目录批量导入专家
///
/// 扫描 WorkBuddy 默认目录下的所有专家子目录，逐个导入到 NTD 专家目录。
/// 已存在的专家会被跳过，不会覆盖。
pub async fn import_from_workbuddy(
    State(state): State<AppState>,
) -> Result<ApiResponse<WorkbuddyImportResult>, AppError> {
    // 定位 WorkBuddy 专家目录
    let home_dir = dirs::home_dir()
        .ok_or_else(|| AppError::Internal("无法获取 home 目录".to_string()))?;
    let workbuddy_dir = home_dir.join(WORKBUDDY_EXPERTS_RELATIVE_PATH);

    // 校验目录存在
    if !workbuddy_dir.exists() || !workbuddy_dir.is_dir() {
        return Err(AppError::BadRequest(format!(
            "WorkBuddy 专家目录不存在: {}",
            workbuddy_dir.display()
        )));
    }

    let experts_dir_path = experts_dir().ok_or_else(|| AppError::Internal("无法获取 home 目录".to_string()))?;
    let expert_manager = state.expert_manager.clone();

    // 在 spawn_blocking 中执行批量导入
    let result = tokio::task::spawn_blocking(move || -> WorkbuddyImportResult {
        batch_import_from_workbuddy(&workbuddy_dir, &experts_dir_path, &expert_manager)
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking join error: {}", e)))?;

    Ok(ApiResponse::ok(result))
}

/// 批量导入 WorkBuddy 专家目录
///
/// 遍历 WorkBuddy 目录下的所有子目录，逐个尝试导入。
/// 已存在的跳过，失败的记录到 errors。
///
/// # 参数
/// - `workbuddy_dir`: WorkBuddy 专家目录路径
/// - `experts_dir`: NTD 专家目标目录路径
/// - `expert_manager`: 专家索引管理器
///
/// # 返回
/// 批量导入结果
fn batch_import_from_workbuddy(
    workbuddy_dir: &Path,
    experts_dir: &Path,
    expert_manager: &crate::expert::ExpertIndexManager,
) -> WorkbuddyImportResult {
    let mut imported = Vec::new();
    let mut skipped = Vec::new();
    let mut errors = Vec::new();

    // 读取 WorkBuddy 目录下的所有子目录
    let entries = match std::fs::read_dir(workbuddy_dir) {
        Ok(e) => e,
        Err(e) => {
            errors.push(format!("无法读取 WorkBuddy 目录: {}", e));
            return WorkbuddyImportResult {
                imported_count: 0,
                skipped_count: 0,
                imported,
                skipped,
                errors,
            };
        }
    };

    // 确保专家根目录存在
    if let Err(e) = std::fs::create_dir_all(experts_dir) {
        errors.push(format!("创建专家目录失败: {}", e));
        return WorkbuddyImportResult {
            imported_count: 0,
            skipped_count: 0,
            imported,
            skipped,
            errors,
        };
    }

    for entry in entries.flatten() {
        let src_dir = entry.path();
        // 只处理目录
        if !src_dir.is_dir() {
            continue;
        }

        // 检查是否包含 plugin.json
        let plugin_json_path = src_dir.join(".codebuddy-plugin/plugin.json");
        if !plugin_json_path.exists() {
            continue;
        }

        // 解析 plugin.json 获取专家名称
        let plugin = match parse_plugin_json(&plugin_json_path) {
            Ok(p) => p,
            Err(e) => {
                // 解析失败的记录错误但继续处理其他专家
                errors.push(format!(
                    "解析 {} 失败: {}",
                    src_dir.display(),
                    e
                ));
                continue;
            }
        };

        let expert_name = plugin.name;

        // 校验名称安全性
        if !is_safe_expert_name(&expert_name) {
            errors.push(format!("无效的专家名称: {}", expert_name));
            continue;
        }

        // 检查是否已存在
        let target_dir = experts_dir.join(&expert_name);
        if target_dir.exists() {
            skipped.push(expert_name);
            continue;
        }

        // 复制目录到目标位置
        if let Err(e) = copy_dir_all(&src_dir, &target_dir) {
            errors.push(format!("复制 {} 失败: {}", expert_name, e));
            continue;
        }

        // 重新加载该专家到索引
        if let Err(e) = expert_manager.reload_expert(&target_dir) {
            errors.push(format!("加载 {} 索引失败: {}", expert_name, e));
            continue;
        }

        imported.push(expert_name);
    }

    let imported_count = imported.len();
    let skipped_count = skipped.len();

    WorkbuddyImportResult {
        imported_count,
        skipped_count,
        imported,
        skipped,
        errors,
    }
}
