use std::sync::Arc;
use clap::{Parser, Subcommand};
use tokio::sync::broadcast;
use tracing::info;

use std::path::PathBuf;
use ntd::service_context::ServiceContext;
use ntd::{adapters, cli, daemon, db, handlers, scheduler::TodoScheduler, task_manager::TaskManager};
use ntd::NtdSkills;

/// ntd - Nothing Todo
#[derive(Parser)]
#[command(name = "ntd", about = "AI Todo CLI", version)]
struct Cli {
    /// API server URL (default: from ~/.ntd/config.yaml, or http://localhost:8088)
    #[arg(long)]
    server: Option<String>,

    /// Output format
    #[arg(short, long, default_value = "json", value_enum)]
    output: cli::OutputFormat,

    /// Select fields to output (comma-separated, e.g. "id,title,status")
    #[arg(short, long)]
    fields: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show version info
    Version,
    /// Upgrade ntd to the latest version via npm
    Upgrade,
    /// Start the API server
    Server {
        #[command(subcommand)]
        action: ServerAction,
    },
    /// Todo management
    Todo {
        #[command(subcommand)]
        action: cli::TodoAction,
    },
    /// Loop management
    Loop {
        #[command(subcommand)]
        action: cli::LoopAction,
    },
    /// Tag management
    Tag {
        #[command(subcommand)]
        action: cli::TagAction,
    },
    /// Global statistics
    Stats,
    /// Manage ntd daemon service (install/uninstall/start/stop/restart/status)
    Daemon {
        #[command(subcommand)]
        action: daemon::DaemonAction,
    },
    /// Manage ntd usage skills for AI executors
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },
}

#[derive(Subcommand)]
enum SkillAction {
    /// Install ntd usage skills to executor skill directories (e.g. ~/.claude/skills/ntd-usage/)
    Install {
        /// Force reinstall even if already installed
        #[arg(short, long)]
        force: bool,
        /// Only install for specific executors (comma-separated, e.g. "claudecode,atomcode")
        #[arg(short, long)]
        executor: Option<String>,
    },
}

#[derive(Subcommand)]
enum ServerAction {
    /// Start the API server
    Start {
        /// Port to listen on (default: from ~/.ntd/config.yaml, or 8088)
        #[arg(short, long)]
        port: Option<u16>,
    },
}

