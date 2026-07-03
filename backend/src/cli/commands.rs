use std::io::Read;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::{
    ClientResponse, CreateTagRequest, CreateTodoRequest, DashboardStats,
    ExecutionRecord, ExecutionRecordsPage, ExecutionSummary, Tag, Todo, ExecuteRequest, LoopDto,
    TriggerLoopRequest,
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
    /// Loop management
    Loop {
        #[command(subcommand)]
        action: LoopAction,
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

        /// Working directory ID (project_directories.id). 唯一键，CLI 不再支持 path。
        #[arg(short = 'w', long = "workspace-id")]
        workspace_id: Option<i64>,

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

        /// New working directory ID (project_directories.id)
        #[arg(long = "workspace-id")]
        workspace_id: Option<i64>,

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

// ============== Loop Commands ==============

/// Loop CLI actions, mirrors the structure of Todo commands for consistency.
#[derive(Debug, Clone, Subcommand)]
pub enum LoopAction {
    /// List all loops
    List {
        /// Filter by workspace ID (unique key; use --workspace-id instead of path
        /// since path is not unique across project_directories).
        #[arg(long = "workspace-id")]
        workspace_id: Option<i64>,
    },
    /// Get loop details
    Get {
        /// Loop ID
        id: i64,
    },
    /// Update loop
    Update {
        /// Loop ID
        id: i64,

        /// New name
        #[arg(long)]
        name: Option<String>,

        /// New description
        #[arg(long)]
        description: Option<String>,

        /// New status (enabled/paused)
        #[arg(long)]
        status: Option<String>,
    },
    /// Delete loop
    Delete {
        /// Loop ID
        id: i64,
    },
    /// Stop a loop (pause all cron triggers)
    Stop {
        /// Loop ID
        id: i64,
    },
    /// Get loop execution stats
    Stats {
        /// Loop ID
        id: i64,

        /// Show recent executions (last N)
        #[arg(long, default_value = "5")]
        recent: i64,
    },
    /// Execute loop
    Execute {
        /// Loop ID
        id: i64,

        /// Parameters for placeholder replacement (key=value format, can be repeated)
        /// Example: --param project_name=myproject --param env=production
        /// These params will be injected into step prompts via {{params.key}} placeholders.
        #[arg(long = "param", num_args = 1, value_parser = parse_key_value)]
        params: Option<Vec<(String, String)>>,
    },
    /// Execution records
    Execution {
        #[command(subcommand)]
        action: LoopExecutionAction,
    },
}

/// Loop execution records subcommands
#[derive(Debug, Clone, Subcommand)]
pub enum LoopExecutionAction {
    /// List execution records for a loop
    List {
        /// Loop ID
        loop_id: i64,

        /// Page number
        #[arg(long, default_value = "1")]
        page: i64,

        /// Items per page
        #[arg(long, default_value = "20")]
        limit: i64,
    },
    /// Get execution details
    Get {
        /// Execution ID
        execution_id: i64,
    },
    /// Show the blackboard (accumulated step conclusions) for a loop execution.
    /// 默认输出 JSON（AI/脚本友好）；加 --human 输出黑板视图（人眼友好）。
    Blackboard {
        /// Execution ID
        execution_id: i64,

        /// 输出人类可读黑板视图（默认是 JSON，便于 AI/脚本消费）
        #[arg(long, default_value = "false")]
        human: bool,
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
        Commands::Loop { action } => handle_loop(&client, action, &cli.output, &cli.fields).await?,
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
        TodoAction::Create { title, prompt, file, stdin, executor, workspace_id, tags, schedule } => {
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
                        webhook_enabled: None,
                        // stdin 路径下 workspace_id 由 JSON body 提供；若 body 没传则 fallback 到 CLI 参数；
                        // 闭包内部不能 `?`，因此取不到时填 0，由下面 outer 检查 fail-fast。
                        workspace_id: value.get("workspace_id").and_then(|v| v.as_i64())
                            .or(*workspace_id)
                            .unwrap_or(0),
                        action_type: None,
                        action_key: None,
                    });
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
                    webhook_enabled: None,
                    auto_review_enabled: None,
                    // 非 stdin 模式下 workspace_id 必填：CLI 唯一标识符是 id 而非 path
                    workspace_id: workspace_id.ok_or_else(|| anyhow::anyhow!("--workspace-id is required"))?,
                    action_type: None,
                    action_key: None,
                }
            };

            // Set scheduler options from CLI args
            if let Some(s) = schedule {
                if !s.is_empty() {
                    req.scheduler_enabled = Some(true);
                    req.scheduler_config = Some(s.clone());
                }
            }

            // stdin 闭包不能 `?`，这里做统一的 fail-fast：workspace_id=0 表示上游未传
            if req.workspace_id == 0 {
                return Err(anyhow::anyhow!("workspace_id is required (pass --workspace-id or include in stdin JSON)").into());
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
        TodoAction::Update { id, title, prompt, file, stdin, status, executor, workspace_id, tags, schedule } => {
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
                    "workspace_id": workspace_id,
                })
            };

            // Merge CLI args over stdin values
            if let Some(t) = title { req["title"] = t.clone().into(); }
            if let Some(p) = prompt { req["prompt"] = p.clone().into(); }
            if let Some(s) = status { req["status"] = s.clone().into(); }
            if let Some(e) = executor { req["executor"] = e.clone().into(); }
            if let Some(w) = workspace_id { req["workspace_id"] = Value::from(*w as i64); }
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

