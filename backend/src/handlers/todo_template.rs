use axum::{
    Router,
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json,
};
use serde::Serialize;
use std::path::Path as FsPath;

use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::{ApiResponse, CreateTemplateRequest, TodoTemplate, UpdateTemplateRequest};

pub async fn get_templates(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<TodoTemplate>>>, AppError> {
    let templates = state.db.get_templates().await?;
    Ok(Json(ApiResponse::ok(templates)))
}

pub async fn create_template(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<CreateTemplateRequest>,
) -> Result<Json<ApiResponse<TodoTemplate>>, AppError> {
    let title = req.title.trim();
    if title.is_empty() {
        return Err(AppError::BadRequest("Title is required".to_string()));
    }

    let category = req.category.trim();
    if category.is_empty() {
        return Err(AppError::BadRequest("Category is required".to_string()));
    }

    let id = state.db
        .create_template(crate::db::TemplateInput { title, prompt: req.prompt.as_deref(), category, sort_order: req.sort_order }, false)
        .await?;

    let template = state.db
        .get_template_by_id(id)
        .await?
        .ok_or_else(|| AppError::Internal("Failed to get created template".to_string()))?;

    Ok(Json(ApiResponse::ok(template)))
}

pub async fn update_template(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ApiJson(req): ApiJson<UpdateTemplateRequest>,
) -> Result<Json<ApiResponse<TodoTemplate>>, AppError> {
    // AppError::NotFound 是单元变体，不捕获变量——用 ok_or 直接构造更简洁
    let existing = state.db
        .get_template_by_id(id)
        .await?
        .ok_or(AppError::NotFound)?;

    // System templates cannot be modified
    if existing.is_system {
        return Err(AppError::BadRequest("Cannot modify system template".to_string()));
    }

    let title = req.title.unwrap_or_else(|| existing.title.clone());
    let prompt = req.prompt.or(existing.prompt);
    let category = req.category.unwrap_or_else(|| existing.category.clone());
    let sort_order = req.sort_order.or(Some(existing.sort_order));

    state.db
        .update_template(id, crate::db::TemplateInput { title: &title, prompt: prompt.as_deref(), category: &category, sort_order })
        .await?;

    let template = state.db
        .get_template_by_id(id)
        .await?
        .ok_or_else(|| AppError::Internal("Failed to get updated template".to_string()))?;

    Ok(Json(ApiResponse::ok(template)))
}

pub async fn delete_template(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    // AppError::NotFound 是单元变体，不捕获变量——用 ok_or 直接构造更简洁
    let existing = state.db
        .get_template_by_id(id)
        .await?
        .ok_or(AppError::NotFound)?;

    // System templates cannot be deleted
    if existing.is_system {
        return Err(AppError::BadRequest("Cannot delete system template".to_string()));
    }

    state.db.delete_template(id).await?;
    Ok(Json(ApiResponse::ok(())))
}

pub async fn copy_template(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<TodoTemplate>>, AppError> {
    // AppError::NotFound 是单元变体，不捕获变量——用 ok_or 直接构造更简洁
    let existing = state.db
        .get_template_by_id(id)
        .await?
        .ok_or(AppError::NotFound)?;

    // Create a copy as user template
    let new_title = format!("{} (副本)", existing.title);
    let id = state.db
        .create_template(crate::db::TemplateInput { title: &new_title, prompt: existing.prompt.as_deref(), category: &existing.category, sort_order: Some(existing.sort_order) }, false)
        .await?;

    let template = state.db
        .get_template_by_id(id)
        .await?
        .ok_or_else(|| AppError::Internal("Failed to get copied template".to_string()))?;

    Ok(Json(ApiResponse::ok(template)))
}

/// 导出响应：返回导出的 YAML 文件路径（~ 相对，不暴露家目录用户名）
#[derive(Debug, Serialize)]
pub struct TemplateExportResponse {
    pub path: String,
}

/// 导出的 YAML 结构：字段与远端 ntd-resource/todos/*.yaml 对齐（title/category/prompt/sort_order）
#[derive(Debug, Serialize)]
struct TemplateExportYaml<'a> {
    title: &'a str,
    category: &'a str,
    prompt: Option<&'a str>,
    sort_order: i32,
}

/// 把模板标题转成安全文件名：仅保留字母/数字/下划线/连字符（Unicode is_alphanumeric 已含中文），
/// 其余字符（含 / \\ : 等路径分隔符）统一替换为下划线——从根上杜绝 `..`/`/` 造成的路径越界；
/// 清理后为空时兜底返回 "template"，保证总能生成合法文件名。
fn sanitize_template_filename(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "template".to_string()
    } else {
        trimmed.to_string()
    }
}

