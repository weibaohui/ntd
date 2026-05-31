use crate::hooks::models::HookContext;

/// Template renderer for hook command arguments and environment variables.
/// Replaces {{variable}} placeholders with values from the context.
pub struct TemplateRenderer;

impl TemplateRenderer {
    /// Render a template string by replacing {{variable}} placeholders
    pub fn render(text: &str, ctx: &HookContext) -> String {
        let mut result = text.to_string();

        // Core context fields
        result = Self::replace(&result, "todo_id", ctx.todo_id.map(|id| id.to_string()));
        result = Self::replace(&result, "todo_title", Some(ctx.todo_title.clone()));
        result = Self::replace(&result, "old_status", ctx.old_status.clone());
        result = Self::replace(&result, "new_status", ctx.new_status.clone());
        result = Self::replace(&result, "executor", ctx.executor.clone());
        result = Self::replace(&result, "workspace", ctx.workspace.clone());
        result = Self::replace(&result, "task_id", ctx.task_id.clone());
        result = Self::replace(&result, "trigger_time", Some(ctx.trigger_time.clone()));
        result = Self::replace(&result, "trigger", Some(ctx.trigger.to_string()));

        result
    }

    /// Render command arguments
    pub fn render_args(args: &[String], ctx: &HookContext) -> Vec<String> {
        args.iter().map(|arg| Self::render(arg, ctx)).collect()
    }

    /// Render environment variables
    pub fn render_env(env: &std::collections::HashMap<String, String>, ctx: &HookContext) -> std::collections::HashMap<String, String> {
        env.iter()
            .map(|(k, v)| (k.clone(), Self::render(v, ctx)))
            .collect()
    }

    fn replace(text: &str, key: &str, value: Option<String>) -> String {
        match value {
            Some(v) => {
                let placeholder = format!("{{{{{}}}}}", key);
                text.replace(&placeholder, &v)
            }
            None => text.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::models::HookTrigger;

    fn test_context() -> HookContext {
        HookContext {
            todo_id: Some(123),
            todo_title: "Test Todo".to_string(),
            old_status: Some("pending".to_string()),
            new_status: Some("completed".to_string()),
            executor: Some("claude".to_string()),
            workspace: Some("/home/user/project".to_string()),
            task_id: Some("task_abc".to_string()),
            trigger_time: "2026-05-31T10:00:00.000Z".to_string(),
            trigger: HookTrigger::AfterStatusChange,
        }
    }

    #[test]
    fn test_render_simple_placeholder() {
        let ctx = test_context();
        let result = TemplateRenderer::render("todo_id={{todo_id}}", &ctx);
        assert_eq!(result, "todo_id=123");
    }

    #[test]
    fn test_render_multiple_placeholders() {
        let ctx = test_context();
        let result = TemplateRenderer::render(
            "[{{old_status}}] -> [{{new_status}}] {{todo_title}}",
            &ctx,
        );
        assert_eq!(result, "[pending] -> [completed] Test Todo");
    }

    #[test]
    fn test_render_preserves_unknown_placeholders() {
        let ctx = test_context();
        let result = TemplateRenderer::render("{{unknown}} {{todo_id}}", &ctx);
        assert_eq!(result, "{{unknown}} 123");
    }

    #[test]
    fn test_render_optional_field_missing() {
        let ctx = HookContext {
            todo_id: None,
            todo_title: "Test".to_string(),
            old_status: None,
            new_status: None,
            executor: None,
            workspace: None,
            task_id: None,
            trigger_time: "2026-05-31T10:00:00.000Z".to_string(),
            trigger: HookTrigger::BeforeCreate,
        };
        let result = TemplateRenderer::render("id={{todo_id}} task={{task_id}}", &ctx);
        assert_eq!(result, "id={{todo_id}} task={{task_id}}");
    }

    #[test]
    fn test_render_args() {
        let ctx = test_context();
        let args = vec![
            "{{todo_id}}".to_string(),
            "{{executor}}".to_string(),
            "static".to_string(),
        ];
        let rendered = TemplateRenderer::render_args(&args, &ctx);
        assert_eq!(rendered, vec!["123", "claude", "static"]);
    }

    #[test]
    fn test_render_env() {
        let ctx = test_context();
        let mut env = std::collections::HashMap::new();
        env.insert("TODO_TITLE".to_string(), "{{todo_title}}".to_string());
        env.insert("STATUS".to_string(), "{{old_status}} -> {{new_status}}".to_string());
        env.insert("STATIC".to_string(), "value".to_string());

        let rendered = TemplateRenderer::render_env(&env, &ctx);
        assert_eq!(rendered.get("TODO_TITLE"), Some(&"Test Todo".to_string()));
        assert_eq!(rendered.get("STATUS"), Some(&"pending -> completed".to_string()));
        assert_eq!(rendered.get("STATIC"), Some(&"value".to_string()));
    }
}
