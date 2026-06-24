use std::io::Read;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::{
    ClientResponse, CreateTagRequest, CreateTodoRequest, DashboardStats, ExecutionRecord,
    ExecutionRecordsPage, ExecutionSummary, Tag, Todo, ExecuteRequest,
};
use crate::cli::client::ApiClient;
use crate::config;

#[derive(Parser, Debug)]
#[command(name = "ntd")]
#[command(about = "AI Todo CLI - Manage AI-powered tasks", long_about = None)]
pub struct Cli {
    /// API server URL (default: from ~/.ntd/config.yaml, or http://localhost:8088)
    #[arg(long)]
    pub server: Option<String>,

    /// Output format
    #[arg(short, long, default_value = "json", value_enum)]
    pub output: OutputFormat,

    /// Select fields to output (comma-separated, e.g. "id,title,status")
    /// Only effective with --output raw
    #[arg(short, long)]
    pub fields: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Json,
    Pretty,
    /// Output raw data without ApiResponse wrapper (best for AI parsing)
    Raw,
}

// ============== CLI Commands ==============

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Todo management
    Todo {
        #[command(subcommand)]
        action: TodoAction,
    },
    /// Tag management
    Tag {
        #[command(subcommand)]
        action: TagAction,
    },
    /// Global statistics
    Stats,
}