// ============== Loop Handlers ==============

async fn handle_loop(
    client: &ApiClient,
    action: &LoopAction,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    match action {
        LoopAction::List { workspace_id } => {
            let path = if let Some(wid) = workspace_id {
                format!("/loops?workspace_id={}", wid)
            } else {
                "/loops".to_string()
            };
            let resp: ClientResponse<Vec<LoopDto>> = client.get(&path).await?;
            print_response(resp, output, fields)?;
        }
        LoopAction::Get { id } => {
            let resp: ClientResponse<LoopDto> = client.get(&format!("/loops/{}", id)).await?;
            print_response(resp, output, fields)?;
        }
        LoopAction::Update { id, name, description, status } => {
            // 构建部分更新 JSON，只包含提供的字段
            let mut obj = serde_json::Map::new();
            if let Some(n) = name {
                obj.insert("name".to_string(), serde_json::Value::String(n.to_string()));
            }
            if let Some(d) = description {
                obj.insert("description".to_string(), serde_json::Value::String(d.to_string()));
            }
            if let Some(s) = status {
                obj.insert("status".to_string(), serde_json::Value::String(s.to_string()));
            }
            let req = serde_json::Value::Object(obj);
            let resp: ClientResponse<LoopDto> = client.put(
                &format!("/loops/{}", id),
                &req,
            ).await?;
            print_response(resp, output, fields)?;
        }
        LoopAction::Delete { id } => {
            let resp: ClientResponse<()> = client.delete(&format!("/loops/{}", id)).await?;
            print_response(resp, output, fields)?;
        }
        LoopAction::Stop { id } => {
            // Pause the loop by disabling all its triggers
            let req = serde_json::json!({ "status": "paused" });
            let resp: ClientResponse<LoopDto> = client.put(
                &format!("/loops/{}/status", id),
                &req,
            ).await?;
            print_response(resp, output, fields)?;
        }
        LoopAction::Stats { id, recent } => {
            // Get loop details with recent executions combined into one response
            let resp: ClientResponse<LoopDto> = client.get(&format!("/loops/{}", id)).await?;
            let execs_resp: ClientResponse<serde_json::Value> = client.get(&format!(
                "/loops/{}/executions?page=1&limit={}",
                id, recent
            )).await?;
            // Combine loop info and recent executions into a single JSON object
            let combined = serde_json::json!({
                "loop": resp.data,
                "recent_executions": execs_resp.data,
            });
            let final_resp: ClientResponse<serde_json::Value> = ClientResponse {
                code: execs_resp.code,
                data: Some(combined),
                message: execs_resp.message,
            };
            print_response(final_resp, output, fields)?;
        }
        LoopAction::Execute { id, params } => {
            let params_map: std::collections::HashMap<String, String> = params
                .as_ref()
                .map(|vec| vec.iter().cloned().collect())
                .unwrap_or_default();
            let req = TriggerLoopRequest { params: params_map };
            let resp: ClientResponse<serde_json::Value> = client.post(
                &format!("/loops/{}/trigger", id),
                &req,
            ).await?;
            print_response(resp, output, fields)?;
        }
        LoopAction::Execution { action } => {
            match action {
                LoopExecutionAction::List { loop_id, page, limit } => {
                    let path = format!(
                        "/loops/{}/executions?page={}&limit={}",
                        loop_id, page, limit
                    );
                    let resp: ClientResponse<serde_json::Value> = client.get(&path).await?;
                    print_response(resp, output, fields)?;
                }
                LoopExecutionAction::Get { execution_id } => {
                    // Get execution results by execution ID directly
                    // 注意: ApiClient 已经自动添加 /api 前缀，所以路径不要带 /api
                    let resp: ClientResponse<serde_json::Value> = client.get(&format!(
                        "/loop-executions/{}",
                        execution_id
                    )).await?;
                    print_response(resp, output, fields)?;
                }
                LoopExecutionAction::Blackboard { execution_id, human } => {
                    // 复用 get_execution_by_id handler 返回的 LoopExecutionDetail,
                    // 它已经按 sequence_index 升序返回 step_executions。
                    // 不新增 API 端点 — 黑板视图本质就是 step_executions 的渲染。
                    let resp: ClientResponse<serde_json::Value> = client.get(&format!(
                        "/loop-executions/{}",
                        execution_id
                    )).await?;
                    if resp.code != 0 {
                        // 与 print_response 一致:错误码非 0 时抛 anyhow
                        return Err(anyhow::anyhow!("API error {}: {}", resp.code, resp.message));
                    }
                    if *human {
                        // 人类视图: 黑板文本渲染
                        render_blackboard(resp.data.as_ref());
                    } else {
                        // 默认: JSON, 直接是 LoopExecutionDetail, AI/脚本友好
                        println!("{}", serde_json::to_string_pretty(&resp.data)?);
                    }
                }
            }
        }
    }
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

// ============== Blackboard Rendering ==============

/// 把 step.status 映射到人类可读的 emoji，与前端 `BlackboardDrawer.tsx` 保持一致。
/// 未知状态使用 ❔ 而非抛错，避免数据库新增状态时让旧 CLI 直接崩溃。
fn status_icon(status: &str) -> &'static str {
    match status {
        "success" => "✅",
        "failed" => "❌",
        "running" => "⏳",
        "pending" => "⏸ ",
        "pending_approval" => "🤔",
        "skipped" => "⏭️",
        _ => "❔",
    }
}