/// 把模板序列化为 YAML 并写入导出目录，返回 ~ 相对路径。
///
/// 拆分原因：handler 只负责取数与守卫，文件落盘逻辑独立便于单测（注入临时目录）。
/// 导出目录由调用方传入（生产为 ~/.ntd/contribution-export/todos/），独立于 bundled 与用户目录，
/// 避免污染单向同步源；同名模板直接覆盖，提示词会引导 AI 提交最新文件。
fn write_template_yaml(template: &TodoTemplate, export_dir: &FsPath) -> Result<String, AppError> {
    let yaml = serde_yaml::to_string(&TemplateExportYaml {
        title: &template.title,
        category: &template.category,
        prompt: template.prompt.as_deref(),
        sort_order: template.sort_order,
    })
    .map_err(|e| AppError::Internal(format!("序列化模板 YAML 失败: {}", e)))?;

    std::fs::create_dir_all(export_dir)
        .map_err(|e| AppError::Internal(format!("创建导出目录失败: {}", e)))?;

    let file_name = format!("{}.yaml", sanitize_template_filename(&template.title));
    std::fs::write(export_dir.join(&file_name), &yaml)
        .map_err(|e| AppError::Internal(format!("写入导出文件失败: {}", e)))?;

    Ok(format!("~/.ntd/contribution-export/todos/{}", file_name))
}

/// `GET /api/v1/todo-templates/:id/export`：把用户事项模板导出为 YAML 文件（分享前置步骤）。
///
/// 模板存数据库、AI 执行器只能读文件，分享前必须先落盘；
/// 仅用户模板（is_system=false）可导出，系统模板由远端同步而来、read-only，不允许回传。
pub async fn export_template_yaml(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<TemplateExportResponse>>, AppError> {
    let template = state
        .db
        .get_template_by_id(id)
        .await?
        .ok_or(AppError::NotFound)?;

    // 系统模板不可导出：与 update/delete 的守卫一致，避免把同步来的模板原样提 PR 回远端
    if template.is_system {
        return Err(AppError::BadRequest("Cannot export system template".to_string()));
    }

    let home = dirs::home_dir().ok_or_else(|| AppError::Internal("无法获取 home 目录".to_string()))?;
    let export_dir = home.join(".ntd").join("contribution-export").join("todos");
    let path = write_template_yaml(&template, &export_dir)?;

    Ok(Json(ApiResponse::ok(TemplateExportResponse { path })))
}

/// API v1 routes for todo-template resource.
/// All paths are full absolute paths with /api/v1 prefix.
pub fn v1_routes() -> Router<AppState> {
    Router::new()
        // GET /api/v1/todo-templates — list all templates
        .route("/api/v1/todo-templates", get(get_templates))
        // POST /api/v1/todo-templates — create a new template
        .route("/api/v1/todo-templates", post(create_template))
        // PUT /api/v1/todo-templates/{id} — update a template
        .route("/api/v1/todo-templates/{id}", put(update_template))
        // DELETE /api/v1/todo-templates/{id} — delete a template
        .route("/api/v1/todo-templates/{id}", delete(delete_template))
        // POST /api/v1/todo-templates/{id}/copy — duplicate a template
        .route("/api/v1/todo-templates/{id}/copy", post(copy_template))
        // GET /api/v1/todo-templates/{id}/export — export user template as YAML (分享前置步骤)
        .route("/api/v1/todo-templates/{id}/export", get(export_template_yaml))
}
#[cfg(test)]
// 测试代码允许 expect（lint 为 warn，-D warnings 下会升级为 error；与项目其他测试模块一致用 allow 豁免）
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_template_filename_path_chars_replaced() {
        // 路径分隔符与冒号、空格等非法字符必须替换为下划线，杜绝目录逃逸
        // （输入 a/b\c: d → 每个非法字符一个下划线，冒号与空格相邻所以出现连续 __）
        assert_eq!(sanitize_template_filename("a/b\\c: d"), "a_b_c__d");
    }

    #[test]
    fn test_sanitize_template_filename_dot_dot_falls_back() {
        // 纯 .. 清理后为空，兜底返回 template，避免生成 .. 文件名
        assert_eq!(sanitize_template_filename(".."), "template");
    }

    #[test]
    fn test_sanitize_template_filename_keeps_cjk() {
        // 中文是 alphanumeric，应保留；空格与感叹号替换为下划线
        assert_eq!(sanitize_template_filename("模板 测试!"), "模板_测试");
    }

    #[test]
    fn test_write_template_yaml_creates_file_with_aligned_fields() {
        // 用临时目录注入，避免污染真实 ~/.ntd；断言文件生成且 YAML 字段与远端对齐
        let dir = std::env::temp_dir().join(format!("ntd-export-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let t = TodoTemplate {
            id: 1,
            title: "测试模板".to_string(),
            prompt: Some("多行\nprompt".to_string()),
            category: "测试".to_string(),
            sort_order: 3,
            is_system: false,
            source_url: None,
            last_sync_at: None,
            created_at: None,
            updated_at: None,
        };
        let path = write_template_yaml(&t, &dir).expect("导出应成功");
        // 返回 ~ 相对路径，供提示词引用
        assert!(path.starts_with("~/.ntd/contribution-export/todos/"));
        let content = std::fs::read_to_string(dir.join("测试模板.yaml")).expect("文件应存在");
        assert!(content.contains("title: 测试模板"));
        assert!(content.contains("category: 测试"));
        assert!(content.contains("sort_order: 3"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}