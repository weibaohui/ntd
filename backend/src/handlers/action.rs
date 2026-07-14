use axum::extract::State;
use serde::Deserialize;

use crate::handlers::{ApiJson, AppError, AppState};
use crate::models::ApiResponse;

/// Action 执行请求。
///
/// 前端传 action_type + action_key，后端查找或自动创建对应的 todo，
/// 然后用 prompt + params 执行。
#[derive(Debug, Deserialize)]
pub struct ExecuteActionRequest {
    /// 动作类型（如 "title_optimize"、"prompt_optimize"）
    pub action_type: String,
    /// 动作键值（如 "default"、"aggressive"）
    pub action_key: String,
    /// Prompt 模板（支持 {{key}} 占位符）
    pub prompt: String,
    /// 模板参数
    pub params: std::collections::HashMap<String, String>,
    /// 工作空间 ID（可选，不传则使用默认工作空间）
    pub workspace_id: Option<i64>,
    /// 执行器类型（可选，覆盖 todo 默认的 executor）
    pub executor: Option<String>,
}

/// Action 执行结果
#[derive(Debug, serde::Serialize)]
pub struct ExecuteActionResult {
    pub task_id: String,
    pub record_id: i64,
    pub todo_id: i64,
    /// todo 是否是本次自动创建的
    pub todo_created: bool,
}

/// POST /api/actions/execute
///
/// 根据 action_type + action_key 查找 todo，如果不存在则自动创建。
/// 然后用 prompt + params 执行该 todo。
pub async fn execute_action(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<ExecuteActionRequest>,
) -> Result<ApiResponse<ExecuteActionResult>, AppError> {
    // 参数校验：action_type、action_key、prompt 不能为空
    if req.action_type.trim().is_empty() {
        return Err(AppError::BadRequest("action_type 不能为空".to_string()));
    }
    if req.action_key.trim().is_empty() {
        return Err(AppError::BadRequest("action_key 不能为空".to_string()));
    }
    if req.prompt.trim().is_empty() {
        return Err(AppError::BadRequest("prompt 不能为空".to_string()));
    }

    // 1. 查找或创建 todo（同时拿到 workspace_id）
    let (todo_id, todo_created, workspace_id) = find_or_create_todo(&state, &req).await?;

    // 2. 构造 message：将 prompt 中的占位符替换为 params 中的值
    let message = replace_placeholders(&req.prompt, &req.params);

    // 3. 根据 workspace_id 查询对应的路径，传给执行器作为 cwd
    let workspace_path = if workspace_id > 0 {
        state
            .db
            .get_project_directory_by_id(workspace_id)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
            .map(|d| d.path)
    } else {
        None
    };

    // 4. 执行 todo，使用请求中指定的执行器（覆盖 todo 默认的 executor）
    let result = crate::handlers::execution::start_todo_execution(
        crate::executor_service::RunTodoExecutionRequest {
            db: state.db.clone(),
            executor_registry: state.executor_registry.clone(),
            tx: state.tx.clone(),
            task_manager: state.task_manager.clone(),
            config: state.config.clone(),
            todo_id,
            message,
            req_executor: req.executor.clone(), // 使用请求中指定的执行器
            trigger_type: "action".to_string(),
            params: Some(req.params.clone()),
            resume_session_id: None,
            resume_message: None,
            source_todo_id: None,
            source_todo_title: None,
            loop_step_execution_id: None,
            step_id: None,
            feishu_bot_id: None,
            feishu_receive_id: None,
            feishu_receive_id_type: None,
            workspace_path,
            workspace_id: Some(workspace_id),
            // action 触发路径：注入专家上下文，让 action todo 也能加载专家 prompt
            expert_manager: Some(state.expert_manager.clone()),
        },
    )
    .await?;

    let record_id = result
        .record_id
        .ok_or_else(|| AppError::Internal("执行启动失败：未获取到执行记录 ID".to_string()))?;

    Ok(ApiResponse::ok(ExecuteActionResult {
        task_id: result.task_id,
        record_id,
        todo_id,
        todo_created,
    }))
}

/// 查找或创建 action 模板 todo。
///
/// 按 action_type + action_key + workspace_id 查找，每个 workspace 独立拥有自己的 action todo。
/// 返回 (todo_id, todo_created, workspace_id)。
async fn find_or_create_todo(
    state: &AppState,
    req: &ExecuteActionRequest,
) -> Result<(i64, bool, i64), AppError> {
    // 动态确定 workspace_id：优先使用请求中的，否则取第一个可用的工作空间
    let workspace_id = match req.workspace_id {
        Some(id) => id,
        None => {
            let dirs = state
                .db
                .get_project_directories()
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
            dirs.first()
                .map(|d| d.id)
                .ok_or_else(|| AppError::BadRequest("没有可用的工作空间".to_string()))?
        }
    };

    // 1. 按 action_type + action_key + workspace_id 查找已有的 todo
    // 每个 workspace 独立拥有自己的 action todo，不会跨 workspace 复用
    if let Some(todo) = state
        .db
        .get_todo_by_action_type_and_key_and_workspace(&req.action_type, &req.action_key, workspace_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    {
        return Ok((todo.id, false, workspace_id));
    }

    // 2. 未找到，创建新的 action todo
    let dir = state
        .db
        .get_project_directory_by_id(workspace_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::BadRequest(format!("工作空间 {} 不存在", workspace_id)))?;

    let title = format!("Action: {}/{}", req.action_type, req.action_key);

    let todo_id = state
        .db
        .create_todo_with_extras(
            &title,
            &req.prompt,
            None,    // executor: 使用默认
            None,    // acceptance_criteria
            false,   // webhook_enabled
            workspace_id,
            &dir.path,
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // 更新 action_type 和 action_key
    state
        .db
        .update_todo_full(crate::db::TodoUpdate {
            id: todo_id,
            title: &title,
            prompt: &req.prompt,
            status: crate::models::TodoStatus::Pending,
            executor: None,
            expert_name: None,
            scheduler_enabled: None,
            scheduler_config: None,
            scheduler_timezone: None,
            workspace_id: None,
            webhook_enabled: None,
            acceptance_criteria: None,
            auto_review_enabled: None,
            action_type: Some(&req.action_type),
            action_key: Some(&req.action_key),
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok((todo_id, true, workspace_id))
}

/// 将 prompt 模板中的占位符替换为 params 中的值。
fn replace_placeholders(
    prompt: &str,
    params: &std::collections::HashMap<String, String>,
) -> String {
    let mut result = prompt.to_string();
    for (key, value) in params {
        let placeholder = format!("{{{{{}}}}}", key);
        result = result.replace(&placeholder, value);
    }
    result
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_placeholders() {
        let prompt = "优化标题：{{title}}，参考：{{prompt}}";
        let mut params = std::collections::HashMap::new();
        params.insert("title".to_string(), "fix bug".to_string());
        params.insert("prompt".to_string(), "帮我修复登录超时".to_string());

        let result = replace_placeholders(prompt, &params);
        assert_eq!(result, "优化标题：fix bug，参考：帮我修复登录超时");
    }

    #[test]
    fn test_replace_placeholders_no_match() {
        let prompt = "优化标题：{{title}}";
        let params = std::collections::HashMap::new();

        let result = replace_placeholders(prompt, &params);
        assert_eq!(result, "优化标题：{{title}}");
    }
}
