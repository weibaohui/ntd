use std::sync::Arc;
use clap::{Parser, Subcommand};
use tokio::sync::broadcast;
use tracing::info;

use std::path::PathBuf;
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
            println!("Upgrading ntd...");
            let status = std::process::Command::new("npm")
                .args(["install", "-g", "@weibaohui/nothing-todo@latest"])
                .status()
                .expect("Failed to run npm. Is npm installed?");
            if status.success() {
                println!("Upgrade completed successfully!");
            } else {
                eprintln!("Upgrade failed.");
                std::process::exit(1);
            }
            return;
        }
        Some(Commands::Server { action: ServerAction::Start { port } }) => {
            println!("Starting ntd server...");
            run_server(*port).await;
            return;
        }
        Some(Commands::Todo { action }) => {
            let cli = cli::Cli {
                server: cli.server.clone(),
                output: cli.output,
                fields: cli.fields.clone(),
                command: cli::Commands::Todo { action: action.clone() },
            };
            if let Err(e) = cli::run_command(&cli).await {
                print_structured_error(&e);
                std::process::exit(1);
            }
            return;
        }
        Some(Commands::Tag { action }) => {
            let cli = cli::Cli {
                server: cli.server.clone(),
                output: cli.output,
                fields: cli.fields.clone(),
                command: cli::Commands::Tag { action: action.clone() },
            };
            if let Err(e) = cli::run_command(&cli).await {
                print_structured_error(&e);
                std::process::exit(1);
            }
            return;
        }
        Some(Commands::Stats) => {
            let cli = cli::Cli {
                server: cli.server.clone(),
                output: cli.output,
                fields: cli.fields.clone(),
                command: cli::Commands::Stats,
            };
            if let Err(e) = cli::run_command(&cli).await {
                print_structured_error(&e);
                std::process::exit(1);
            }
            return;
        }
        Some(Commands::Daemon { action }) => {
            daemon::handle_daemon_command(action);
            return;
        }
        Some(Commands::Skill { action: SkillAction::Install { force, executor } }) => {
            if let Err(e) = handle_skill_install(*force, executor.as_deref()) {
                eprintln!("{}", serde_json::json!({"error": true, "message": e.to_string()}));
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

fn print_structured_error(e: &anyhow::Error) {
    let err = serde_json::json!({
        "error": true,
        "message": e.to_string(),
    });
    eprintln!("{}", serde_json::to_string(&err).unwrap_or_else(|_| r#"{"error":true,"message":"unknown"}"#.to_string()));
}

/// Executor type → skill directory mapping (delegated to shared module).
fn executor_skills_dir(et: &str) -> Option<PathBuf> {
    handlers::skills::executor_skills_dir_str(et)
}

const ALL_EXECUTORS: &[&str] = &[
    "claudecode", "hermes", "codex", "codebuddy",
    "opencode", "atomcode", "kimi", "joinai",
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
    let cfg = ntd::config::Config::load();

    let level = cfg.log_level
        .parse::<tracing::Level>()
        .unwrap_or(tracing::Level::INFO);

    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(true)
        .with_timer(tracing_subscriber::fmt::time::time())
        .init();

    let db_path = &cfg.db_path;
    if let Some(parent) = std::path::Path::new(db_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let db = match db::Database::new(db_path).await {
        Ok(db) => Arc::new(db),
        Err(e) => {
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

    let (tx, _rx) = broadcast::channel(100);
    let task_manager = Arc::new(TaskManager::new());
    let config = Arc::new(tokio::sync::RwLock::new(cfg.clone()));

    let scheduler = Arc::new({
        let sched = TodoScheduler::new().await.unwrap_or_else(|e| {
            tracing::error!("Failed to create scheduler: {}. Exiting.", e);
            std::process::exit(1);
        });
        if let Err(e) = sched.load_from_db(db.clone(), executor_registry.clone(), tx.clone(), task_manager.clone(), config.clone()).await {
            tracing::warn!("Failed to load scheduled tasks: {}", e);
        }
        if let Err(e) = sched.start().await {
            tracing::warn!("Failed to start scheduler: {}", e);
        }

        // 注册自动数据库备份定时任务
        if cfg.auto_backup_enabled {
            match handlers::backup::start_auto_backup(&cfg.auto_backup_cron, config.clone()) {
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

    let app = handlers::create_app(db, executor_registry, tx, scheduler, task_manager, config.clone());

    let port = cli_port.unwrap_or(cfg.port);

    info!("===========================================");
    info!("  Nothing Todo (ntd)");
    info!("  Open http://localhost:{} in your browser", port);
    info!("===========================================");

    let std_listener = match std::net::TcpListener::bind(format!("0.0.0.0:{}", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to bind to port {}: {}", port, e);
            std::process::exit(1);
        }
    };

    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let optval: libc::c_int = 1;
        unsafe {
            libc::setsockopt(
                std_listener.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_REUSEADDR,
                &optval as *const libc::c_int as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }
    }

    if let Err(e) = std_listener.set_nonblocking(true) {
        eprintln!("Failed to set non-blocking: {}", e);
        std::process::exit(1);
    }
    let listener = match tokio::net::TcpListener::from_std(std_listener) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to create async listener: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("Server error: {}", e);
    }
}