/// 把 LoopExecutionDetail 渲染成人类可读的黑板视图。
///
/// 输入是 `serde_json::Value`（来自 ApiClient 的反序列化结果），不是强类型，
/// 是因为这个函数唯一的调用点在 CLI 命令分发处，没必要为它再定义一个 DTO。
/// 如果渲染失败（字段缺失或类型错误），降级输出原始 JSON + 错误提示，
/// 而不是让 CLI 崩溃——黑板视图是辅助功能，不能阻塞主流程。
fn render_blackboard(data: Option<&Value>) {
    let Some(data) = data else {
        println!("(无数据)");
        return;
    };

    let exec_id = data.get("id").and_then(Value::as_i64).unwrap_or(0);
    let loop_name = data.get("loop_name").and_then(Value::as_str).unwrap_or("?");
    let trigger_meta = data.get("trigger_meta").and_then(Value::as_str).unwrap_or("");
    let status = data.get("status").and_then(Value::as_str).unwrap_or("unknown");
    let total = data.get("total_steps").and_then(Value::as_i64).unwrap_or(0);
    let completed = data.get("completed_steps").and_then(Value::as_i64).unwrap_or(0);
    let started = data.get("started_at").and_then(Value::as_str).unwrap_or("");
    let finished = data.get("finished_at").and_then(Value::as_str).unwrap_or("");

    println!("═══ Loop Execution #{exec_id} ────────────────────────────────");
    println!("循环: {loop_name}");
    if !trigger_meta.is_empty() && trigger_meta != "{}" {
        println!("触发: {trigger_meta}");
    }
    println!(
        "状态: {} {} · 完成 {}/{} 步",
        status_icon(status),
        status,
        completed,
        total
    );
    if !started.is_empty() {
        let end_part = if !finished.is_empty() {
            format!(" · 结束: {finished}")
        } else {
            String::new()
        };
        println!("开始: {started}{end_part}");
    }
    println!();

    let steps = data.get("step_executions").and_then(Value::as_array);
    let Some(steps) = steps else {
        println!("(step_executions 字段缺失或类型错误)");
        println!("\n原始数据:\n{}", serde_json::to_string_pretty(data).unwrap_or_default());
        return;
    };

    if steps.is_empty() {
        println!("黑板为空（loop 尚未执行任何步骤）");
        println!();
    } else {
        for step in steps {
            render_blackboard_step(step);
        }
        println!();
    }

    // Token 汇总：LoopExecutionDetail.token_summary 是独立字段
    if let Some(ts) = data.get("token_summary") {
        let ti = ts.get("total_input_tokens").and_then(Value::as_i64).unwrap_or(0);
        let to = ts.get("total_output_tokens").and_then(Value::as_i64).unwrap_or(0);
        println!("═══ {} 步 / Token: 输入 {} 输出 {} ════════════════════════", steps.len(), ti, to);
    } else {
        println!("═══ {} 步 ═══════════════════════════════════════════════════", steps.len());
    }
}

