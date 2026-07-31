use std::io::Read;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::project_directory::ProjectDirectory;
use crate::models::{
    ClientResponse, CreateTagRequest, CreateTodoRequest, DashboardStats,
    ExecutionRecord, ExecutionRecordsPage, ExecutionSummary, Tag, Todo, ExecuteRequest, LoopDto,
};
use crate::cli::client::ApiClient;
use crate::config;

/// 对 slug 进行 percent-encoding，防止特殊字符破坏 URL 路径结构。
fn percent_encode_slug(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            _ => {
                result.push('%');
                result.push_str(&hex_digit(b >> 4));
                result.push_str(&hex_digit(b & 0xf));
            }
        }
    }
    result
}

fn hex_digit(b: u8) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    format!("{}{}", HEX[(b as usize) >> 4] as char, HEX[(b as usize) & 0xf] as char)
}

#[derive(Parser, Debug)]
#[command(name = "ntd")]
#[command(about = "AI-powered task engine CLI", long_about = None)]
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
    Stats {
        // Dashboard 为全局运营视图，不再依赖 workspace_id。
    },
    /// Blackboard (knowledge wiki) management
    Blackboard {
        #[command(subcommand)]
        action: BlackboardAction,
    },
    /// Workspace (project directory) management
    Workspace {
        #[command(subcommand)]
        action: WorkspaceAction,
    },
    /// Process template management
    Process {
        #[command(subcommand)]
        action: ProcessAction,
    },
}

/// Workspace CLI actions: 列出 / 查询单个 / 注册一个项目目录。
/// 后端表 `project_directories` 的唯一键是自增 id，path 不保证唯一，
/// 故 CLI 一律按 id 消费；create 仍要 path+name 是因为注册动作就是登记这两个字段。
#[derive(Debug, Clone, Subcommand)]
pub enum WorkspaceAction {
    /// List all registered workspaces
    List,
    /// Register a new workspace (project directory)
    Create {
        /// Directory path (absolute or relative; stored verbatim)
        #[arg(short, long)]
        path: String,

        /// Human-readable name (shown in todo picker)
        #[arg(short, long)]
        name: String,
    },
}

/// Blackboard CLI actions: manage wiki pages for a workspace.
#[derive(Debug, Clone, Subcommand)]
pub enum BlackboardAction {
    /// Wiki 文件管理（从旧的 Page 语义迁移为 Wiki 文件语义）。
    /// workspace_id 用于限定文件系统 Wiki 读取范围，确保不同 workspace 数据隔离。
    Wiki {
        #[command(subcommand)]
        action: WikiAction,
        /// Working directory ID (project_directories.id)
        #[arg(short = 'w', long = "workspace-id")]
        workspace_id: i64,
    },
}

/// 把 workspace_id 拼成 v1 路径前缀 `/workspaces/{ws}`。
/// 单独抽出来避免在每个分支里手写 format!，确保所有 workspace-scoped 命令路径一致。
fn ws_prefix(workspace_id: i64) -> String {
    format!("/workspaces/{}", workspace_id)
}

/// Wiki 文件子命令（替代旧的 Page 子命令）。
#[derive(Debug, Clone, Subcommand)]
pub enum WikiAction {
    /// 列出所有 wiki 文件（index, log, topics）
    List,
    /// 根据 slug 获取单个 wiki 文件内容
    Get {
        /// 文件 slug（如 "auth-module", "index", "log"）
        slug: String,
    },
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

        /// Working directory ID (project_directories.id). v1 路由必填（嵌入 URL）。
        /// 唯一键，CLI 不再支持 path；stdin 模式下也必须传，body 字段会被本参数覆盖。
        #[arg(short = 'w', long = "workspace-id")]
        workspace_id: i64,

        /// Tag IDs (comma-separated)
        #[arg(long)]
        tags: Option<String>,