#[tokio::main]
async fn main() {
    // Initialize tracing early so every code path (CLI subcommands, server startup,
    // Upgrade/Skill helpers) can emit structured logs through `tracing` instead of
    // raw `eprintln!`. We use `try_init` so test binaries that already set up a
    // subscriber don't panic with "global default subscriber already set".
    //
    // 输出目标固定为 stderr：CLI 模式下 stdout 仍保留给 `println!` 的用户面消息
    // （如 "Upgrading ntd..." / "Installed for N executor(s)"），日志通道与输出通道
    // 解耦。RUST_LOG 可覆盖默认 info 级别。
    init_tracing();

    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Version) => {
            println!("ntd {}", env!("CARGO_PKG_VERSION"));
            println!("git: {}", option_env!("VERGEN_GIT_SHA").unwrap_or("unknown"));
            if let Some(desc) = option_env!("VERGEN_GIT_DESCRIBE") {
                println!("tag: {}", desc);
            }
            return;
        }
        Some(Commands::Upgrade) => {
            // CLI 升级：npm 安装新版本 → 重新部署 daemon 服务
            // 与 Web API 一键更新保持一致的升级路径
            println!("Upgrading ntd...");

            // 检测 npm 全局目录写权限，不可写时回退到 ~/.npm-global
            let prefix = ntd::npm_utils::get_npm_global_prefix();

            // 使用 --prefix 安装到有写权限的目录，避免 EACCES
            // npm 不存在时给明确提示（Issue #495 关注点：避免 panic）。
            let status = match std::process::Command::new("npm")
                .args([
                    "install",
                    "-g",
                    &format!("--prefix={}", prefix),
                    "@weibaohui/nothing-todo@latest",
                ])
                .status()
            {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to run npm ({}). Is npm installed and on PATH?", e);
                    std::process::exit(1);
                }
            };
            if !status.success() {
                // 升级 npm 失败属于「CLI 子命令不可恢复错误」，走 tracing::error 让结构化日志
                // 携带 target/module 信息，便于通过 RUST_LOG 过滤定位；进程随即以非零状态退出。
                tracing::error!("Upgrade failed.");
                std::process::exit(1);
            }

            // npm 升级成功，查找新安装的 ntd 路径
            let ntd_cmd = ntd::npm_utils::find_ntd_binary(&prefix);

            // 重新部署 daemon：stop → uninstall → install --force → start
            // 保持与 Web API 升级路径一致
            println!("Redeploying daemon service...");

            // stop：失败不阻断（服务可能已停止）
            let _ = std::process::Command::new(&ntd_cmd)
                .args(["daemon", "stop"])
                .status();

            // uninstall：失败则终止，因为旧配置未清除可能导致冲突
            let uninstall_result = std::process::Command::new(&ntd_cmd)
                .args(["daemon", "uninstall"])
                .status();
            match &uninstall_result {
                Ok(s) if s.success() => {}
                Ok(s) => {
                    tracing::error!("Daemon uninstall failed with exit code {}", s);
                    std::process::exit(1);
                }
                Err(e) => {
                    tracing::error!("Daemon uninstall exec error: {}", e);
                    std::process::exit(1);
                }
            }

            // install --force：必须传 --force 以覆盖已有配置，更新 binary 路径
            let install_result = std::process::Command::new(&ntd_cmd)
                .args(["daemon", "install", "--force"])
                .status();
            match &install_result {
                Ok(s) if s.success() => {}
                Ok(s) => {
                    tracing::error!("Daemon install failed with exit code {}", s);
                    std::process::exit(1);
                }
                Err(e) => {
                    tracing::error!("Daemon install exec error: {}", e);
                    std::process::exit(1);
                }
            }

            // start：启动新版本服务
            let start_result = std::process::Command::new(&ntd_cmd)
                .args(["daemon", "start"])
                .status();
            match &start_result {
                Ok(s) if s.success() => println!("Upgrade completed successfully!"),
                Ok(s) => {
                    tracing::error!("Daemon start failed with exit code {}. Please run 'ntd daemon start' manually.", s);
                    std::process::exit(1);
                }
                Err(e) => {
                    tracing::error!("Daemon start exec error: {}. Please run 'ntd daemon start' manually.", e);
                    std::process::exit(1);
                }
            }

            return;
        }
        Some(Commands::Server { action: ServerAction::Start { port } }) => {
            println!("Starting ntd server...");
            run_server(*port).await;
            return;
        }
        Some(Commands::Todo { action }) => {
            dispatch_subcommand(&cli, cli::Commands::Todo { action: action.clone() }).await;
            return;
        }
        Some(Commands::Loop { action }) => {
            dispatch_subcommand(&cli, cli::Commands::Loop { action: action.clone() }).await;
            return;
        }
        Some(Commands::Tag { action }) => {
            dispatch_subcommand(&cli, cli::Commands::Tag { action: action.clone() }).await;
            return;
        }
        Some(Commands::Stats) => {
            dispatch_subcommand(&cli, cli::Commands::Stats).await;
            return;
        }
        Some(Commands::Daemon { action }) => {
            // daemon::handle_daemon_command 已声明为 async,以便内部 restart
            // 路径可以 await tokio::time::sleep,避免 std::thread::sleep 阻塞
            // tokio worker。
            daemon::handle_daemon_command(action).await;
            return;
        }
        Some(Commands::Skill { action: SkillAction::Install { force, executor } }) => {
            if let Err(e) = handle_skill_install(*force, executor.as_deref()) {
                // Skill install 失败时仍用结构化 JSON 走 stderr（CLI 解析约定），
                // 同时通过 tracing 记录带 target/level 的日志，便于日志聚合。
                let payload = serde_json::json!({"error": true, "message": e.to_string()});
                let payload_str = serde_json::to_string(&payload)
                    .unwrap_or_else(|_| r#"{"error":true,"message":"unknown"}"#.to_string());
                tracing::error!(error_payload = %payload_str, "Skill install failed");
                eprintln!("{}", payload_str);
                std::process::exit(1);
            }
            return;
        }
        None => {
            // No subcommand: start server by default
            println!("Starting ntd server...");
            run_server(None).await;
        }
    }
}

/// 初始化 tracing-subscriber，作为进程日志的统一入口。
///
/// 设计要点：
/// 1. **早期初始化**：在 `main()` 第一行调用，让 `Config::load()`、`Upgrade` 子命令、
///    `Skill install` 等早期路径都能用 `tracing::warn!` / `tracing::error!` 输出结构化日志，
///    不再被 raw `eprintln!` 污染 stderr。
/// 2. **stderr 输出**：tracing 默认写 stderr，与 `println!` 的 stdout 用户面消息解耦，
///    方便 2>&1 重定向 / journald 收集。
/// 3. **`try_init` 而非 `init`**：测试或集成测试可能在更外层已经设置过 subscriber，
///    用 `try_init` 避免 "global default subscriber already set" panic。
/// 4. **EnvFilter 优先级**：RUST_LOG 环境变量优先；缺省走 "info"。
fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(true)
        .with_timer(tracing_subscriber::fmt::time::time())
        .try_init();
}