/// 渲染单个 step 块。
/// 字段名与 `LoopStepExecutionDto` 一致（见 `backend/src/models/loop_.rs`）。
fn render_blackboard_step(step: &Value) {
    let seq = step.get("sequence_index").and_then(Value::as_i64).unwrap_or(0);
    let status = step.get("status").and_then(Value::as_str).unwrap_or("unknown");
    // step_name 为 None 时回退到 "step #{step_id}"，异常处理步骤（step_id=-1）显示「异常处理」
    let step_name = match (
        step.get("step_name").and_then(Value::as_str),
        step.get("step_id").and_then(Value::as_i64),
    ) {
        (Some(name), _) if !name.is_empty() => name.to_string(),
        (None, Some(-1)) => "异常处理".to_string(),
        (_, Some(sid)) => format!("step #{sid}"),
        (_, None) => "(未知环节)".to_string(),
    };
    let rating = step
        .get("rating")
        .and_then(Value::as_i64)
        .map(|r| r.to_string())
        .unwrap_or_else(|| "-".to_string());
    let exec_id = step
        .get("execution_record_id")
        .and_then(Value::as_i64)
        .map(|r| format!("#{r}"))
        .unwrap_or_else(|| "-".to_string());

    println!(
        "  #{seq} {} {:<22} 评分 {rating}",
        status_icon(status),
        truncate(&step_name, 22),
    );
    println!("     exec: {exec_id}");

    // 结论展示：优先级为 approval_comment (pending_approval) > conclusion > error_message > (无结论)
    if status == "pending_approval" {
        if let Some(comment) = step.get("approval_comment").and_then(Value::as_str) {
            if !comment.is_empty() {
                println!("     待审批意见: {comment}");
            }
        }
        println!("     (等待人工审批)");
    } else if let Some(err) = step.get("error_message").and_then(Value::as_str) {
        if !err.is_empty() {
            println!("     失败: {err}");
        }
        if let Some(c) = step.get("conclusion").and_then(Value::as_str) {
            if !c.is_empty() {
                println!("     结论: {c}");
            }
        }
    } else if let Some(c) = step.get("conclusion").and_then(Value::as_str) {
        if !c.is_empty() {
            // 多行结论：保留缩进让层级清晰
            for line in c.lines() {
                println!("     {line}");
            }
        } else {
            println!("     (无结论)");
        }
    } else {
        println!("     (无结论)");
    }
}