        /// Schedule (cron expression, e.g. "*/30 * * * *")
        #[arg(long)]
        schedule: Option<String>,
    },
    /// List todos
    List {
        /// Workspace ID (project_directories.id). v1 路由要求 workspace 嵌入 URL。
        #[arg(long = "workspace-id")]
        workspace_id: i64,

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
        /// Workspace ID (project_directories.id). v1 路由要求 workspace 嵌入 URL。
        #[arg(long = "workspace-id")]
        workspace_id: i64,

        /// Todo ID
        id: i64,
    },
    /// Update todo
    Update {
        /// Todo ID
        id: i64,

        /// Workspace ID (project_directories.id) the todo currently belongs to.
        /// v1 路由把 workspace 嵌入 URL；要切换 workspace 请用 --stdin 传 body.workspace_id。
        #[arg(long = "workspace-id")]
        workspace_id: i64,

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

        /// New tag IDs (comma-separated)
        #[arg(long)]
        tags: Option<String>,

        /// Schedule (cron expression)
        #[arg(long)]
        schedule: Option<String>,
    },
    /// Delete todo
    Delete {
        /// Workspace ID (project_directories.id). v1 路由要求 workspace 嵌入 URL。
        #[arg(long = "workspace-id")]
        workspace_id: i64,

        /// Todo ID
        id: i64,
    },
    /// Execute todo
    Execute {
        /// Workspace ID (project_directories.id). v1 路由要求 workspace 嵌入 URL。
        #[arg(long = "workspace-id")]
        workspace_id: i64,

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
        /// Workspace ID (project_directories.id). v1 路由要求 workspace 嵌入 URL。
        #[arg(long = "workspace-id")]
        workspace_id: i64,

        /// Execution record ID（v1 路由把 record_id 嵌入 URL，旧版的 todo_id body 字段已废弃）
        id: i64,
    },
    /// Get todo execution stats
    Stats {
        /// Workspace ID (project_directories.id). v1 路由要求 workspace 嵌入 URL。
        #[arg(long = "workspace-id")]
        workspace_id: i64,

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
        /// Workspace ID (project_directories.id). v1 路由要求 workspace 嵌入 URL。
        #[arg(long = "workspace-id")]
        workspace_id: i64,

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
        /// Workspace ID (project_directories.id). v1 路由要求 workspace 嵌入 URL。
        #[arg(long = "workspace-id")]
        workspace_id: i64,

        /// Execution record ID
        id: i64,
    },
    /// Resume a conversation from an execution record
    Resume {
        /// Workspace ID (project_directories.id). v1 路由要求 workspace 嵌入 URL。
        #[arg(long = "workspace-id")]
        workspace_id: i64,

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

/// 工艺模板 CLI 动作。
///
/// 主标识统一用位置参数 `<NAME_OR_GUID>`：传人类可读的 name（如 4p12s-delivery）
/// 或 guid（UUID v4）都行，由 `resolve_process_guid` 自动解析——修复了 040 之后
/// 旧版把 name 直接塞进 guid 路径段导致 404 的 bug。
///
/// 风格与 todo/loop 对齐：位置参数放主标识、内容走 --file/--stdin、输出复用全局
/// --output/--fields。后端能力（recommend/create/delete/upgrade/loops/versions/diff）
/// 已就绪，这里只做 CLI 封装。
#[derive(Debug, Clone, Subcommand)]
pub enum ProcessAction {
    /// 列出所有工艺模板（--system 只看系统模板，--user 只看用户自建，二者互斥）
    List {
        /// 只看系统工艺（bundled 同步来的）
        #[arg(long)]
        system: bool,
        /// 只看用户自建工艺
        #[arg(long)]
        user: bool,
    },
    /// 查看工艺模板详情（name 或 guid 都可）
    Show {
        /// 工艺 name（如 4p12s-delivery）或 guid（UUID v4）
        name_or_guid: String,
    },
    /// 根据任务描述推荐合适的工艺（AI 最常用：描述目标 → 拿到匹配工艺 + 理由）
    Recommend {
        /// 任务描述（自然语言，描述你想达成什么目标）
        description: String,
    },
    /// 新建用户工艺（YAML 正文走 --file 或 --stdin）
    Create {
        /// 工艺唯一标识，^[a-zA-Z0-9_-]+$（--stdin 模式下可省略，从 body 读）
        #[arg(short = 'n', long)]
        name: Option<String>,

        /// 人类可读名称（可空，回退到 name）
        #[arg(long)]
        display_name: Option<String>,

        /// 分类（可空）
        #[arg(long)]
        category: Option<String>,

        /// 复杂度（可空）
        #[arg(long)]
        complexity: Option<String>,

        /// 版本（可空，默认由后端给 1.0.0）
        #[arg(long)]
        version: Option<String>,

        /// 从文件读取工艺 YAML 正文
        #[arg(short = 'f', long)]
        file: Option<String>,

        /// 从 stdin 读取完整 JSON body（覆盖以上字段）
        #[arg(long)]
        stdin: bool,
    },
    /// 删除用户工艺（系统工艺后端会拒绝，错误经统一通道透出）
    Delete {
        /// 工艺 name 或 guid
        name_or_guid: String,
    },
    /// 安装工艺到工作空间并触发执行
    Run {
        /// 工艺 name 或 guid
        name_or_guid: String,

        /// 目标工作空间路径（按 path 反查 workspace_id）
        /// process 域一直按 path 消费（与 todo/loop 的 --workspace-id 不同），
        /// 因为 AI 手里通常是项目路径而非 ws_id，按 path 更顺手。
        #[arg(long = "workspace")]
        workspace: String,
    },
    /// 把指定 loop 升级到工艺模板最新版
    Upgrade {
        /// 工艺 name 或 guid
        name_or_guid: String,

        /// 要升级的 loop id（先用 `ntd process loops <name-or-guid>` 查到）
        #[arg(long = "loop-id")]
        loop_id: i64,
    },
    /// 列出该工艺实例化的所有 loop
    Loops {
        /// 工艺 name 或 guid
        name_or_guid: String,
    },
    /// 查看工艺版本历史
    Versions {
        /// 工艺 name 或 guid
        name_or_guid: String,
    },
    /// 对比两个版本的工艺正文 diff
    Diff {
        /// 工艺 name 或 guid
        name_or_guid: String,

        /// 目标版本号（如 1.2.0）
        version: String,

        /// 基准版本号
        #[arg(long = "base")]
        base: String,
    },
    /// 查看工艺实例审计状态（按 loop execution id 遍历工作空间查找）
    ExecutionStatus {
        /// Loop execution ID
        id: i64,
    },
}

/// Loop CLI actions, mirrors the structure of Todo commands for consistency.
#[derive(Debug, Clone, Subcommand)]
pub enum LoopAction {
    /// List all loops
    List {
        /// Workspace ID (project_directories.id). v1 路由要求 workspace 嵌入 URL；
        /// 不再是可选 filter，而是必填 URL 段（旧版跨 workspace 列表能力已下线）。
        #[arg(long = "workspace-id")]
        workspace_id: i64,
    },
    /// Get loop details
    Get {
        /// Workspace ID (project_directories.id). v1 路由要求 workspace 嵌入 URL。
        #[arg(long = "workspace-id")]
        workspace_id: i64,

        /// Loop ID
        id: i64,
    },
    /// Update loop
    Update {
        /// Workspace ID (project_directories.id). v1 路由要求 workspace 嵌入 URL。
        #[arg(long = "workspace-id")]
        workspace_id: i64,

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
        /// Workspace ID (project_directories.id). v1 路由要求 workspace 嵌入 URL。
        #[arg(long = "workspace-id")]
        workspace_id: i64,

        /// Loop ID
        id: i64,
    },
    /// Stop a loop (pause all cron triggers)
    Stop {
        /// Workspace ID (project_directories.id). v1 路由要求 workspace 嵌入 URL。
        #[arg(long = "workspace-id")]
        workspace_id: i64,

        /// Loop ID
        id: i64,
    },
    /// Get loop execution stats
    Stats {
        /// Workspace ID (project_directories.id). v1 路由要求 workspace 嵌入 URL。
        #[arg(long = "workspace-id")]
        workspace_id: i64,

        /// Loop ID
        id: i64,

        /// Show recent executions (last N)
        #[arg(long, default_value = "5")]
        recent: i64,
    },
    // 044：Execute 子命令（基于 TriggerLoopRequest 手动触发 loop）随触发器入口下线。
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
        /// Workspace ID (project_directories.id). v1 路由要求 workspace 嵌入 URL。
        #[arg(long = "workspace-id")]
        workspace_id: i64,

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
        /// Workspace ID (project_directories.id). v1 路由要求 workspace 嵌入 URL。
        #[arg(long = "workspace-id")]
        workspace_id: i64,

        /// Loop ID（v1 路径 /workspaces/{ws}/loops/{loop_id}/executions/{eid} 必填）
        #[arg(long = "loop-id")]
        loop_id: i64,

        /// Execution ID
        execution_id: i64,
    },
    /// Show the blackboard (accumulated step conclusions) for a loop execution.
    /// 默认输出 JSON（AI/脚本友好）；加 --human 输出黑板视图（人眼友好）。
    Blackboard {
        /// Workspace ID (project_directories.id). v1 路由要求 workspace 嵌入 URL。
        #[arg(long = "workspace-id")]
        workspace_id: i64,

        /// Loop ID（v1 路径 /workspaces/{ws}/loops/{loop_id}/executions/{eid} 必填）
        #[arg(long = "loop-id")]
        loop_id: i64,

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
        Commands::Stats { } => handle_stats(&client, &cli.output, &cli.fields).await?,
        Commands::Blackboard { action } => handle_blackboard(&client, action, &cli.output, &cli.fields).await?,
        Commands::Workspace { action } => handle_workspace(&client, action, &cli.output, &cli.fields).await?,
        Commands::Process { action } => handle_process(&client, action, &cli.output, &cli.fields).await?,
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
            // workspace_id 现在是必填（i64，不再 Option），从 clap 直接拿；
            // stdin 模式下若 JSON 也带 workspace_id，以命令行参数为准（更显式）。
            let ws_id = *workspace_id;
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
                        expert_name: value.get("expert_name").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        scheduler_enabled: None,
                        scheduler_config: None,
                        scheduler_timezone: None,
                        acceptance_criteria: value.get("acceptance_criteria").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        auto_review_enabled: value.get("auto_review_enabled").and_then(|v| v.as_bool()),
                        webhook_enabled: None,
                        // URL 已经带 workspace，body 字段以 CLI 参数为准
                        workspace_id: Some(ws_id),
                        action_type: None,
                        action_key: None,
                        model: None,
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
                    expert_name: None,
                    scheduler_enabled: None,
                    scheduler_config: None,
                    scheduler_timezone: None,
                    acceptance_criteria: None,
                    webhook_enabled: None,
                    auto_review_enabled: None,
                    workspace_id: Some(ws_id),
                    action_type: None,
                    action_key: None,
                    model: None,
                }
            };

            // Set scheduler options from CLI args
            if let Some(s) = schedule {
                if !s.is_empty() {
                    req.scheduler_enabled = Some(true);
                    req.scheduler_config = Some(s.clone());
                }
            }

            // v1: workspace 嵌入 URL 路径段，body 的 workspace_id 仅作冗余字段。
            // body 仍保留是为了兼容后端 CreateTodoRequest 解析（handler 也读 body.workspace_id）。
            let path = format!("{}/todos", ws_prefix(ws_id));
            let resp: ClientResponse<Todo> = client.post(&path, &req).await?;
            print_response(&resp, output, fields)?;
        }
        TodoAction::List { workspace_id, status, tag, running, search } => {
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

            // v1: 列表也按 workspace 隔离，URL 前缀带 ws。
            let path = if query_params.is_empty() {
                format!("{}/todos", ws_prefix(*workspace_id))
            } else {
                format!("{}/todos?{}", ws_prefix(*workspace_id), query_params.join("&"))
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

            print_response(&resp, output, fields)?;
        }
        TodoAction::Get { workspace_id, id } => {
            // v1: GET /workspaces/{ws}/todos/{id}
            let path = format!("{}/todos/{}", ws_prefix(*workspace_id), id);
            let resp: ClientResponse<Todo> = client.get(&path).await?;
            print_response(&resp, output, fields)?;
        }
        TodoAction::Update { id, workspace_id, title, prompt, file, stdin, status, executor, tags, schedule } => {
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
                })
            };

            // Merge CLI args over stdin values
            if let Some(t) = title { req["title"] = t.clone().into(); }
            if let Some(p) = prompt { req["prompt"] = p.clone().into(); }
            if let Some(s) = status { req["status"] = s.clone().into(); }
            if let Some(e) = executor { req["executor"] = e.clone().into(); }
            if let Some(t) = tags {
                let tag_ids: Vec<i64> = t.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                req["tag_ids"] = tag_ids.into();
            }
            if let Some(s) = schedule {
                req["scheduler_enabled"] = (!s.is_empty()).into();
                req["scheduler_config"] = if s.is_empty() { Value::Null } else { s.clone().into() };
            }

            // v1: PUT /workspaces/{ws}/todos/{id}。workspace_id 不再放进 body（移除 --workspace-id 作为 body move 的语义）；
            // 如需切换 workspace，请用 --stdin 在 body 中带 workspace_id 字段。
            let path = format!("{}/todos/{}", ws_prefix(*workspace_id), id);
            let resp: ClientResponse<Todo> = client.put(&path, &req).await?;
            print_response(&resp, output, fields)?;
        }
        TodoAction::Delete { workspace_id, id } => {
            // v1: DELETE /workspaces/{ws}/todos/{id}
            let path = format!("{}/todos/{}", ws_prefix(*workspace_id), id);
            let resp: ClientResponse<()> = client.delete(&path).await?;
            print_response(&resp, output, fields)?;
        }
        TodoAction::Execute { workspace_id, id, message, executor, params } => {
            let params: Option<std::collections::HashMap<String, String>> = params.as_ref().map(|vec| {
                vec.iter().cloned().collect()
            });
            let req = ExecuteRequest {
                todo_id: *id,
                message: message.clone(),
                executor: executor.clone(),
                model: None,
                params,
            };
            // v1: 触发执行改走 executions 域，POST /workspaces/{ws}/executions
            let path = format!("{}/executions", ws_prefix(*workspace_id));
            let resp: ClientResponse<Value> = client.post(&path, &req).await?;
            print_response(&resp, output, fields)?;
        }
        TodoAction::Stop { workspace_id, id } => {
            // v1: stop 改用 record_id 路径参数（原 body todo_id 已废弃），
            // 路径为 POST /workspaces/{ws}/executions/{record_id}/stop。
            let path = format!("{}/executions/{}/stop", ws_prefix(*workspace_id), id);
            let resp: ClientResponse<()> = client.post(&path, &serde_json::Value::Null).await?;
            print_response(&resp, output, fields)?;
        }
        TodoAction::Stats { workspace_id, id } => {
            // v1: GET /workspaces/{ws}/todos/{id}/summary
            let path = format!("{}/todos/{}/summary", ws_prefix(*workspace_id), id);
            let resp: ClientResponse<ExecutionSummary> = client.get(&path).await?;
            print_response(&resp, output, fields)?;
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
        ExecutionAction::List { workspace_id, todo_id, status, page, limit } => {
            // v1: GET /workspaces/{ws}/executions?todo_id=...（旧 /execution-records 路径已废）
            let path = format!(
                "{}/executions?todo_id={}&page={}&limit={}{}",
                ws_prefix(*workspace_id),
                todo_id,
                page,
                limit,
                status.as_ref().map(|s| format!("&status={}", s)).unwrap_or_default()
            );
            let resp: ClientResponse<ExecutionRecordsPage> = client.get(&path).await?;
            print_response(&resp, output, fields)?;
        }
        ExecutionAction::Get { workspace_id, id } => {
            // v1: GET /workspaces/{ws}/executions/{id}
            let path = format!("{}/executions/{}", ws_prefix(*workspace_id), id);
            let resp: ClientResponse<ExecutionRecord> = client.get(&path).await?;
            print_response(&resp, output, fields)?;
        }
        ExecutionAction::Resume { workspace_id, id, message } => {
            let req = serde_json::json!({ "message": message });
            // v1: POST /workspaces/{ws}/executions/{id}/resume
            let path = format!("{}/executions/{}/resume", ws_prefix(*workspace_id), id);
            let resp: ClientResponse<Value> = client.post(&path, &req).await?;
            print_response(&resp, output, fields)?;
        }
    }
    Ok(())
}