fn print_structured_error(e: &anyhow::Error) {
    // 保留 stderr 上的 JSON 输出：CLI 工具契约（`ntd ... --output json` 解析此格式），
    // 不能改成 tracing。同时记录一条结构化日志供聚合。
    let err = serde_json::json!({
        "error": true,
        "message": e.to_string(),
    });
    let payload = serde_json::to_string(&err)
        .unwrap_or_else(|_| r#"{"error":true,"message":"unknown"}"#.to_string());
    tracing::error!(error_payload = %payload, "CLI subcommand failed");
    eprintln!("{}", payload);
}

/// Print a structured error and exit the process.
fn fail_on<T>(result: anyhow::Result<T>) -> T {
    match result {
        Ok(v) => v,
        Err(e) => {
            print_structured_error(&e);
            std::process::exit(1);
        }
    }
}

/// Dispatch a CLI subcommand (Todo/Tag/Stats) with unified error handling.
async fn dispatch_subcommand(cli: &Cli, command: cli::Commands) {
    let sub_cli = cli::Cli {
        server: cli.server.clone(),
        output: cli.output,
        fields: cli.fields.clone(),
        command,
    };
    fail_on(cli::run_command(&sub_cli).await);
}

/// Executor type → skill directory mapping (delegated to shared module).
fn executor_skills_dir(et: &str) -> Option<PathBuf> {
    handlers::skills::executor_skills_dir_str(et)
}

const ALL_EXECUTORS: &[&str] = &[
    "claudecode", "hermes", "codex", "codebuddy",
    "opencode", "atomcode", "kimi", "mobilecoder", "codewhale", "pi", "mimo",
    "zhanlu",
];

/// Install embedded ntd-usage skill to executor skill directories.
fn handle_skill_install(force: bool, executor_filter: Option<&str>) -> anyhow::Result<()> {
    let executors: Vec<&str> = if let Some(filter) = executor_filter {
        filter.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect()
    } else {
        ALL_EXECUTORS.to_vec()
    };

    if executors.is_empty() {
        anyhow::bail!("No executors specified");
    }

    // Verify the embedded skill exists
    let skill_dir = "ntd-usage";
    let has_skill = NtdSkills::iter().any(|path| path.starts_with(skill_dir));
    if !has_skill {
        anyhow::bail!("Embedded skill '{}' not found in binary. Rebuild ntd to include skill files.", skill_dir);
    }

    let mut installed = 0;
    let mut skipped = 0;
    let mut unknown: Vec<&str> = Vec::new();

    for et in &executors {
        let base_dir = match executor_skills_dir(et) {
            Some(d) => d,
            None => {
                unknown.push(et);
                continue;
            }
        };

        let target = base_dir.join(skill_dir);

        if target.exists() {
            if !force {
                println!("  ✓ {} already installed (use --force to reinstall)", et);
                skipped += 1;
                continue;
            }
            std::fs::remove_dir_all(&target)?;
        }

        // Create target directory
        std::fs::create_dir_all(&target)?;

        // Extract embedded skill files
        let prefix = format!("{}/", skill_dir);
        let mut extracted = 0;
        for path in NtdSkills::iter() {
            if !path.starts_with(&prefix) {
                continue;
            }
            let relative_path = &path[prefix.len()..];
            if relative_path.is_empty() {
                continue; // skip the directory entry itself
            }

            let file = match NtdSkills::get(&path) {
                Some(f) => f,
                None => continue,
            };

            let file_path = target.join(relative_path);

            // Create parent directories
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            std::fs::write(&file_path, &file.data)?;
            extracted += 1;
        }

        if extracted > 0 {
            println!("  ✓ Installed ntd-usage skill for {} ({} files)", et, extracted);
            installed += 1;
        } else {
            anyhow::bail!("No files extracted for executor '{}'. Embedded skill data may be empty.", et);
        }
    }

    // When --executor is explicitly provided, unknown executors are fatal.
    // Without --executor (installing for all known), only warn and continue.
    if executor_filter.is_some() && !unknown.is_empty() {
        anyhow::bail!(
            "Unknown executor(s): {}. Supported executors: {}",
            unknown.join(", "),
            ALL_EXECUTORS.join(", ")
        );
    }
    for et in &unknown {
        println!("  ✗ Unknown executor '{}', skipping", et);
    }

    if installed == 0 && skipped > 0 {
        println!("All skills already installed. Use `ntd skill install --force` to reinstall.");
    } else {
        println!("Done. Installed for {} executor(s), skipped {} (already present).", installed, skipped);
    }

    Ok(())
}

async fn run_server(cli_port: Option<u16>) {
    // tracing-subscriber has already been initialized in `main()`; no need to init again here.
    let cfg = ntd::config::Config::load();

    // Ensure ~/.ntd/ directory exists on first startup
    if let Some(home) = dirs::home_dir() {
        std::fs::create_dir_all(home.join(".ntd")).ok();
    }

    // Expand tilde in db_path to home directory (normalize_paths is called in Config::load,
    // but may not have expanded if config file didn't exist or was corrupted)
    let db_path = if cfg.db_path.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            let relative = cfg.db_path.trim_start_matches('~').trim_start_matches(std::path::MAIN_SEPARATOR);
            home.join(relative).to_string_lossy().to_string()
        } else {
            cfg.db_path.clone()
        }
    } else {
        cfg.db_path.clone()
    };
    if let Some(parent) = std::path::Path::new(&db_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let db = match db::Database::new(&db_path).await {
        Ok(db) => Arc::new(db),
        Err(e) => {
            // 数据库不可用是「启动级不可恢复错误」：除了 tracing 结构化日志，
            // 额外保留一行 stderr 直写作为「last-resort」兜底，确保即使 subscriber
            // 没有安装成功（如 RUST_LOG 写错导致 EnvFilter panic）也能给运维留痕。
            tracing::error!(db_path = %db_path, error = %e, "Failed to open database");
            eprintln!("Failed to open database at {}: {}", db_path, e);
            std::process::exit(1);
        }
    };

    if let Err(e) = db.cleanup_orphan_execution_records().await {
        tracing::error!("Failed to cleanup orphan execution records: {}", e);
    }

    // Migrate executor paths from config.yaml to database (one-time), then seed defaults if empty
    if let Err(e) = db.migrate_from_config(&cfg.executors).await {
        tracing::warn!("Executor config migration check failed: {}", e);
    }
    if let Err(e) = db.seed_default_executors().await {
        tracing::warn!("Failed to seed default executors: {}", e);
    }
    // Sync any new executors added since last version
    if let Err(e) = db.sync_new_executors().await {
        tracing::warn!("Failed to sync new executors: {}", e);
    }
    if let Err(e) = db.backfill_session_dir().await {
        tracing::warn!("Failed to backfill session_dir: {}", e);
    }

    let executor_registry = Arc::new(adapters::ExecutorRegistry::new());
    let db_executors = db.get_enabled_executors().await.unwrap_or_default();
    for ec in &db_executors {
        if executor_registry.register_by_name(&ec.name, &ec.path).await {
            info!("Registered executor: {} ({})", ec.display_name, ec.name);
        } else {
            tracing::warn!("Unknown executor '{}' in database, skipping", ec.name);
        }
    }

    let executors = executor_registry.list_executors().await;
    info!("Available executors: {:?}", executors);

    // WebSocket 事件 broadcast channel 容量从配置读取，避免硬编码 100 在高频输出场景下
    // 因 ring buffer 覆盖而丢失 Finished 等关键事件。Config::load 已经做过最小值 clamp。
    // Safety guard: values > 100_000_000 would request >100GB for ring buffer;
    // treat as fatal rather than silently OOM.
    if cfg.broadcast_channel_capacity > 100_000_000 {
        panic!(
            "broadcast_channel_capacity {} exceeds hard safety limit (100_000_000); refusing to start",
            cfg.broadcast_channel_capacity
        );
    }
    let (tx, _rx) = broadcast::channel(cfg.broadcast_channel_capacity);
    let task_manager = Arc::new(TaskManager::new());
    let config = Arc::new(std::sync::RwLock::new(cfg.clone()));

    let scheduler = Arc::new({
        // TodoScheduler 现在不再持有 hook_service（todo hook 已整块移除），
        // 构造函数无参；保留 ServiceContext 透传给 load_from_db 走调度加载。
        let sched = TodoScheduler::new().await.unwrap_or_else(|e| {
            tracing::error!("Failed to create scheduler: {}. Exiting.", e);
            std::process::exit(1);
        });
        let ctx = ServiceContext {
            db: db.clone(),
            executor_registry: executor_registry.clone(),
            tx: tx.clone(),
            task_manager: task_manager.clone(),
            config: config.clone(),
        };
        if let Err(e) = sched.load_from_db(&ctx).await {
            tracing::warn!("Failed to load scheduled tasks: {}", e);
        }
        if let Err(e) = sched.start().await {
            tracing::warn!("Failed to start scheduler: {}", e);
        }

        // 注册自动数据库备份定时任务
        if cfg.auto_backup_enabled {
            match handlers::backup::start_auto_backup(&cfg.auto_backup_cron, db.clone(), config.clone()) {
                Ok(()) => info!("Auto database backup enabled, cron: {}", cfg.auto_backup_cron),
                Err(e) => tracing::warn!("Failed to start auto backup: {}", e),
            }
        }

        // 注册 Todo 自动备份定时任务
        if cfg.auto_todo_backup_enabled {
            match handlers::backup::start_todo_auto_backup(db.clone(), config.clone()) {
                Ok(()) => info!("Auto Todo backup enabled, cron: {}", cfg.auto_todo_backup_cron),
                Err(e) => tracing::warn!("Failed to start Todo auto backup: {}", e),
            }
        }

        // 注册 Skill 自动备份定时任务
        if cfg.auto_skill_backup_enabled {
            match handlers::backup::start_skill_auto_backup(config.clone()) {
                Ok(()) => info!("Auto Skill backup enabled, cron: {}", cfg.auto_skill_backup_cron),
                Err(e) => tracing::warn!("Failed to start Skill auto backup: {}", e),
            }
        }

        // 注册 AI 使用统计自动归档定时任务
        if cfg.auto_usage_stats_enabled {
            match handlers::backup::start_usage_stats_archival(db.clone(), config.clone()) {
                Ok(()) => info!("Auto usage stats archival enabled, cron: {}", cfg.auto_usage_stats_cron),
                Err(e) => tracing::warn!("Failed to start usage stats archival: {}", e),
            }
        }

        // 注册自定义模板自动同步定时任务
        if cfg.auto_sync_custom_templates_enabled {
            let db = Arc::clone(&db);
            match handlers::custom_template::start_custom_template_auto_sync(&cfg.auto_sync_custom_templates_cron, db, config.clone()) {
                Ok(()) => info!("Auto custom template sync enabled, cron: {}", cfg.auto_sync_custom_templates_cron),
                Err(e) => tracing::warn!("Failed to start custom template auto sync: {}", e),
            }
        }

        sched
    });

    let app = handlers::create_app(
        ntd::service_context::ServiceContext {
            db: db.clone(),
            executor_registry: executor_registry.clone(),
            tx: tx.clone(),
            task_manager: task_manager.clone(),
            config: config.clone(),
        },
        scheduler,
    )
    .await;

    let port = cli_port.unwrap_or(cfg.port);

    info!("===========================================");
    info!("  Nothing Todo (ntd)");
    info!("  Open http://localhost:{} in your browser", port);
    info!("===========================================");

    let std_listener = match std::net::TcpListener::bind(format!("0.0.0.0:{}", port)) {
        Ok(l) => l,
        Err(e) => {
            // 端口绑定失败（如被占用、权限不足）：典型启动级不可恢复错误。
            // 同上保留 eprintln! 作为 last-resort 兜底。
            tracing::error!(port, error = %e, "Failed to bind to port");
            eprintln!("Failed to bind to port {}: {}", port, e);
            std::process::exit(1);
        }
    };

    #[cfg(unix)]
    {
        // 把 unsafe libc::setsockopt 调用集中到 sys 模块；
        // 此处只关心业务意图（"在已 bind 的 socket 上开 SO_REUSEADDR"），
        // FFI 细节（指针、optlen、错误码）由 sys 内部处理。
        use std::os::fd::AsRawFd;
        if let Err(e) = ntd::sys::set_socket_reuseaddr(std_listener.as_raw_fd()) {
            // setsockopt 失败不会让进程无法启动；只记 warning，保留原行为（启动仍继续）。
            tracing::warn!("Failed to set SO_REUSEADDR: {}", e);
        }
    }

    if let Err(e) = std_listener.set_nonblocking(true) {
        // set_nonblocking 是 fnctl 级别调用，理论上不会失败；失败时仍走 tracing + stderr 兜底
        tracing::error!(error = %e, "Failed to set non-blocking");
        eprintln!("Failed to set non-blocking: {}", e);
        std::process::exit(1);
    }
    let listener = match tokio::net::TcpListener::from_std(std_listener) {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, "Failed to create async listener");
            eprintln!("Failed to create async listener: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("Server error: {}", e);
    }
}