/// 截断字符串到指定字符宽度（按 char 边界，安全处理 UTF-8）。
/// 中文/全角字符按 1 个单位计算，与终端实际宽度一致。
fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    let mut out: String = chars.into_iter().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
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

    // ===== Blackboard CLI tests =====

    #[test]
    fn test_cli_parse_loop_execution_blackboard() {
        // 校验命令行解析：ntd loop execution blackboard 42
        // 默认行为: JSON 输出 (human=false)
        let cli = Cli::try_parse_from(["ntd", "loop", "execution", "blackboard", "42"]).unwrap();
        match cli.command {
            Commands::Loop { action: LoopAction::Execution { action: LoopExecutionAction::Blackboard { execution_id, human } } } => {
                assert_eq!(execution_id, 42);
                assert!(!human, "默认应输出 JSON，human=false");
            }
            _ => panic!("Expected Loop::Execution::Blackboard"),
        }
    }

    #[test]
    fn test_cli_parse_loop_execution_blackboard_human() {
        // --human 开关: 启用人类可读黑板视图
        let cli = Cli::try_parse_from(["ntd", "loop", "execution", "blackboard", "42", "--human"]).unwrap();
        match cli.command {
            Commands::Loop { action: LoopAction::Execution { action: LoopExecutionAction::Blackboard { execution_id, human } } } => {
                assert_eq!(execution_id, 42);
                assert!(human, "--human 应启用人类视图");
            }
            _ => panic!("Expected Loop::Execution::Blackboard with --human"),
        }
    }

    #[test]
    fn test_status_icon_known() {
        // 已知状态全部映射到正确 emoji
        assert_eq!(status_icon("success"), "✅");
        assert_eq!(status_icon("failed"), "❌");
        assert_eq!(status_icon("running"), "⏳");
        assert_eq!(status_icon("pending"), "⏸ ");
        assert_eq!(status_icon("pending_approval"), "🤔");
        assert_eq!(status_icon("skipped"), "⏭️");
    }

    #[test]
    fn test_status_icon_unknown() {
        // 未知状态降级为 ❔ 而非 panic — 数据库可能新增 status 时不应让旧 CLI 崩溃
        assert_eq!(status_icon("something_new"), "❔");
        assert_eq!(status_icon(""), "❔");
    }

    #[test]
    fn test_truncate_short() {
        // 短于阈值原样返回
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("中文测试", 10), "中文测试");
    }

    #[test]
    fn test_truncate_long() {
        // 超长截断到阈值-1 + 省略号，避免 panic 在 UTF-8 字符边界
        let s = "this is a very long step name that exceeds limit";
        let out = truncate(s, 10);
        assert_eq!(out.chars().count(), 10);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn test_truncate_utf8_safe() {
        // 截断点落在多字节字符中间时不能 panic
        let s = "中文abcdefghij";
        let out = truncate(s, 5);
        assert!(out.chars().count() <= 5);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn test_render_blackboard_none() {
        // data 为 None 时输出降级提示，不 panic
        render_blackboard(None);
    }

    #[test]
    fn test_render_blackboard_normal() {
        // 正常 3 step 全 success：断言输出包含关键字段
        let data = json!({
            "id": 42,
            "loop_name": "每日代码 review",
            "trigger_meta": "cron @ 0 9 * * *",
            "status": "success",
            "total_steps": 3,
            "completed_steps": 3,
            "started_at": "2026-07-03 09:00:00",
            "finished_at": "2026-07-03 09:45:32",
            "step_executions": [
                {"sequence_index": 1, "step_id": 1, "step_name": "编写 CRUD 代码", "status": "success", "rating": 85, "execution_record_id": 1024, "conclusion": "完成了用户登录功能的 CRUD 代码"},
                {"sequence_index": 2, "step_id": 2, "step_name": "补充单元测试", "status": "success", "rating": 90, "execution_record_id": 1025, "conclusion": "新增 12 个测试用例，覆盖率提升到 87%"},
                {"sequence_index": 3, "step_id": 3, "step_name": "更新 README", "status": "success", "rating": 75, "execution_record_id": 1026, "conclusion": "更新了安装步骤"}
            ],
            "token_summary": {"total_input_tokens": 12000, "total_output_tokens": 5000}
        });
        render_blackboard(Some(&data));
        // 渲染函数直接 println，单元测试只验证它不 panic；
        // 真实输出验证放在手动测试或集成测试。
    }

    #[test]
    fn test_render_blackboard_no_record_id() {
        // execution_record_id 为 None 时不应 panic
        let data = json!({
            "id": 1,
            "loop_name": "L",
            "status": "running",
            "total_steps": 1,
            "completed_steps": 0,
            "step_executions": [
                {"sequence_index": 1, "step_id": 1, "step_name": "等待中", "status": "pending", "conclusion": null, "execution_record_id": null}
            ]
        });
        render_blackboard(Some(&data));
    }

    #[test]
    fn test_render_blackboard_empty() {
        // step_executions 为空时显示「黑板为空」提示
        let data = json!({
            "id": 1,
            "loop_name": "L",
            "status": "pending",
            "total_steps": 0,
            "completed_steps": 0,
            "step_executions": []
        });
        render_blackboard(Some(&data));
    }

    #[test]
    fn test_render_blackboard_failed() {
        // failed step：有 error_message 但无 conclusion 时，error_message 替代结论
        let data = json!({
            "id": 1,
            "loop_name": "L",
            "status": "failed",
            "total_steps": 2,
            "completed_steps": 1,
            "step_executions": [
                {"sequence_index": 1, "step_id": 1, "step_name": "成功步骤", "status": "success", "rating": 80, "conclusion": "ok"},
                {"sequence_index": 2, "step_id": 2, "step_name": "失败步骤", "status": "failed", "error_message": "执行超时", "conclusion": null, "execution_record_id": null}
            ]
        });
        render_blackboard(Some(&data));
    }

    #[test]
    fn test_render_blackboard_pending_approval() {
        // pending_approval：显示 approval_comment + 待审批提示
        let data = json!({
            "id": 1,
            "loop_name": "L",
            "status": "running",
            "total_steps": 1,
            "completed_steps": 0,
            "step_executions": [
                {"sequence_index": 1, "step_id": 1, "step_name": "需要审批", "status": "pending_approval", "approval_comment": "请确认改动", "conclusion": null}
            ]
        });
        render_blackboard(Some(&data));
    }

    #[test]
    fn test_render_blackboard_anomaly_handler() {
        // step_id=-1 → 显示「异常处理」
        let data = json!({
            "id": 1,
            "loop_name": "L",
            "status": "failed",
            "total_steps": 2,
            "completed_steps": 1,
            "step_executions": [
                {"sequence_index": 999, "step_id": -1, "step_name": null, "status": "failed", "conclusion": "触发异常处理流程"}
            ]
        });
        render_blackboard(Some(&data));
    }

    #[test]
    fn test_render_blackboard_missing_step_executions() {
        // step_executions 字段缺失时降级：打印提示 + 原始数据，不 panic
        let data = json!({
            "id": 1,
            "loop_name": "L",
            "status": "running"
        });
        render_blackboard(Some(&data));
    }
}