// ============== Tag Handlers ==============
// Tag 是全局资源（无 workspace_id 列），v1 路由直接挂 /api/v1/tags，
// 不嵌 workspace 前缀；client.rs 会自动补 /api/v1。

async fn handle_tag(
    client: &ApiClient,
    action: &TagAction,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    match action {
        TagAction::List => {
            // v1: GET /tags（axum 在 nest("/api/v1/tags") + .route("/") 下，
            // 无尾斜杠的 /api/v1/tags 也能命中根路由）
            let resp: ClientResponse<Vec<Tag>> = client.get("/tags").await?;
            print_response(&resp, output, fields)?;
        }
        TagAction::Create { name, color } => {
            let req = CreateTagRequest {
                name: name.clone(),
                color: color.clone(),
            };
            // v1: POST /tags（全局资源）
            let resp: ClientResponse<Tag> = client.post("/tags", &req).await?;
            print_response(&resp, output, fields)?;
        }
        TagAction::Delete { id } => {
            // v1: DELETE /tags/{id}
            let resp: ClientResponse<()> = client.delete(&format!("/tags/{}", id)).await?;
            print_response(&resp, output, fields)?;
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
    // Dashboard 为全局运营视图，直接请求全局 stats 端点。
    let resp: ClientResponse<DashboardStats> = client.get("/api/v1/stats/dashboard").await?;
    print_response(&resp, output, fields)?;
    Ok(())
}

// ============== Blackboard Handlers ==============

async fn handle_blackboard(
    client: &ApiClient,
    action: &BlackboardAction,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    match action {
        BlackboardAction::Wiki { action, workspace_id } => {
            match action {
                WikiAction::List => {
                    // v1: wiki 独立于 blackboard 域，GET /workspaces/{ws}/wiki/files
                    let path = format!("{}/wiki/files", ws_prefix(*workspace_id));
                    let resp: ClientResponse<serde_json::Value> = client.get(&path).await?;
                    print_response(&resp, output, fields)?;
                }
                WikiAction::Get { slug } => {
                    // slug 可能包含中文或特殊字符，必须 percent-encode 后才能安全放入 URL 路径
                    let encoded = percent_encode_slug(slug);
                    // v1: wiki 独立域，不与 blackboard 嵌套
                    let path = format!("{}/wiki/files/{}", ws_prefix(*workspace_id), encoded);
                    let resp: ClientResponse<serde_json::Value> = client.get(&path).await?;
                    print_response(&resp, output, fields)?;
                }
            }
        }
    }
    Ok(())
}

// ============== Workspace Handlers ==============
// 三个子命令各拆一个独立函数，避免 handle_workspace 单函数超 30 行；
// 后端接口路径与 handlers/project_directory.rs::routes 对齐。

async fn handle_workspace(
    client: &ApiClient,
    action: &WorkspaceAction,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    match action {
        WorkspaceAction::List => list_workspaces(client, output, fields).await,
        WorkspaceAction::Create { path, name } => {
            create_workspace(client, path, name, output, fields).await
        }
    }
}

// ============== Process Handlers ==============
// 工艺 CLI：所有子命令在 handle_process 分派，每个动作拆成独立小函数（≤30 行）。
// handle_process 只做 match 路由、不掺业务逻辑（与 run_command 同构）。

/// name/guid 解析结果。Ambiguous 携带候选 guid 列表，供错误提示直接列出，AI 可复制重试。
#[derive(Debug, PartialEq)]
enum GuidResolution {
    Found(String),
    NotFound,
    Ambiguous(Vec<String>),
}

/// 纯逻辑：在已拉取的工艺模板列表里，把 name-or-guid 解析成 guid。
/// 抽出来不碰网络，便于单测覆盖 0/1/多条与 guid 直命中等分支。
///
/// 规则：guid 全局唯一，先看 key 本身是否某条 guid，命中即返回；
/// 否则按 name 匹配——0 条 NotFound、1 条返回该 guid、>1 条 Ambiguous（040 起 name 不唯一）。
fn resolve_guid_from_items(items: &[Value], key: &str) -> GuidResolution {
    // 1. guid 精确命中：guid 唯一，找到即确定，无需再判 name。
    let guid_hit = items
        .iter()
        .find(|t| t.get("guid").and_then(Value::as_str) == Some(key));
    if let Some(guid) = guid_hit.and_then(|t| t.get("guid").and_then(Value::as_str)) {
        return GuidResolution::Found(guid.to_string());
    }
    // 2. 按 name 匹配：收集所有同名项的 guid，用数量区分 NotFound/唯一/歧义。
    let hits: Vec<String> = items
        .iter()
        .filter(|t| t.get("name").and_then(Value::as_str) == Some(key))
        .filter_map(|t| t.get("guid").and_then(Value::as_str).map(str::to_string))
        .collect();
    match hits.len() {
        0 => GuidResolution::NotFound,
        1 => GuidResolution::Found(hits[0].clone()),
        _ => GuidResolution::Ambiguous(hits),
    }
}

/// 网络：GET /bundled/processes 拉全量后用纯逻辑解析。
/// 一次请求搞定，避免「guid 直查 404 → 再 list」的二次往返；
/// name 命中多条时给出候选 guid 清单，引导 AI 改用 guid 重试。
async fn resolve_process_guid(client: &ApiClient, key: &str) -> Result<String> {
    let resp: ClientResponse<Vec<Value>> = client.get("/bundled/processes").await?;
    let items = resp.data.unwrap_or_default();
    match resolve_guid_from_items(&items, key) {
        GuidResolution::Found(guid) => Ok(guid),
        GuidResolution::NotFound => Err(anyhow::anyhow!(
            "未找到工艺「{}」。用 `ntd process list` 查看可用工艺。",
            key
        )),
        GuidResolution::Ambiguous(guids) => Err(anyhow::anyhow!(
            "工艺名「{}」命中 {} 条，请改用 guid：\n  {}",
            key,
            guids.len(),
            guids.join("\n  ")
        )),
    }
}

/// 按 path 反查 workspace_id。project_directories.path 不保证唯一，取第一条匹配。
/// 抽出来供 run 复用，统一 path→id 解析口径（修复旧版 /v1/project-directories 双前缀 404）。
async fn resolve_workspace_id_by_path(client: &ApiClient, path: &str) -> Result<i64> {
    let resp: ClientResponse<Vec<Value>> = client.get("/project-directories").await?;
    let ws_id = resp
        .data
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .find_map(|ws| {
            let p = ws.get("path").and_then(Value::as_str)?;
            if p == path {
                ws.get("id").and_then(Value::as_i64)
            } else {
                None
            }
        })
        .ok_or_else(|| {
            anyhow::anyhow!("工作空间 {} 未找到，用 `ntd workspace list` 查看已注册工作空间", path)
        })?;
    Ok(ws_id)
}

#[allow(clippy::print_stdout)]
async fn handle_process(
    client: &ApiClient,
    action: &ProcessAction,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    // 纯分派：每个动作一个 process_* 小函数，本函数不掺业务逻辑（与 run_command 同构）。
    // arm 较多是工艺能力齐全的结果，但每条都是单行调用、无嵌套，符合「扁平分派」豁免。
    match action {
        ProcessAction::List { system, user } => {
            process_list(client, *system, *user, output, fields).await
        }
        ProcessAction::Show { name_or_guid } => {
            process_show(client, name_or_guid, output, fields).await
        }
        ProcessAction::Recommend { description } => {
            process_recommend(client, description, output, fields).await
        }
        ProcessAction::Create { name, display_name, category, complexity, version, file, stdin } => {
            // 7 个字段收拢成结构体再传，避免 process_create 触发 too_many_arguments。
            let args = ProcessCreateArgs {
                name: name.as_deref(),
                display_name: display_name.as_deref(),
                category: category.as_deref(),
                complexity: complexity.as_deref(),
                version: version.as_deref(),
                file: file.as_deref(),
                stdin: *stdin,
            };
            process_create(client, &args, output, fields).await
        }
        ProcessAction::Delete { name_or_guid } => {
            process_delete(client, name_or_guid, output, fields).await
        }
        ProcessAction::Run { name_or_guid, workspace } => {
            run_process_install(client, name_or_guid, workspace, output, fields).await
        }
        ProcessAction::Upgrade { name_or_guid, loop_id } => {
            process_upgrade(client, name_or_guid, *loop_id, output, fields).await
        }
        ProcessAction::Loops { name_or_guid } => {
            process_loops(client, name_or_guid, output, fields).await
        }
        ProcessAction::Versions { name_or_guid } => {
            process_versions(client, name_or_guid, output, fields).await
        }
        ProcessAction::Diff { name_or_guid, version, base } => {
            process_diff(client, name_or_guid, version, base, output, fields).await
        }
        ProcessAction::ExecutionStatus { id } => {
            query_execution_status(client, *id, output, fields).await
        }
    }
}

/// list：--system/--user 决定 ?is_system 查询参数（同传报错，二者语义互斥）。
async fn process_list(
    client: &ApiClient,
    system: bool,
    user: bool,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    let path = match (system, user) {
        (true, true) => return Err(anyhow::anyhow!("--system 与 --user 互斥，请只选其一")),
        (true, false) => "/bundled/processes?is_system=true",
        (false, true) => "/bundled/processes?is_system=false",
        (false, false) => "/bundled/processes",
    };
    let resp: ClientResponse<Vec<Value>> = client.get(path).await?;
    print_response(&resp, output, fields)?;
    Ok(())
}

/// show：先 name-or-guid → guid，再取详情。修复旧版把 name 塞进 guid 槽的 bug。
async fn process_show(
    client: &ApiClient,
    name_or_guid: &str,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    let guid = resolve_process_guid(client, name_or_guid).await?;
    let path = format!("/bundled/processes/{}", percent_encode_slug(&guid));
    let resp: ClientResponse<Value> = client.get(&path).await?;
    print_response(&resp, output, fields)?;
    Ok(())
}

/// recommend：把任务描述发给后端，返回按 score 排序的推荐工艺 + reasons。
async fn process_recommend(
    client: &ApiClient,
    description: &str,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    let body = serde_json::json!({ "description": description });
    let resp: ClientResponse<Value> = client.post("/processes/recommend", &body).await?;
    print_response(&resp, output, fields)?;
    Ok(())
}

/// Create 子命令的 7 个字段收拢成一个结构，避免 process_create 参数过多触发 too_many_arguments。
/// 借用 Create arm 解构出的 &String → &str，零拷贝传给 body 构造。
struct ProcessCreateArgs<'a> {
    name: Option<&'a str>,
    display_name: Option<&'a str>,
    category: Option<&'a str>,
    complexity: Option<&'a str>,
    version: Option<&'a str>,
    file: Option<&'a str>,
    stdin: bool,
}

/// 新建用户工艺。body 构造抽到 build_create_body，本函数保持纤薄只发请求。
async fn process_create(
    client: &ApiClient,
    args: &ProcessCreateArgs<'_>,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    let body = build_create_body(args)?;
    let resp: ClientResponse<Value> = client.post("/processes", &body).await?;
    print_response(&resp, output, fields)?;
    Ok(())
}

/// 构造新建工艺请求体。
/// 两条来源：--stdin 读全量 JSON body；否则 --name + --file(definition) 组装，可选元数据按需附上。
fn build_create_body(args: &ProcessCreateArgs<'_>) -> Result<Value> {
    if args.stdin {
        // read_stdin_json 已返回 Result<Value>，直接转发，无需 Ok(...?) 多此一举。
        return read_stdin_json();
    }
    let name = args
        .name
        .ok_or_else(|| anyhow::anyhow!("新建工艺需要 --name（或用 --stdin 传完整 body）"))?;
    let definition = read_definition_source(args.file)?;
    let mut obj = serde_json::Map::new();
    obj.insert("name".to_string(), Value::String(name.to_string()));
    obj.insert("definition".to_string(), Value::String(definition));
    // 可选元数据：只附用户显式传入的字段，避免把 null 写进 body。
    for (k, v) in [
        ("display_name", args.display_name),
        ("category", args.category),
        ("complexity", args.complexity),
        ("version", args.version),
    ] {
        if let Some(val) = v {
            obj.insert(k.to_string(), Value::String(val.to_string()));
        }
    }
    Ok(Value::Object(obj))
}

/// 读取工艺 YAML 正文来源。非 stdin 模式下 --file 必填，缺失给出可操作的错误。
fn read_definition_source(file: Option<&str>) -> Result<String> {
    match file {
        Some(p) => std::fs::read_to_string(p)
            .map_err(|e| anyhow::anyhow!("读取 YAML 文件 {} 失败: {}", p, e)),
        None => Err(anyhow::anyhow!(
            "新建工艺需要 YAML 正文：用 --file <yaml> 或 --stdin 传入"
        )),
    }
}

/// delete：name-or-guid → guid → DELETE。系统工艺或有实例 loop 时后端会拒绝。
async fn process_delete(
    client: &ApiClient,
    name_or_guid: &str,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    let guid = resolve_process_guid(client, name_or_guid).await?;
    let path = format!("/processes/{}", percent_encode_slug(&guid));
    let resp: ClientResponse<Value> = client.delete(&path).await?;
    print_response(&resp, output, fields)?;
    Ok(())
}

/// upgrade：把指定 loop 升级到工艺模板最新版。
async fn process_upgrade(
    client: &ApiClient,
    name_or_guid: &str,
    loop_id: i64,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    let guid = resolve_process_guid(client, name_or_guid).await?;
    let path = format!(
        "/processes/{}/loops/{}/upgrade",
        percent_encode_slug(&guid),
        loop_id
    );
    let resp: ClientResponse<Value> = client.post(&path, &serde_json::Value::Null).await?;
    print_response(&resp, output, fields)?;
    Ok(())
}

/// loops：列出该工艺实例化的全部 loop（含各 loop 执行次数）。
async fn process_loops(
    client: &ApiClient,
    name_or_guid: &str,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    let guid = resolve_process_guid(client, name_or_guid).await?;
    let path = format!("/processes/{}/loops", percent_encode_slug(&guid));
    let resp: ClientResponse<Value> = client.get(&path).await?;
    print_response(&resp, output, fields)?;
    Ok(())
}

/// versions：该工艺的版本历史（id/version/updated_at/source_path）。
async fn process_versions(
    client: &ApiClient,
    name_or_guid: &str,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    let guid = resolve_process_guid(client, name_or_guid).await?;
    let path = format!("/processes/{}/versions", percent_encode_slug(&guid));
    let resp: ClientResponse<Value> = client.get(&path).await?;
    print_response(&resp, output, fields)?;
    Ok(())
}

/// diff：对比 base → version 两个版本的工艺正文逐行 diff。
async fn process_diff(
    client: &ApiClient,
    name_or_guid: &str,
    version: &str,
    base: &str,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    let guid = resolve_process_guid(client, name_or_guid).await?;
    let path = format!(
        "/processes/{}/versions/{}/diff?base={}",
        percent_encode_slug(&guid),
        percent_encode_slug(version),
        percent_encode_slug(base)
    );
    let resp: ClientResponse<Value> = client.get(&path).await?;
    print_response(&resp, output, fields)?;
    Ok(())
}

/// ProcessRun：path→workspace_id → name/guid→guid → 安装 → 打印结果。
/// 线性管道：三步紧密依赖上一步、每步只调一次，拆开反而要传一堆中间参数，故按豁免保留为流水线。
#[allow(clippy::print_stdout)]
async fn run_process_install(
    client: &ApiClient,
    name_or_guid: &str,
    workspace: &str,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    let ws_id = resolve_workspace_id_by_path(client, workspace).await?;
    let guid = resolve_process_guid(client, name_or_guid).await?;
    let install_req = serde_json::json!({ "workspace_id": ws_id });
    let install_path = format!("/bundled/processes/{}/install", percent_encode_slug(&guid));
    let install_resp: ClientResponse<Value> = client.post(&install_path, &install_req).await?;
    println!("工艺模板「{}」已安装到工作空间「{}」", name_or_guid, workspace);
    if let Some(ref data) = install_resp.data {
        if let Some(loop_id) = data.get("loop_id").and_then(Value::as_i64) {
            println!("创建 Loop #{}，可用 `ntd process upgrade` 升级或在前端启用触发", loop_id);
        }
    }
    print_response(&install_resp, output, fields)?;
    Ok(())
}

/// ProcessExecutionStatus：遍历工作空间 → loop → 查找审计数据。
/// audit 端点是 workspace/loop 作用域的，CLI 不持有这层映射，只能穷举查找命中即返回。
#[allow(clippy::print_stdout)]
async fn query_execution_status(
    client: &ApiClient,
    id: i64,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    let ws_resp: ClientResponse<Vec<Value>> = client.get("/project-directories").await?;
    for ws in ws_resp.data.as_deref().unwrap_or(&[]) {
        let Some(ws_id) = ws.get("id").and_then(Value::as_i64) else { continue };
        // 修复旧版 /v1/workspaces 双前缀 404：client 已自动补 /api/v1。
        let loops_resp: ClientResponse<Vec<Value>> =
            match client.get(&format!("/workspaces/{}/loops", ws_id)).await {
                Ok(r) => r,
                Err(_) => continue,
            };
        for lp in loops_resp.data.as_deref().unwrap_or(&[]) {
            let Some(lp_id) = lp.get("id").and_then(Value::as_i64) else { continue };
            let audit_path = format!("/workspaces/{}/loops/{}/executions/{}/audit", ws_id, lp_id, id);
            if let Ok(audit_resp) = client.get::<ClientResponse<Value>>(&audit_path).await {
                if audit_resp.data.is_some() {
                    return print_response(&audit_resp, output, fields);
                }
            }
        }
    }
    println!("工艺执行 #{} 未找到。请确认 loop_execution_id 正确。", id);
    Ok(())
}

/// 调 `GET /api/v1/project-directories` 拉全部已注册工作空间。
/// 全局资源，路径不嵌 workspace。
async fn list_workspaces(
    client: &ApiClient,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    let resp: ClientResponse<Vec<ProjectDirectory>> = client.get("/project-directories").await?;
    print_response(&resp, output, fields)?;
    Ok(())
}

/// 调 `POST /api/v1/project-directories` 注册一个新工作空间。
/// body 结构与 handlers/project_directory.rs::CreateProjectDirectoryRequest 对齐。
/// 全局资源，路径不嵌 workspace。
async fn create_workspace(
    client: &ApiClient,
    path: &str,
    name: &str,
    output: &OutputFormat,
    fields: &Option<String>,
) -> Result<()> {
    // 用 serde_json::json 构造 body 而非具名 struct，避免在 models 层再开一个 DTO——
    // create 接口只有两个字段，cli 侧不会再复用，具名反而是过度设计。
    let body = serde_json::json!({ "path": path, "name": name });
    let resp: ClientResponse<ProjectDirectory> = client
        .post("/project-directories", &body)
        .await?;
    print_response(&resp, output, fields)?;
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
            // v1: List 必带 workspace 嵌入 URL（旧的可选 workspace_id 过滤已废弃）。
            let path = format!("{}/loops", ws_prefix(*workspace_id));
            let resp: ClientResponse<Vec<LoopDto>> = client.get(&path).await?;
            print_response(&resp, output, fields)?;
        }
        LoopAction::Get { workspace_id, id } => {
            // v1: GET /workspaces/{ws}/loops/{id}
            let path = format!("{}/loops/{}", ws_prefix(*workspace_id), id);
            let resp: ClientResponse<LoopDto> = client.get(&path).await?;
            print_response(&resp, output, fields)?;
        }
        LoopAction::Update { workspace_id, id, name, description, status } => {
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
            // v1: PUT /workspaces/{ws}/loops/{id}
            let path = format!("{}/loops/{}", ws_prefix(*workspace_id), id);
            let resp: ClientResponse<LoopDto> = client.put(&path, &req).await?;
            print_response(&resp, output, fields)?;
        }
        LoopAction::Delete { workspace_id, id } => {
            // v1: DELETE /workspaces/{ws}/loops/{id}
            let path = format!("{}/loops/{}", ws_prefix(*workspace_id), id);
            let resp: ClientResponse<()> = client.delete(&path).await?;
            print_response(&resp, output, fields)?;
        }
        LoopAction::Stop { workspace_id, id } => {
            // Pause the loop by disabling all its triggers
            let req = serde_json::json!({ "status": "paused" });
            // v1: PUT /workspaces/{ws}/loops/{id}/status
            let path = format!("{}/loops/{}/status", ws_prefix(*workspace_id), id);
            let resp: ClientResponse<LoopDto> = client.put(&path, &req).await?;
            print_response(&resp, output, fields)?;
        }
        LoopAction::Stats { workspace_id, id, recent } => {
            // Get loop details with recent executions combined into one response
            // v1: 两次请求都走 /workspaces/{ws}/loops/...
            let base = format!("{}/loops/{}", ws_prefix(*workspace_id), id);
            let resp: ClientResponse<LoopDto> = client.get(&base).await?;
            let execs_resp: ClientResponse<serde_json::Value> = client.get(&format!(
                "{}/executions?page=1&limit={}",
                base, recent
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
            print_response(&final_resp, output, fields)?;
        }
        // 044：LoopAction::Execute 已随触发器入口下线移除（原走 POST .../trigger）。
        LoopAction::Execution { action } => {
            match action {
                LoopExecutionAction::List { workspace_id, loop_id, page, limit } => {
                    // v1: GET /workspaces/{ws}/loops/{loop_id}/executions
                    let path = format!(
                        "{}/loops/{}/executions?page={}&limit={}",
                        ws_prefix(*workspace_id), loop_id, page, limit
                    );
                    let resp: ClientResponse<serde_json::Value> = client.get(&path).await?;
                    print_response(&resp, output, fields)?;
                }
                LoopExecutionAction::Get { workspace_id, loop_id, execution_id } => {
                    // v1 后端没有 /loop-executions/{eid} 顶层路由，
                    // 必须按 /workspaces/{ws}/loops/{loop_id}/executions/{eid} 访问。
                    let path = format!(
                        "{}/loops/{}/executions/{}",
                        ws_prefix(*workspace_id), loop_id, execution_id
                    );
                    let resp: ClientResponse<serde_json::Value> = client.get(&path).await?;
                    print_response(&resp, output, fields)?;
                }
                LoopExecutionAction::Blackboard { workspace_id, loop_id, execution_id, human } => {
                    // 复用 v1 get_execution_v1 handler 返回的 LoopExecutionDetail,
                    // 它已经按 sequence_index 升序返回 step_executions。
                    // 不新增 API 端点 — 黑板视图本质就是 step_executions 的渲染。
                    let path = format!(
                        "{}/loops/{}/executions/{}",
                        ws_prefix(*workspace_id), loop_id, execution_id
                    );
                    let resp: ClientResponse<serde_json::Value> = client.get(&path).await?;
                    if resp.code != 0 {
                        // 与 print_response 一致:错误码非 0 时抛 anyhow
                        return Err(anyhow::anyhow!("API error {}: {}", resp.code, resp.message));
                    }
                    // 先写到 Vec<u8>, 最后一次性 stdout.lock().write_all,
                    // 这样集成测试可以 capture 到完整输出, 不会出现 println!
                    // 与 JSON 字符串交错污染。
                    let mut buf: Vec<u8> = Vec::new();
                    if *human {
                        // 人类视图: 黑板文本渲染
                        render_blackboard_to(resp.data.as_ref(), &mut buf);
                    } else {
                        // 默认: JSON, 直接是 LoopExecutionDetail, AI/脚本友好
                        use std::io::Write;
                        let pretty = serde_json::to_string_pretty(&resp.data)?;
                        writeln!(buf, "{pretty}")?;
                    }
                    use std::io::Write;
                    let _ = std::io::stdout().lock().write_all(&buf);
                }
            }
        }
    }
    Ok(())
}

// ============== Output ==============

// CLI 入口：resp 按引用传入避免所有权转移，仅用于序列化输出
#[allow(clippy::print_stdout)]
fn print_response<T: serde::Serialize>(
    resp: &ClientResponse<T>,
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
            // resp 已经是引用，不需要再取引用
            let value = serde_json::to_value(resp)?;
            println!("{}", serde_json::to_string(&value)?);
        }
        OutputFormat::Pretty => {
            let value = serde_json::to_value(resp)?;
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

/// 把 LoopExecutionDetail 渲染成人类可读的黑板视图（写到 stdout）。
///
/// 输入是 `Option<&serde_json::Value>` —— None 时由调用方传入表示「API 返回
/// 的 data 字段为 null」，本函数显式处理这种情况而不是强制调用方过滤，
/// 让 dispatch 层代码更扁平。
///
/// 渲染失败（字段缺失或类型错误）时降级输出原始 JSON + 错误提示，
/// 而不是让 CLI 崩溃——黑板视图是辅助功能，不能阻塞主流程。
// 保留为单元测试的稳定 stdout 入口；生产 dispatch 已直接走 render_blackboard_to 路径。
// cfg(test)：该函数仅被本文件 tests 模块调用，非测试构建不编入二进制，避免 dead_code 告警。
#[cfg(test)]
fn render_blackboard(data: Option<&Value>) {
    let mut buf: Vec<u8> = Vec::new();
    render_blackboard_to(data, &mut buf);
    // CLI 入口把 buf 一次性 flush 到 stdout, 而不是 println! 散落到各 helper,
    // 这样测试也能通过 render_blackboard_to 抓取完整输出。
    use std::io::Write;
    let _ = std::io::stdout().write_all(&buf);
}

/// 把黑板渲染到任意 `Write` 目标，单元测试和集成测试用 `Vec<u8>` 抓取输出做断言。
/// 所有 println! 在这里都改成 writeln!，避免分散在 helper 里写死 stdout。
/// `pub` 让 `tests/blackboard_cli_tests.rs` 集成测试能复用 (集成测试是独立 crate, pub(crate) 不可见)。
pub fn render_blackboard_to<W: std::io::Write>(data: Option<&Value>, w: &mut W) {
    let Some(data) = data else {
        let _ = writeln!(w, "(无数据)");
        return;
    };

    write_blackboard_header(data, w);
    let _ = writeln!(w);

    match data.get("step_executions").and_then(Value::as_array) {
        Some(steps) if !steps.is_empty() => {
            for step in steps {
                write_blackboard_step(step, w);
            }
            let _ = writeln!(w);
            write_blackboard_footer(data, steps.len(), w);
        }
        Some(_) => {
            let _ = writeln!(w, "黑板为空（loop 尚未执行任何步骤）");
            let _ = writeln!(w);
            write_blackboard_footer(data, 0, w);
        }
        None => {
            let _ = writeln!(w, "(step_executions 字段缺失或类型错误)");
            let _ = writeln!(w, "\n原始数据:\n{}", serde_json::to_string_pretty(data).unwrap_or_default());
        }
    }
}

/// 渲染黑板头部：循环名、触发信息、状态、时间。
/// 字段全部缺失时降级为占位符，不影响主流程。
fn write_blackboard_header<W: std::io::Write>(data: &Value, w: &mut W) {
    let exec_id = data.get("id").and_then(Value::as_i64).unwrap_or(0);
    let loop_name = data.get("loop_name").and_then(Value::as_str).unwrap_or("?");
    let trigger_meta = data.get("trigger_meta").and_then(Value::as_str).unwrap_or("");
    let status = data.get("status").and_then(Value::as_str).unwrap_or("unknown");
    let total = data.get("total_steps").and_then(Value::as_i64).unwrap_or(0);
    let completed = data.get("completed_steps").and_then(Value::as_i64).unwrap_or(0);
    let started = data.get("started_at").and_then(Value::as_str).unwrap_or("");
    let finished = data.get("finished_at").and_then(Value::as_str).unwrap_or("");

    let _ = writeln!(w, "═══ Loop Execution #{exec_id} ────────────────────────────────");
    let _ = writeln!(w, "循环: {loop_name}");
    if !trigger_meta.is_empty() && trigger_meta != "{}" {
        let _ = writeln!(w, "触发: {trigger_meta}");
    }
    let _ = writeln!(
        w,
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
        let _ = writeln!(w, "开始: {started}{end_part}");
    }
}

/// 渲染黑板尾部：步骤总数 + Token 汇总（如果有）。
/// Token 汇总来自 LoopExecutionDetail.token_summary，与 step_executions 平级。
fn write_blackboard_footer<W: std::io::Write>(data: &Value, step_count: usize, w: &mut W) {
    if let Some(ts) = data.get("token_summary") {
        let ti = ts.get("total_input_tokens").and_then(Value::as_i64).unwrap_or(0);
        let to = ts.get("total_output_tokens").and_then(Value::as_i64).unwrap_or(0);
        let _ = writeln!(w, "═══ {} 步 / Token: 输入 {} 输出 {} ════════════════════════", step_count, ti, to);
    } else {
        let _ = writeln!(w, "═══ {} 步 ═══════════════════════════════════════════════════", step_count);
    }
}

/// 渲染单个 step 块（标题行 + exec id + 多行结论）。
/// 字段名与 `LoopStepExecutionDto` 一致（见 `backend/src/models/loop_.rs`）。
fn write_blackboard_step<W: std::io::Write>(step: &Value, w: &mut W) {
    let header = format_step_header(step);
    let _ = writeln!(w, "  {header}");
    let exec_id = step
        .get("execution_record_id")
        .and_then(Value::as_i64)
        .map(|r| format!("#{r}"))
        .unwrap_or_else(|| "-".to_string());
    let _ = writeln!(w, "     exec: {exec_id}");
    write_step_body(step, w);
}

/// 格式化 step 标题行：`#<seq> <icon> <name(padded to 22)> 评分 <rating>`。
/// 名字用 display-width-aware 截断，避免中文字符把对齐打乱。
fn format_step_header(step: &Value) -> String {
    let seq = step.get("sequence_index").and_then(Value::as_i64).unwrap_or(0);
    let status = step.get("status").and_then(Value::as_str).unwrap_or("unknown");
    let rating = step
        .get("rating")
        .and_then(Value::as_i64)
        .map(|r| r.to_string())
        .unwrap_or_else(|| "-".to_string());
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
    // 按终端显示宽度截断（中文/Emoji 按 2 计算），并 pad 空格让「评分」列对齐。
    let padded = truncate_to_width(&step_name, 22);
    format!(
        "#{seq} {} {:<22} 评分 {rating}",
        status_icon(status),
        padded,
    )
}

/// 写 step 正文：结论 / 错误 / 待审批意见。
/// 优先级：pending_approval 的 approval_comment > error_message > conclusion > (无结论)。
fn write_step_body<W: std::io::Write>(step: &Value, w: &mut W) {
    let status = step.get("status").and_then(Value::as_str).unwrap_or("unknown");
    if status == "pending_approval" {
        if let Some(comment) = step.get("approval_comment").and_then(Value::as_str) {
            if !comment.is_empty() {
                let _ = writeln!(w, "     待审批意见: {comment}");
            }
        }
        let _ = writeln!(w, "     (等待人工审批)");
        return;
    }
    if let Some(err) = step.get("error_message").and_then(Value::as_str) {
        if !err.is_empty() {
            let _ = writeln!(w, "     失败: {err}");
        }
    }
    match step.get("conclusion").and_then(Value::as_str) {
        Some(c) if !c.is_empty() => {
            // 多行结论：保留缩进让层级清晰
            for line in c.lines() {
                let _ = writeln!(w, "     {line}");
            }
        }
        _ => {
            let _ = writeln!(w, "     (无结论)");
        }
    }
}

/// 截断字符串到指定「终端显示宽度」（display width），按 char 边界安全处理 UTF-8。
/// ASCII 按 1 计算宽度，CJK（>= U+0080）按 2。
/// 注意：emoji 的实际宽度因字体而异，这里按统一近似计算；
/// 真正的 terminal width 需要 unicode-width crate，但对黑板视图而言够用。
///
/// 输出长度恒等于 max_width：不足时右补空格（让对齐列就位），超出时截断到
/// `max_width - 1` 个 width 后追加 `…` 占第 max_width 个 width。
fn truncate_to_width(s: &str, max_width: usize) -> String {
    // 第一遍: 计算 s 的真实 display width
    let total: usize = s
        .chars()
        .map(|c| if (c as u32) < 0x80 { 1 } else { 2 })
        .sum();
    if total <= max_width {
        // 短于阈值: 原样 + 补空格到 max_width
        let mut out = String::with_capacity(max_width);
        out.push_str(s);
        for _ in 0..(max_width - total) {
            out.push(' ');
        }
        return out;
    }
    // 超长: 截到 max_width - 1 个 width 的字符 + … = max_width
    let mut out = String::with_capacity(max_width);
    let mut w = 0usize;
    let target = max_width.saturating_sub(1); // 留给 … 的位置
    for c in s.chars() {
        let cw = if (c as u32) < 0x80 { 1 } else { 2 };
        if w + cw > target {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('…');
    out
}

// ============== Tests ==============

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
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
        // v1: list 需要 --workspace-id（workspace 嵌入 URL）
        let cli = Cli::try_parse_from(["ntd", "-o", "raw", "todo", "list", "--workspace-id", "1"]).unwrap();
        assert_eq!(cli.output, OutputFormat::Raw);
    }

    #[test]
    fn test_cli_parse_fields() {
        // v1: list 需要 --workspace-id
        let cli = Cli::try_parse_from(["ntd", "-f", "id,title", "todo", "list", "--workspace-id", "1"]).unwrap();
        assert_eq!(cli.fields, Some("id,title".to_string()));
    }

    #[test]
    fn test_cli_parse_search() {
        // v1: list 需要 --workspace-id
        let cli = Cli::try_parse_from(["ntd", "todo", "list", "--workspace-id", "1", "-s", "rust"]).unwrap();
        match cli.command {
            Commands::Todo { action: TodoAction::List { search, .. } } => {
                assert_eq!(search, Some("rust".to_string()));
            }
            _ => panic!("Expected Todo::List with search"),
        }
    }

    #[test]
    fn test_cli_parse_stdin_create() {
        // v1: --workspace-id 必填
        let cli = Cli::try_parse_from(["ntd", "todo", "create", "--workspace-id", "1", "--stdin"]).unwrap();
        match cli.command {
            Commands::Todo { action: TodoAction::Create { stdin, .. } } => {
                assert!(stdin);
            }
            _ => panic!("Expected Todo::Create with stdin"),
        }
    }

    #[test]
    fn test_cli_parse_stdin_update() {
        // v1: --workspace-id 必填
        let cli = Cli::try_parse_from(["ntd", "todo", "update", "--workspace-id", "1", "1", "--stdin"]).unwrap();
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
        // v1: --workspace-id 必填（CLI 解析阶段强制）
        let cli = Cli::try_parse_from(["ntd", "todo", "create", "--workspace-id", "1"]).unwrap();
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
            "--workspace-id", "1",
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

    // workspace list：无参数子命令，parse 后应落到 WorkspaceAction::List
    #[test]
    fn test_cli_parse_workspace_list() {
        let cli = Cli::try_parse_from(["ntd", "workspace", "list"]).unwrap();
        match cli.command {
            Commands::Workspace { action: WorkspaceAction::List } => {}
            _ => panic!("Expected Workspace::List"),
        }
    }

    // workspace create：-p/--path + -n/--name 两个必填 short flag
    #[test]
    fn test_cli_parse_workspace_create() {
        let cli = Cli::try_parse_from([
            "ntd", "workspace", "create",
            "-p", "/tmp/proj-a",
            "-n", "proj-a",
        ]).unwrap();
        match cli.command {
            Commands::Workspace { action: WorkspaceAction::Create { path, name } } => {
                assert_eq!(path, "/tmp/proj-a");
                assert_eq!(name, "proj-a");
            }
            _ => panic!("Expected Workspace::Create with path and name"),
        }
    }

    #[test]
    fn test_cli_parse_execution_resume() {
        // v1: resume 路径嵌入 workspace，必填 --workspace-id
        let cli = Cli::try_parse_from(["ntd", "todo", "execution", "resume", "--workspace-id", "1", "42"]).unwrap();
        match cli.command {
            Commands::Todo { action: TodoAction::Execution { action: ExecutionAction::Resume { id, message, .. } } } => {
                assert_eq!(id, 42);
                assert!(message.is_none());
            }
            _ => panic!("Expected Todo::Execution::Resume"),
        }
    }

    #[test]
    fn test_cli_parse_execution_resume_with_message() {
        // v1: 同上，--workspace-id 必填
        let cli = Cli::try_parse_from(["ntd", "todo", "execution", "resume", "--workspace-id", "1", "42", "-m", "fix the bug"]).unwrap();
        match cli.command {
            Commands::Todo { action: TodoAction::Execution { action: ExecutionAction::Resume { id, message, .. } } } => {
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
        // v1: --workspace-id 和 --loop-id 必填（v1 路径 /workspaces/{ws}/loops/{loop_id}/executions/{eid}）
        let cli = Cli::try_parse_from([
            "ntd", "loop", "execution", "blackboard",
            "--workspace-id", "1", "--loop-id", "2", "42",
        ]).unwrap();
        match cli.command {
            Commands::Loop { action: LoopAction::Execution { action: LoopExecutionAction::Blackboard { execution_id, human, .. } } } => {
                assert_eq!(execution_id, 42);
                assert!(!human, "默认应输出 JSON，human=false");
            }
            _ => panic!("Expected Loop::Execution::Blackboard"),
        }
    }

    #[test]
    fn test_cli_parse_loop_execution_blackboard_human() {
        // --human 开关: 启用人类可读黑板视图
        // v1: 同上，必填 --workspace-id + --loop-id
        let cli = Cli::try_parse_from([
            "ntd", "loop", "execution", "blackboard",
            "--workspace-id", "1", "--loop-id", "2", "42", "--human",
        ]).unwrap();
        match cli.command {
            Commands::Loop { action: LoopAction::Execution { action: LoopExecutionAction::Blackboard { execution_id, human, .. } } } => {
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
    fn test_truncate_to_width_short() {
        // 短于阈值: 原样返回 + 补 pad 空格到 max_width
        assert_eq!(truncate_to_width("hello", 10), "hello     ");
        // 中文按 2 计算宽度, 4 个中文 = 8, 小于 10, pad 2 个空格
        assert_eq!(truncate_to_width("中文测试", 10), "中文测试  ");
    }

    #[test]
    fn test_truncate_to_width_ascii_overflow() {
        // 超长 ASCII 截断: 末尾加 … 占第 max_width 位, 总长 = max_width
        let s = "this is a very long step name that exceeds limit";
        let out = truncate_to_width(s, 10);
        // 9 个 ASCII + … = 10 字符, 总长 == max_width
        assert_eq!(out, "this is a…");
        assert_eq!(out.chars().count(), 10);
    }

    #[test]
    fn test_truncate_to_width_cjk_safe() {
        // 中文按 2 计算宽度, 「中文abcdefghij」: 中(2)文(2)a(1)b(1)c(1)d(1)e(1)f(1)g(1)h(1)i(1)j(1) = 13
        // 截断到 max_width=5: 留 1 个位置给 …, 所以填充 4 个 width 的字符 = 「中文」 (4), 再 + … = 5
        let out = truncate_to_width("中文abcdefghij", 5);
        assert_eq!(out, "中文…");
    }

    #[test]
    fn test_truncate_to_width_exact() {
        // 宽度刚好等于 max_width 时, 不截断也不加 …
        assert_eq!(truncate_to_width("hello", 5), "hello");
        // 「中文ab」= 2+2+1+1=6, max_width=6, 刚好填满
        assert_eq!(truncate_to_width("中文ab", 6), "中文ab");
    }

    /// 截断到指定 display width 的 helper 测试。
    #[test]
    fn test_render_blackboard_none() {
        // data 为 None 时输出降级提示, 不 panic
        // 内部只 println, 不返回 String, 此测试只验不崩溃
        render_blackboard(None);
    }

    #[test]
    fn test_render_blackboard_normal() {
        // 正常 3 step 全 success: 不 panic
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
                {"sequence_index": 2, "step_id": 2, "step_name": "补充单元测试", "status": "success", "rating": 90, "execution_record_id": 1025, "conclusion": "新增 12 个测试用例"},
                {"sequence_index": 3, "step_id": 3, "step_name": "更新 README", "status": "success", "rating": 75, "execution_record_id": 1026, "conclusion": "更新了安装步骤"}
            ],
            "token_summary": {"total_input_tokens": 12000, "total_output_tokens": 5000}
        });
        render_blackboard(Some(&data));
    }

    #[test]
    fn test_render_blackboard_normal_assert_output() {
        // 把 render_blackboard 的输出抓到字符串, 断言关键片段, 防止回归。
        let data = json!({
            "id": 42,
            "loop_name": "L",
            "status": "success",
            "total_steps": 1,
            "completed_steps": 1,
            "step_executions": [
                {"sequence_index": 1, "step_id": 1, "step_name": "S1", "status": "success", "rating": 85, "execution_record_id": 1024, "conclusion": "ok"}
            ]
        });
        let out = render_blackboard_to_string(Some(&data));
        // 头部: 包含 exec id 和循环名
        assert!(out.contains("Loop Execution #42"), "missing exec id header: {out}");
        assert!(out.contains("循环: L"), "missing loop name: {out}");
        // 状态行: 包含图标和进度
        assert!(out.contains("✅"), "missing success icon: {out}");
        assert!(out.contains("完成 1/1 步"), "missing progress: {out}");
        // step 标题: 序号 + 图标 + 评分
        assert!(out.contains("#1"), "missing seq: {out}");
        assert!(out.contains("评分 85"), "missing rating: {out}");
        // exec 行
        assert!(out.contains("exec: #1024"), "missing exec id: {out}");
        // 结论多行
        assert!(out.contains("ok"), "missing conclusion: {out}");
    }

    #[test]
    fn test_render_blackboard_failed_assert_output() {
        // failed: 有 error_message 但无 conclusion 时, error_message 替代结论
        let data = json!({
            "id": 1,
            "loop_name": "L",
            "status": "failed",
            "total_steps": 1,
            "completed_steps": 0,
            "step_executions": [
                {"sequence_index": 1, "step_id": 1, "step_name": "失败步骤", "status": "failed", "error_message": "执行超时", "conclusion": null, "execution_record_id": null}
            ]
        });
        let out = render_blackboard_to_string(Some(&data));
        assert!(out.contains("❌"), "missing failed icon: {out}");
        assert!(out.contains("失败: 执行超时"), "missing error message: {out}");
        assert!(out.contains("(无结论)"), "missing fallback conclusion: {out}");
        assert!(out.contains("exec: -"), "missing record id fallback: {out}");
    }

    #[test]
    fn test_render_blackboard_pending_approval_assert_output() {
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
        let out = render_blackboard_to_string(Some(&data));
        assert!(out.contains("🤔"), "missing pending_approval icon: {out}");
        assert!(out.contains("待审批意见: 请确认改动"), "missing approval comment: {out}");
        assert!(out.contains("(等待人工审批)"), "missing pending hint: {out}");
    }

    #[test]
    fn test_render_blackboard_missing_step_executions_assert_output() {
        let data = json!({"id": 1, "loop_name": "L", "status": "running"});
        let out = render_blackboard_to_string(Some(&data));
        assert!(out.contains("(step_executions 字段缺失或类型错误)"), "missing fallback msg: {out}");
        assert!(out.contains("原始数据:"), "missing raw data dump: {out}");
    }

    #[test]
    fn test_render_blackboard_empty_assert_output() {
        let data = json!({
            "id": 1, "loop_name": "L", "status": "pending",
            "total_steps": 0, "completed_steps": 0,
            "step_executions": []
        });
        let out = render_blackboard_to_string(Some(&data));
        assert!(out.contains("黑板为空"), "missing empty hint: {out}");
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
        let out = render_blackboard_to_string(Some(&data));
        assert!(out.contains("exec: -"), "expected exec: - fallback: {out}");
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
        let out = render_blackboard_to_string(Some(&data));
        assert!(out.contains("异常处理"), "missing anomaly handler name: {out}");
    }

    // ===== Process CLI 解析测试 =====
    // 每个新子命令一条 try_parse_from 断言，沿用现有 test_cli_parse_* 风格，
    // 确保命令面/flag/位置参数与设计文档一致。

    #[test]
    fn test_cli_parse_process_list_default() {
        // 无过滤：列出全部工艺
        let cli = Cli::try_parse_from(["ntd", "process", "list"]).unwrap();
        match cli.command {
            Commands::Process { action: ProcessAction::List { system, user } } => {
                assert!(!system && !user);
            }
            _ => panic!("Expected Process::List"),
        }
    }

    #[test]
    fn test_cli_parse_process_list_filters() {
        // --system / --user 两个互斥过滤开关各解析一次
        let cli = Cli::try_parse_from(["ntd", "process", "list", "--system"]).unwrap();
        match cli.command {
            Commands::Process { action: ProcessAction::List { system, user } } => {
                assert!(system && !user);
            }
            _ => panic!("Expected Process::List --system"),
        }
        let cli = Cli::try_parse_from(["ntd", "process", "list", "--user"]).unwrap();
        match cli.command {
            Commands::Process { action: ProcessAction::List { system, user } } => {
                assert!(!system && user);
            }
            _ => panic!("Expected Process::List --user"),
        }
    }

    #[test]
    fn test_cli_parse_process_show() {
        // show 接受 name 或 guid（运行期解析，CLI 层只校验位置参数到位）
        let cli = Cli::try_parse_from(["ntd", "process", "show", "4p12s-delivery"]).unwrap();
        match cli.command {
            Commands::Process { action: ProcessAction::Show { name_or_guid } } => {
                assert_eq!(name_or_guid, "4p12s-delivery");
            }
            _ => panic!("Expected Process::Show"),
        }
    }

    #[test]
    fn test_cli_parse_process_recommend() {
        // recommend：位置参数是任务描述，含中文/空格也应以单参传入
        let cli =
            Cli::try_parse_from(["ntd", "process", "recommend", "Rust 持续交付流水线"]).unwrap();
        match cli.command {
            Commands::Process { action: ProcessAction::Recommend { description } } => {
                assert_eq!(description, "Rust 持续交付流水线");
            }
            _ => panic!("Expected Process::Recommend"),
        }
    }

    #[test]
    fn test_cli_parse_process_create_file() {
        // 非 stdin：--name 必填 + 可选元数据 + --file 读 YAML
        let cli = Cli::try_parse_from([
            "ntd", "process", "create",
            "--name", "my-delivery",
            "--display-name", "我的交付",
            "--category", "devops",
            "--complexity", "high",
            "--version", "1.0.0",
            "--file", "/tmp/delivery.yaml",
        ])
        .unwrap();
        match cli.command {
            Commands::Process {
                action:
                    ProcessAction::Create {
                        name, display_name, category, complexity, version, file, stdin,
                    },
            } => {
                assert_eq!(name.as_deref(), Some("my-delivery"));
                assert_eq!(display_name.as_deref(), Some("我的交付"));
                assert_eq!(category.as_deref(), Some("devops"));
                assert_eq!(complexity.as_deref(), Some("high"));
                assert_eq!(version.as_deref(), Some("1.0.0"));
                assert_eq!(file.as_deref(), Some("/tmp/delivery.yaml"));
                assert!(!stdin);
            }
            _ => panic!("Expected Process::Create"),
        }
    }

    #[test]
    fn test_cli_parse_process_create_stdin() {
        // --stdin 模式：name 可省略，body 从 stdin 读
        let cli = Cli::try_parse_from(["ntd", "process", "create", "--stdin"]).unwrap();
        match cli.command {
            Commands::Process { action: ProcessAction::Create { stdin, name, .. } } => {
                assert!(stdin);
                assert!(name.is_none());
            }
            _ => panic!("Expected Process::Create --stdin"),
        }
    }

    #[test]
    fn test_cli_parse_process_delete() {
        let cli = Cli::try_parse_from(["ntd", "process", "delete", "abc-123-guid"]).unwrap();
        match cli.command {
            Commands::Process { action: ProcessAction::Delete { name_or_guid } } => {
                assert_eq!(name_or_guid, "abc-123-guid");
            }
            _ => panic!("Expected Process::Delete"),
        }
    }

    #[test]
    fn test_cli_parse_process_run() {
        // run：位置 name-or-guid + --workspace 路径
        let cli = Cli::try_parse_from([
            "ntd", "process", "run", "4p12s-delivery", "--workspace", "/tmp/proj",
        ])
        .unwrap();
        match cli.command {
            Commands::Process { action: ProcessAction::Run { name_or_guid, workspace } } => {
                assert_eq!(name_or_guid, "4p12s-delivery");
                assert_eq!(workspace, "/tmp/proj");
            }
            _ => panic!("Expected Process::Run"),
        }
    }

    #[test]
    fn test_cli_parse_process_upgrade() {
        let cli =
            Cli::try_parse_from(["ntd", "process", "upgrade", "my-proc", "--loop-id", "7"]).unwrap();
        match cli.command {
            Commands::Process { action: ProcessAction::Upgrade { name_or_guid, loop_id } } => {
                assert_eq!(name_or_guid, "my-proc");
                assert_eq!(loop_id, 7);
            }
            _ => panic!("Expected Process::Upgrade"),
        }
    }

    #[test]
    fn test_cli_parse_process_loops() {
        let cli = Cli::try_parse_from(["ntd", "process", "loops", "my-proc"]).unwrap();
        match cli.command {
            Commands::Process { action: ProcessAction::Loops { name_or_guid } } => {
                assert_eq!(name_or_guid, "my-proc");
            }
            _ => panic!("Expected Process::Loops"),
        }
    }

    #[test]
    fn test_cli_parse_process_versions() {
        let cli = Cli::try_parse_from(["ntd", "process", "versions", "my-proc"]).unwrap();
        match cli.command {
            Commands::Process { action: ProcessAction::Versions { name_or_guid } } => {
                assert_eq!(name_or_guid, "my-proc");
            }
            _ => panic!("Expected Process::Versions"),
        }
    }

    #[test]
    fn test_cli_parse_process_diff() {
        // diff：位置 name-or-guid + 位置目标版本 + --base 基准版本
        let cli =
            Cli::try_parse_from(["ntd", "process", "diff", "my-proc", "1.2.0", "--base", "1.1.0"])
                .unwrap();
        match cli.command {
            Commands::Process { action: ProcessAction::Diff { name_or_guid, version, base } } => {
                assert_eq!(name_or_guid, "my-proc");
                assert_eq!(version, "1.2.0");
                assert_eq!(base, "1.1.0");
            }
            _ => panic!("Expected Process::Diff"),
        }
    }

    #[test]
    fn test_cli_parse_process_execution_status() {
        // execution-status：保留命令，确认 kebab-case 子命令名可解析
        let cli = Cli::try_parse_from(["ntd", "process", "execution-status", "42"]).unwrap();
        match cli.command {
            Commands::Process { action: ProcessAction::ExecutionStatus { id } } => {
                assert_eq!(id, 42);
            }
            _ => panic!("Expected Process::ExecutionStatus"),
        }
    }

    // ===== name/guid 解析纯逻辑测试 =====
    // resolve_guid_from_items 不碰网络，直接喂数据断言 0/1/多条与优先级分支。

    /// 构造一份模拟的 GET /bundled/processes data，覆盖唯一 name、guid 直命中、同名多条。
    fn sample_process_items() -> Vec<Value> {
        vec![
            json!({ "id": 1, "guid": "11111111-1111-1111-1111-111111111111", "name": "alpha" }),
            json!({ "id": 2, "guid": "22222222-2222-2222-2222-222222222222", "name": "beta" }),
            // 同名 gamma 两条：模拟 040 后 name 不唯一的歧义场景
            json!({ "id": 3, "guid": "33333333-3333-3333-3333-333333333333", "name": "gamma" }),
            json!({ "id": 4, "guid": "44444444-4444-4444-4444-444444444444", "name": "gamma" }),
        ]
    }

    #[test]
    fn test_resolve_guid_from_items_by_guid() {
        // 传 guid 直接命中，唯一确定
        let items = sample_process_items();
        let key = "22222222-2222-2222-2222-222222222222";
        assert_eq!(resolve_guid_from_items(&items, key), GuidResolution::Found(key.to_string()));
    }

    #[test]
    fn test_resolve_guid_from_items_by_name_unique() {
        // name 唯一命中：返回该条的 guid
        let items = sample_process_items();
        match resolve_guid_from_items(&items, "beta") {
            GuidResolution::Found(g) => assert_eq!(g, "22222222-2222-2222-2222-222222222222"),
            other => panic!("expected Found, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_guid_from_items_by_name_ambiguous() {
        // name 命中多条：返回 Ambiguous 并携带全部候选 guid，供错误提示列出
        let items = sample_process_items();
        match resolve_guid_from_items(&items, "gamma") {
            GuidResolution::Ambiguous(g) => {
                assert_eq!(g.len(), 2);
                assert!(g.contains(&"33333333-3333-3333-3333-333333333333".to_string()));
                assert!(g.contains(&"44444444-4444-4444-4444-444444444444".to_string()));
            }
            other => panic!("expected Ambiguous, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_guid_from_items_not_found() {
        let items = sample_process_items();
        assert_eq!(resolve_guid_from_items(&items, "nope"), GuidResolution::NotFound);
    }

    #[test]
    fn test_resolve_guid_from_items_guid_priority_over_name() {
        // 极端：某条 name 恰好等于另一条的 guid 字符串时，guid 直命中应优先、不误判为 name
        let items = vec![
            json!({ "guid": "special-guid", "name": "alpha" }),
            json!({ "guid": "real-guid", "name": "special-guid" }),
        ];
        assert_eq!(
            resolve_guid_from_items(&items, "special-guid"),
            GuidResolution::Found("special-guid".to_string())
        );
    }

    /// 写一段内容到系统临时目录的固定文件，返回路径供 build_create_body 测 --file 来源。
    /// 不用 tempfile 的 guard（drop 即删），改用固定路径 + 测试结束手动清理，避免生命周期问题。
    fn temp_yaml_with(content: &str) -> String {
        let path = std::env::temp_dir().join("ntd_cli_test_def.yaml");
        std::fs::write(&path, content).expect("write temp file");
        path.to_string_lossy().to_string()
    }

    #[test]
    fn test_build_create_body_name_and_file() {
        // 非 stdin：name + 文件正文组装，可选元数据按需附上，未传字段不应出现
        let tmp = temp_yaml_with("process: dummy");
        let args = ProcessCreateArgs {
            name: Some("my-proc"),
            display_name: Some("显示名"),
            category: None,
            complexity: None,
            version: Some("2.0.0"),
            file: Some(&tmp),
            stdin: false,
        };
        let body = build_create_body(&args).unwrap();
        assert_eq!(body["name"], "my-proc");
        assert_eq!(body["definition"], "process: dummy");
        assert_eq!(body["display_name"], "显示名");
        assert_eq!(body["version"], "2.0.0");
        // 未传字段不应落进 body，避免把 null 写给后端
        assert!(body.get("category").is_none());
        assert!(body.get("complexity").is_none());
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn test_build_create_body_requires_name_without_stdin() {
        // 非 stdin 且无 --name：给出可操作错误，而不是静默构造空 name
        let args = ProcessCreateArgs {
            name: None,
            display_name: None,
            category: None,
            complexity: None,
            version: None,
            file: None,
            stdin: false,
        };
        let err = build_create_body(&args).unwrap_err();
        assert!(err.to_string().contains("--name"), "应提示 --name: {}", err);
    }

    #[test]
    fn test_build_create_body_requires_file_without_stdin() {
        // 有 name 但无 --file 且非 stdin：提示用 --file 或 --stdin
        let args = ProcessCreateArgs {
            name: Some("my-proc"),
            display_name: None,
            category: None,
            complexity: None,
            version: None,
            file: None,
            stdin: false,
        };
        let err = build_create_body(&args).unwrap_err();
        assert!(err.to_string().contains("--file"), "应提示 --file: {}", err);
    }

    /// 把 render_blackboard 的全部输出收集到 String，便于测试断言关键片段。
    /// 底层走 render_blackboard_to(Vec<u8>)，避免直接 stdout 抓取（Rust 没有
    /// 标准库的 stdout redirect API）。
    fn render_blackboard_to_string(data: Option<&Value>) -> String {
        let mut buf: Vec<u8> = Vec::new();
        render_blackboard_to(data, &mut buf);
        String::from_utf8(buf).unwrap_or_default()
    }
}