#[derive(Debug, Clone, Subcommand)]
pub enum TodoAction {
    /// Create a new todo
    Create {
        /// Todo title (optional if --stdin is used)
        title: Option<String>,

        /// Prompt content (use --file to load from file)
        #[arg(short, long)]
        prompt: Option<String>,

        /// Read prompt from file
        #[arg(short, long)]
        file: Option<String>,

        /// Read todo data from stdin as JSON
        #[arg(long)]
        stdin: bool,

        /// Executor type (claudecode, mobilecoder, codebuddy, opencode, atomcode, hermes, kimi, codex, codewhale, zhanlu)
        #[arg(short, long)]
        executor: Option<String>,

        /// Working directory
        #[arg(short, long)]
        workspace: Option<String>,

        /// Tag IDs (comma-separated)
        #[arg(long)]
        tags: Option<String>,

        /// Schedule (cron expression, e.g. "*/30 * * * *")
        #[arg(long)]
        schedule: Option<String>,
    },
    /// List todos
    List {
        /// Filter by status
        #[arg(long)]
        status: Option<String>,

        /// Filter by tag ID
        #[arg(long)]
        tag: Option<i64>,

        /// Show only running todos
        #[arg(long)]
        running: bool,

        /// Search by keyword in title or prompt
        #[arg(short, long)]
        search: Option<String>,
    },
    /// Get todo details
    Get {
        /// Todo ID
        id: i64,
    },
    /// Update todo
    Update {
        /// Todo ID
        id: i64,

        /// New title
        #[arg(long)]
        title: Option<String>,

        /// New prompt (use --file to load from file)
        #[arg(long)]
        prompt: Option<String>,

        /// Read prompt from file
        #[arg(short, long)]
        file: Option<String>,

        /// Read update data from stdin as JSON
        #[arg(long)]
        stdin: bool,

        /// New status
        #[arg(long)]
        status: Option<String>,

        /// New executor type
        #[arg(long)]
        executor: Option<String>,

        /// New working directory
        #[arg(long)]
        workspace: Option<String>,

        /// New tag IDs (comma-separated)
        #[arg(long)]
        tags: Option<String>,

        /// Schedule (cron expression)
        #[arg(long)]
        schedule: Option<String>,
    },
    /// Delete todo
    Delete {
        /// Todo ID
        id: i64,
    },
    /// Execute todo
    Execute {
        /// Todo ID
        id: i64,

        /// Additional message
        #[arg(short, long)]
        message: Option<String>,

        /// Override executor
        #[arg(long)]
        executor: Option<String>,

        /// Parameters for placeholder replacement (key=value format, can be repeated)
        /// Example: --param project_name=myproject --param env=production
        #[arg(long = "param", num_args = 1, value_parser = parse_key_value)]
        params: Option<Vec<(String, String)>>,
    },
    /// Stop todo execution
    Stop {
        /// Todo ID
        id: i64,
    },
    /// Get todo execution stats
    Stats {
        /// Todo ID
        id: i64,
    },
    /// Execution records
    Execution {
        #[command(subcommand)]
        action: ExecutionAction,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum ExecutionAction {
    /// List execution records for a todo
    List {
        /// Todo ID
        todo_id: i64,

        /// Filter by status
        #[arg(long)]
        status: Option<String>,

        /// Page number
        #[arg(long, default_value = "1")]
        page: i64,

        /// Items per page
        #[arg(long, default_value = "20")]
        limit: i64,
    },
    /// Get execution record details
    Get {
        /// Execution record ID
        id: i64,
    },
    /// Resume a conversation from an execution record
    Resume {
        /// Execution record ID
        id: i64,

        /// Optional message to send (defaults to todo prompt)
        #[arg(short, long)]
        message: Option<String>,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum TagAction {
    /// List all tags
    List,
    /// Create a new tag
    Create {
        /// Tag name
        name: String,

        /// Tag color (hex)
        #[arg(short, long, default_value = "#1890ff")]
        color: String,
    },
    /// Delete a tag
    Delete {
        /// Tag ID
        id: i64,
    },
}

// ============== Helper Functions ==============

fn read_prompt_from_file(file: &Option<String>) -> Result<String> {
    match file {
        Some(path) => Ok(std::fs::read_to_string(path)?),
        None => Ok(String::new()),
    }
}

fn parse_tags(tags: &Option<String>) -> Vec<i64> {
    match tags {
        Some(s) => s.split(',').filter_map(|s| s.trim().parse().ok()).collect(),
        None => Vec::new(),
    }
}

fn parse_key_value(s: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid key=value format: {}", s));
    }
    Ok((parts[0].trim().to_string(), parts[1].trim().to_string()))
}

fn read_stdin_json() -> Result<Value> {
    let mut buffer = String::new();
    std::io::stdin().read_to_string(&mut buffer)?;
    let value: Value = serde_json::from_str(&buffer)
        .map_err(|e| anyhow::anyhow!("Invalid JSON from stdin: {}", e))?;
    Ok(value)
}

fn parse_fields(fields: &Option<String>) -> Option<Vec<String>> {
    fields.as_ref().map(|s| {
        s.split(',').map(|f| f.trim().to_string()).filter(|f| !f.is_empty()).collect()
    })
}

fn filter_fields(value: &Value, fields: &[String]) -> Value {
    match value {
        Value::Object(map) => {
            let mut filtered = serde_json::Map::new();
            for field in fields {
                if let Some(v) = map.get(field) {
                    filtered.insert(field.clone(), v.clone());
                }
            }
            Value::Object(filtered)
        }
        _ => value.clone(),
    }
}

fn filter_array_fields(arr: &[Value], fields: &[String]) -> Vec<Value> {
    arr.iter().map(|v| filter_fields(v, fields)).collect()
}

// ============== Main Entry Point ==============

pub async fn run_command(cli: &Cli) -> Result<()> {
    let server_url = cli.server.clone().unwrap_or_else(|| config::Config::load().server_url());
    let client = ApiClient::new(&server_url);

    match &cli.command {
        Commands::Todo { action } => handle_todo(&client, action, &cli.output, &cli.fields).await?,
        Commands::Tag { action } => handle_tag(&client, action, &cli.output, &cli.fields).await?,
        Commands::Stats => handle_stats(&client, &cli.output, &cli.fields).await?,
    }

    Ok(())
}

// ============== Todo Handlers ==============

async fn handle_todo(
    client: &ApiClient,
    action: &TodoAction,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    match action {
        TodoAction::Create { title, prompt, file, stdin, executor, workspace, tags, schedule } => {
            let mut req = if *stdin {
                // Read from stdin
                let value = read_stdin_json()?;
                let req = serde_json::from_value::<CreateTodoRequest>(value.clone())
                    .unwrap_or_else(|_| CreateTodoRequest {
                        title: value.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        prompt: value.get("prompt").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        tag_ids: value.get("tag_ids")
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
                            .unwrap_or_default(),
                        executor: value.get("executor").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        scheduler_enabled: None,
                        scheduler_config: None,
                        scheduler_timezone: None,
                        acceptance_criteria: value.get("acceptance_criteria").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        auto_review_enabled: value.get("auto_review_enabled").and_then(|v| v.as_bool()),
                    });
                if workspace.is_some() {
                    // workspace is sent separately in the full JSON body
                }
                req
            } else {
                let title = title.clone().ok_or_else(|| anyhow::anyhow!("Title is required. Use --stdin to read from stdin."))?;
                let prompt_content = if let Some(p) = prompt {
                    p.clone()
                } else {
                    read_prompt_from_file(file)?
                };

                CreateTodoRequest {
                    title,
                    prompt: prompt_content,
                    tag_ids: parse_tags(tags),
                    executor: executor.clone(),
                    scheduler_enabled: None,
                    scheduler_config: None,
                    scheduler_timezone: None,
                    acceptance_criteria: None,
                    auto_review_enabled: None,
                }
            };

            // Set scheduler options from CLI args
            if let Some(s) = schedule {
                if !s.is_empty() {
                    req.scheduler_enabled = Some(true);
                    req.scheduler_config = Some(s.clone());
                }
            }

            let resp: ClientResponse<Todo> = client.post("/todos", &req).await?;
            print_response(resp, output, fields)?;
        }
        TodoAction::List { status, tag, running, search } => {
            let mut query_params = Vec::new();

            if let Some(s) = status {
                query_params.push(format!("status={}", s));
            }
            if let Some(t) = tag {
                query_params.push(format!("tag_id={}", t));
            }
            if *running {
                query_params.push("running=true".to_string());
            }

            let path = if query_params.is_empty() {
                "/todos".to_string()
            } else {
                format!("/todos?{}", query_params.join("&"))
            };

            let resp: ClientResponse<Vec<Todo>> = client.get(&path).await?;

            // Client-side search filtering
            let resp = if let Some(keyword) = search {
                let keyword = keyword.to_lowercase();
                match resp.data {
                    Some(todos) => {
                        let filtered: Vec<Todo> = todos.into_iter()
                            .filter(|t| {
                                t.title.to_lowercase().contains(&keyword)
                                    || t.prompt.to_lowercase().contains(&keyword)
                            })
                            .collect();
                        ClientResponse { code: resp.code, data: Some(filtered), message: resp.message }
                    }
                    None => resp,
                }
            } else {
                resp
            };

            print_response(resp, output, fields)?;
        }
        TodoAction::Get { id } => {
            let resp: ClientResponse<Todo> = client.get(&format!("/todos/{}", id)).await?;
            print_response(resp, output, fields)?;
        }
        TodoAction::Update { id, title, prompt, file, stdin, status, executor, workspace, tags, schedule } => {
            let mut req = if *stdin {
                read_stdin_json()?
            } else {
                let prompt_content = if let Some(path) = file {
                    read_prompt_from_file(&Some(path.clone()))?
                } else {
                    prompt.clone().unwrap_or_default()
                };
                serde_json::json!({
                    "title": title,
                    "prompt": prompt_content,
                    "status": status,
                    "executor": executor,
                    "workspace": workspace,
                })
            };

            // Merge CLI args over stdin values
            if let Some(t) = title { req["title"] = t.clone().into(); }
            if let Some(p) = prompt { req["prompt"] = p.clone().into(); }
            if let Some(s) = status { req["status"] = s.clone().into(); }
            if let Some(e) = executor { req["executor"] = e.clone().into(); }
            if let Some(w) = workspace { req["workspace"] = w.clone().into(); }
            if let Some(t) = tags {
                let tag_ids: Vec<i64> = t.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                req["tag_ids"] = tag_ids.into();
            }
            if let Some(s) = schedule {
                req["scheduler_enabled"] = (!s.is_empty()).into();
                req["scheduler_config"] = if s.is_empty() { Value::Null } else { s.clone().into() };
            }

            let resp: ClientResponse<Todo> = client.put(&format!("/todos/{}", id), &req).await?;
            print_response(resp, output, fields)?;
        }
        TodoAction::Delete { id } => {
            let resp: ClientResponse<()> = client.delete(&format!("/todos/{}", id)).await?;
            print_response(resp, output, fields)?;
        }
        TodoAction::Execute { id, message, executor, params } => {
            let params: Option<std::collections::HashMap<String, String>> = params.as_ref().map(|vec| {
                vec.iter().cloned().collect()
            });
            let req = ExecuteRequest {
                todo_id: *id,
                message: message.clone(),
                executor: executor.clone(),
                params,
            };
            let resp: ClientResponse<Value> = client.post("/execute", &req).await?;
            print_response(resp, output, fields)?;
        }
        TodoAction::Stop { id } => {
            let req = serde_json::json!({ "todo_id": id });
            let resp: ClientResponse<()> = client.post("/execute/stop", &req).await?;
            print_response(resp, output, fields)?;
        }
        TodoAction::Stats { id } => {
            let resp: ClientResponse<ExecutionSummary> = client.get(&format!("/todos/{}/summary", id)).await?;
            print_response(resp, output, fields)?;
        }
        TodoAction::Execution { action } => {
            handle_execution(client, action, output, fields).await?;
        }
    }
    Ok(())
}

async fn handle_execution(
    client: &ApiClient,
    action: &ExecutionAction,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    match action {
        ExecutionAction::List { todo_id, status, page, limit } => {
            let path = format!(
                "/execution-records?todo_id={}&page={}&limit={}{}",
                todo_id,
                page,
                limit,
                status.as_ref().map(|s| format!("&status={}", s)).unwrap_or_default()
            );
            let resp: ClientResponse<ExecutionRecordsPage> = client.get(&path).await?;
            print_response(resp, output, fields)?;
        }
        ExecutionAction::Get { id } => {
            let resp: ClientResponse<ExecutionRecord> = client.get(&format!("/execution-records/{}", id)).await?;
            print_response(resp, output, fields)?;
        }
        ExecutionAction::Resume { id, message } => {
            let req = serde_json::json!({ "message": message });
            let resp: ClientResponse<Value> = client.post(&format!("/execution-records/{}/resume", id), &req).await?;
            print_response(resp, output, fields)?;
        }
    }
    Ok(())
}

// ============== Tag Handlers ==============

async fn handle_tag(
    client: &ApiClient,
    action: &TagAction,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    match action {
        TagAction::List => {
            let resp: ClientResponse<Vec<Tag>> = client.get("/tags").await?;
            print_response(resp, output, fields)?;
        }
        TagAction::Create { name, color } => {
            let req = CreateTagRequest {
                name: name.clone(),
                color: color.clone(),
            };
            let resp: ClientResponse<Tag> = client.post("/tags", &req).await?;
            print_response(resp, output, fields)?;
        }
        TagAction::Delete { id } => {
            let resp: ClientResponse<()> = client.delete(&format!("/tags/{}", id)).await?;
            print_response(resp, output, fields)?;
        }
    }
    Ok(())
}

// ============== Stats Handler ==============

async fn handle_stats(
    client: &ApiClient,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    let resp: ClientResponse<DashboardStats> = client.get("/dashboard-stats").await?;
    print_response(resp, output, fields)?;
    Ok(())
}

// ============== Output ==============

fn print_response<T: serde::Serialize>(
    resp: ClientResponse<T>,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    if resp.code != 0 {
        // Let the caller handle structured error printing
        return Err(anyhow::anyhow!("API error {}: {}", resp.code, resp.message));
    }

    let field_list = parse_fields(fields);

    match output {
        OutputFormat::Json => {
            let value = serde_json::to_value(&resp)?;
            println!("{}", serde_json::to_string(&value)?);
        }
        OutputFormat::Pretty => {
            let value = serde_json::to_value(&resp)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        OutputFormat::Raw => {
            let mut value = serde_json::to_value(&resp.data)?;
            if let Some(ref fl) = field_list {
                value = match value {
                    Value::Array(arr) => Value::Array(filter_array_fields(&arr, fl)),
                    _ => filter_fields(&value, fl),
                };
            }
            println!("{}", serde_json::to_string(&value)?);
        }
    }
    Ok(())
}

// ============== Tests ==============

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_fields_none() {
        assert_eq!(parse_fields(&None), None);
    }

    #[test]
    fn test_parse_fields_single() {
        assert_eq!(
            parse_fields(&Some("id".to_string())),
            Some(vec!["id".to_string()])
        );
    }

    #[test]
    fn test_parse_fields_multiple() {
        assert_eq!(
            parse_fields(&Some("id,title,status".to_string())),
            Some(vec!["id".to_string(), "title".to_string(), "status".to_string()])
        );
    }

    #[test]
    fn test_parse_fields_with_spaces() {
        assert_eq!(
            parse_fields(&Some("id, title , status ".to_string())),
            Some(vec!["id".to_string(), "title".to_string(), "status".to_string()])
        );
    }

    #[test]
    fn test_parse_fields_empty_string() {
        assert_eq!(parse_fields(&Some("".to_string())), Some(vec![]));
    }

    #[test]
    fn test_filter_fields_object() {
        let value = json!({"id": 1, "title": "test", "prompt": "long text", "status": "pending"});
        let fields = vec!["id".to_string(), "title".to_string()];
        let result = filter_fields(&value, &fields);
        assert_eq!(result, json!({"id": 1, "title": "test"}));
    }

    #[test]
    fn test_filter_fields_missing_field() {
        let value = json!({"id": 1, "title": "test"});
        let fields = vec!["id".to_string(), "nonexistent".to_string()];
        let result = filter_fields(&value, &fields);
        assert_eq!(result, json!({"id": 1}));
    }

    #[test]
    fn test_filter_fields_non_object() {
        let value = json!("string value");
        let fields = vec!["id".to_string()];
        let result = filter_fields(&value, &fields);
        assert_eq!(result, json!("string value"));
    }

    #[test]
    fn test_filter_array_fields() {
        let arr = vec![
            json!({"id": 1, "title": "a", "prompt": "p1"}),
            json!({"id": 2, "title": "b", "prompt": "p2"}),
        ];
        let fields = vec!["id".to_string(), "title".to_string()];
        let result = filter_array_fields(&arr, &fields);
        assert_eq!(
            result,
            vec![
                json!({"id": 1, "title": "a"}),
                json!({"id": 2, "title": "b"}),
            ]
        );
    }

    #[test]
    fn test_filter_fields_empty_selection() {
        let value = json!({"id": 1, "title": "test"});
        let fields: Vec<String> = vec![];
        let result = filter_fields(&value, &fields);
        assert_eq!(result, json!({}));
    }

    // Clap parsing tests for new arguments

    #[test]
    fn test_cli_parse_raw_output() {
        let cli = Cli::try_parse_from(["ntd", "-o", "raw", "todo", "list"]).unwrap();
        assert_eq!(cli.output, OutputFormat::Raw);
    }

    #[test]
    fn test_cli_parse_fields() {
        let cli = Cli::try_parse_from(["ntd", "-f", "id,title", "todo", "list"]).unwrap();
        assert_eq!(cli.fields, Some("id,title".to_string()));
    }

    #[test]
    fn test_cli_parse_search() {
        let cli = Cli::try_parse_from(["ntd", "todo", "list", "-s", "rust"]).unwrap();
        match cli.command {
            Commands::Todo { action: TodoAction::List { search, .. } } => {
                assert_eq!(search, Some("rust".to_string()));
            }
            _ => panic!("Expected Todo::List with search"),
        }
    }

    #[test]
    fn test_cli_parse_stdin_create() {
        let cli = Cli::try_parse_from(["ntd", "todo", "create", "--stdin"]).unwrap();
        match cli.command {
            Commands::Todo { action: TodoAction::Create { stdin, .. } } => {
                assert!(stdin);
            }
            _ => panic!("Expected Todo::Create with stdin"),
        }
    }

    #[test]
    fn test_cli_parse_stdin_update() {
        let cli = Cli::try_parse_from(["ntd", "todo", "update", "1", "--stdin"]).unwrap();
        match cli.command {
            Commands::Todo { action: TodoAction::Update { stdin, .. } } => {
                assert!(stdin);
            }
            _ => panic!("Expected Todo::Update with stdin"),
        }
    }

    #[test]
    fn test_cli_parse_create_without_title_requires_stdin() {
        // Creating without title and without --stdin should still parse (validation is at runtime)
        let cli = Cli::try_parse_from(["ntd", "todo", "create"]).unwrap();
        match cli.command {
            Commands::Todo { action: TodoAction::Create { title, stdin, .. } } => {
                assert!(title.is_none());
                assert!(!stdin);
            }
            _ => panic!("Expected Todo::Create"),
        }
    }

    #[test]
    fn test_cli_parse_combined_options() {
        let cli = Cli::try_parse_from([
            "ntd", "-o", "raw", "-f", "id,title,status",
            "todo", "list",
            "--status", "pending",
            "--search", "bug",
        ]).unwrap();
        assert_eq!(cli.output, OutputFormat::Raw);
        assert_eq!(cli.fields, Some("id,title,status".to_string()));
        match cli.command {
            Commands::Todo { action: TodoAction::List { status, search, .. } } => {
                assert_eq!(status, Some("pending".to_string()));
                assert_eq!(search, Some("bug".to_string()));
            }
            _ => panic!("Expected Todo::List"),
        }
    }

    #[test]
    fn test_cli_parse_execution_resume() {
        let cli = Cli::try_parse_from(["ntd", "todo", "execution", "resume", "42"]).unwrap();
        match cli.command {
            Commands::Todo { action: TodoAction::Execution { action: ExecutionAction::Resume { id, message } } } => {
                assert_eq!(id, 42);
                assert!(message.is_none());
            }
            _ => panic!("Expected Todo::Execution::Resume"),
        }
    }

    #[test]
    fn test_cli_parse_execution_resume_with_message() {
        let cli = Cli::try_parse_from(["ntd", "todo", "execution", "resume", "42", "-m", "fix the bug"]).unwrap();
        match cli.command {
            Commands::Todo { action: TodoAction::Execution { action: ExecutionAction::Resume { id, message } } } => {
                assert_eq!(id, 42);
                assert_eq!(message, Some("fix the bug".to_string()));
            }
            _ => panic!("Expected Todo::Execution::Resume with message"),
        }
    }
}
