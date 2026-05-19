use axum::{
    extract::{State, Query},
    body::Bytes,
    response::IntoResponse,
    http::header,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;
use zip::write::FileOptions;
use zip::ZipWriter;
use std::io::Write;

use crate::handlers::{AppError, AppState};
use crate::models::{ApiResponse, BackupData, TagBackup, TodoBackup, utc_timestamp};

/// 导出备份（返回 YAML 格式字符串）
pub async fn export_backup(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let tags = state.db.get_tag_backups().await?;
    let todos = state.db.get_todo_backups().await?;
    let data = BackupData {
        version: "1.0".to_string(),
        created_at: utc_timestamp(),
        tags,
        todos,
    };
    let yaml = serde_yaml::to_string(&data).map_err(|e| AppError::Internal(e.to_string()))?;
    Ok((
        [(header::CONTENT_TYPE, "application/x-yaml; charset=utf-8")],
        yaml,
    ))
}

/// 选择性导出（按 todo ID 列表导出）
#[derive(Deserialize)]
pub struct ExportSelectedRequest {
    pub todo_ids: Vec<i64>,
}

pub async fn export_selected(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<ExportSelectedRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.todo_ids.is_empty() {
        return Err(AppError::BadRequest("No todo IDs provided".to_string()));
    }
    let todos = state.db.get_todo_backups_by_ids(&req.todo_ids).await?;
    if todos.is_empty() {
        return Err(AppError::BadRequest("No todos found for given IDs".to_string()));
    }
    // Collect unique tag names from valid todos and query tags by name
    let tag_names: std::collections::HashSet<&str> = todos.iter()
        .flat_map(|t| t.tag_names.iter().map(|s| s.as_str()))
        .collect();
    let tags = state.db.get_tag_backups_by_names(&tag_names.into_iter().collect::<Vec<_>>()).await?;
    let data = BackupData {
        version: "1.0".to_string(),
        created_at: utc_timestamp(),
        tags,
        todos,
    };
    let yaml = serde_yaml::to_string(&data).map_err(|e| AppError::Internal(e.to_string()))?;
    Ok((
        [(header::CONTENT_TYPE, "application/x-yaml; charset=utf-8")],
        yaml,
    ))
}

/// 导入备份（接收 YAML 格式字符串，清空现有数据后导入）
pub async fn import_backup(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<ApiResponse<String>, AppError> {
    let yaml_str = String::from_utf8(body.to_vec())
        .map_err(|_| AppError::BadRequest("Invalid UTF-8 in request body".to_string()))?;
    let data: BackupData = serde_yaml::from_str(&yaml_str)
        .map_err(|e| AppError::BadRequest(format!("Invalid YAML: {}", e)))?;

    if data.todos.is_empty() {
        return Err(AppError::BadRequest("Backup contains no todos".to_string()));
    }

    state.db.import_backup(&data.tags, &data.todos).await
        .map_err(|e| AppError::Internal(format!("Import failed, data unchanged: {}", e)))?;

    Ok(ApiResponse::ok(format!("Imported {} todos and {} tags", data.todos.len(), data.tags.len())))
}

#[derive(Deserialize)]
pub struct MergeRequest {
    pub tags: Vec<TagBackup>,
    pub todos: Vec<TodoBackup>,
}

/// 智能合并导入（不清空现有数据，按 title+prompt 匹配覆盖或新建）
pub async fn merge_backup(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<MergeRequest>,
) -> Result<ApiResponse<String>, AppError> {
    if req.todos.is_empty() {
        return Err(AppError::BadRequest("No todos to merge".to_string()));
    }

    let (created, updated) = state.db.merge_backup(&req.tags, &req.todos).await
        .map_err(|e| AppError::Internal(format!("Merge failed: {}", e)))?;

    Ok(ApiResponse::ok(format!("新建 {} 项，覆盖 {} 项", created, updated)))
}

// ============ 数据库备份 ============

fn backup_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".ntd").join("backups")
}

/// 获取数据库备份目录（按数据库文件名分目录）
fn db_backup_dir(db_filename: &str) -> PathBuf {
    backup_dir().join("db").join(db_filename)
}

/// 获取Todo备份目录
fn todo_backup_dir() -> PathBuf {
    backup_dir().join("todo")
}

/// 手动下载数据库文件
pub async fn download_database(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = state.config.read().await;
    let db_path = PathBuf::from(&cfg.db_path);

    // 路径穿越防护：验证数据库路径位于安全目录 ~/.ntd/ 内
    let canonicalized = std::fs::canonicalize(&db_path)
        .map_err(|_| AppError::BadRequest("Invalid database path".to_string()))?;
    let safe_dir = dirs::home_dir()
        .ok_or_else(|| AppError::Internal("Cannot determine home directory".to_string()))?
        .join(".ntd");
    let safe_dir_canonical = std::fs::canonicalize(&safe_dir)
        .unwrap_or(safe_dir);
    if !canonicalized.starts_with(&safe_dir_canonical) {
        return Err(AppError::BadRequest("Database path outside safe directory".to_string()));
    }

    if !db_path.exists() {
        return Err(AppError::Internal("Database file not found".to_string()));
    }

    let path = db_path.clone();
    let bytes = tokio::task::spawn_blocking(move || std::fs::read(&path))
        .await
        .map_err(|e| AppError::Internal(format!("Task join error: {}", e)))?
        .map_err(|e| AppError::Internal(format!("Failed to read database: {}", e)))?;

    let filename = format!("ntd-database-{}.db",
        chrono::Utc::now().format("%Y%m%d-%H%M%S"));

    let disposition = format!("attachment; filename=\"{}\"", filename);
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        bytes,
    ))
}

/// 将数据库复制到备份目录
pub async fn trigger_local_backup(
    State(state): State<AppState>,
) -> Result<ApiResponse<String>, AppError> {
    let cfg = state.config.read().await;
    let db_path = PathBuf::from(&cfg.db_path);
    let max_files = cfg.auto_backup_max_files;
    drop(cfg);

    if !db_path.exists() {
        return Err(AppError::Internal("Database file not found".to_string()));
    }

    // 获取原始数据库文件名
    let db_filename = db_path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let dir = db_backup_dir(&db_filename);
    let db_path_clone = db_path.clone();
    let dir_clone = dir.clone();
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    // 备份文件名包含原始数据库文件名
    let backup_filename = format!("{}-{}.db", db_filename, timestamp);
    let backup_path = dir.join(&backup_filename);

    let backup_path_clone = backup_path.clone();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&dir_clone)?;
        std::fs::copy(&db_path_clone, &backup_path_clone)
    })
    .await
    .map_err(|e| AppError::Internal(format!("Task join error: {}", e)))?
    .map_err(|e| AppError::Internal(format!("Failed to copy database: {}", e)))?;

    // 清理旧备份（按数据库分目录清理）
    cleanup_old_db_backups(&dir, max_files);

    Ok(ApiResponse::ok(format!("备份成功: {}", backup_path.display())))
}

/// 执行数据库压缩优化
pub async fn database_optimize(
    State(state): State<AppState>,
) -> Result<ApiResponse<String>, AppError> {
    let cfg = state.config.read().await;
    let db_path = PathBuf::from(&cfg.db_path);
    drop(cfg);

    if !db_path.exists() {
        return Err(AppError::Internal("Database file not found".to_string()));
    }

    // 执行 PRAGMA optimize，这是 SQLite 的轻量级优化命令
    // 它会更新数据库的统计信息，帮助查询优化器生成更好的执行计划
    // 注意：PRAGMA optimize 返回结果集，需要使用 query_exec 而非 exec
    state.db.query_exec("PRAGMA optimize").await
        .map_err(|e| AppError::Internal(format!("Database optimize failed: {}", e)))?;

    tracing::info!("Database optimization completed for: {}", db_path.display());
    Ok(ApiResponse::ok("数据库优化完成".to_string()))
}

#[derive(Serialize)]
pub struct BackupFile {
    pub name: String,
    pub size: u64,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct BackupStatus {
    pub auto_backup_enabled: bool,
    pub auto_backup_cron: String,
    pub auto_backup_max_files: usize,
    pub last_backup: Option<String>,
    pub files: Vec<BackupFile>,
}

/// 获取数据库备份状态
pub async fn get_database_backup_status(
    State(state): State<AppState>,
) -> Result<ApiResponse<BackupStatus>, AppError> {
    let cfg = state.config.read().await;
    let db_path = PathBuf::from(&cfg.db_path);
    let auto_backup_enabled = cfg.auto_backup_enabled;
    let auto_backup_cron = cfg.auto_backup_cron.clone();
    let auto_backup_max_files = cfg.auto_backup_max_files;
    drop(cfg);

    // 获取原始数据库文件名
    let db_filename = db_path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let dir = db_backup_dir(&db_filename);

    let files = tokio::task::spawn_blocking(move || {
        let mut files = Vec::new();
        if dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|ext| ext == "db") {
                        let meta = entry.metadata().ok();
                        let created = meta.as_ref()
                            .and_then(|m| m.created().ok())
                            .map(|t| {
                                let dt: chrono::DateTime<chrono::Local> = t.into();
                                dt.format("%Y-%m-%d %H:%M:%S").to_string()
                            })
                            .unwrap_or_default();
                        files.push(BackupFile {
                            name: path.file_name().unwrap_or_default().to_string_lossy().to_string(),
                            size: meta.map(|m| m.len()).unwrap_or(0),
                            created_at: created,
                        });
                    }
                }
            }
        }
        files.sort_by(|a, b| b.name.cmp(&a.name));
        files
    })
    .await
    .map_err(|e| AppError::Internal(format!("Task join error: {}", e)))?;

    let last_backup = files.first().map(|f| f.created_at.clone());

    Ok(ApiResponse::ok(BackupStatus {
        auto_backup_enabled,
        auto_backup_cron,
        auto_backup_max_files,
        last_backup,
        files,
    }))
}

/// 更新自动备份配置
#[derive(Deserialize)]
pub struct UpdateAutoBackupRequest {
    pub enabled: bool,
    pub cron: String,
    pub max_files: Option<usize>,
}

pub async fn update_auto_backup(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<UpdateAutoBackupRequest>,
) -> Result<ApiResponse<String>, AppError> {
    // 验证 cron 表达式
    if req.enabled {
        let schedule = cron::Schedule::from_str(&req.cron)
            .map_err(|e| AppError::BadRequest(format!("Invalid cron expression: {}", e)))?;
        // 验证能产生下一次执行时间
        schedule.upcoming(chrono::Utc).next()
            .ok_or_else(|| AppError::BadRequest("Cron expression has no future executions".to_string()))?;
    }

    let mut cfg = state.config.write().await;
    cfg.auto_backup_enabled = req.enabled;
    cfg.auto_backup_cron = req.cron;
    if let Some(max_files) = req.max_files {
        if max_files == 0 {
            return Err(AppError::BadRequest("保留数量不能为 0".to_string()));
        }
        cfg.auto_backup_max_files = max_files;
    }
    cfg.normalize_paths();

    let cfg_clone = cfg.clone();
    tokio::task::spawn_blocking(move || cfg_clone.save())
        .await
        .map_err(|e| AppError::Internal(format!("Join error: {}", e)))?
        .map_err(|e| AppError::Internal(format!("Failed to save config: {}", e)))?;

    Ok(ApiResponse::ok("自动备份配置已更新".to_string()))
}

/// 删除数据库备份文件
#[derive(Deserialize)]
pub struct DeleteBackupRequest {
    pub filename: String,
}

pub async fn delete_backup_file(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<DeleteBackupRequest>,
) -> Result<ApiResponse<String>, AppError> {
    // 安全检查：文件名不能包含路径分隔符
    if req.filename.contains('/') || req.filename.contains('\\') || req.filename.contains("..") {
        return Err(AppError::BadRequest("Invalid filename".to_string()));
    }

    let cfg = state.config.read().await;
    let db_path = PathBuf::from(&cfg.db_path);
    let db_filename = db_path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let path = db_backup_dir(&db_filename).join(&req.filename);

    if !path.exists() {
        return Err(AppError::NotFound);
    }
    tokio::task::spawn_blocking(move || std::fs::remove_file(&path))
        .await
        .map_err(|e| AppError::Internal(format!("Task join error: {}", e)))?
        .map_err(|e| AppError::Internal(format!("Failed to delete: {}", e)))?;
    Ok(ApiResponse::ok("已删除".to_string()))
}

/// 下载指定数据库备份文件
#[derive(Deserialize)]
pub struct DownloadBackupQuery {
    pub filename: String,
}

pub async fn download_backup_file(
    State(state): State<AppState>,
    Query(query): Query<DownloadBackupQuery>,
) -> Result<impl IntoResponse, AppError> {
    if query.filename.contains('/') || query.filename.contains('\\') || query.filename.contains("..") {
        return Err(AppError::BadRequest("Invalid filename".to_string()));
    }

    let cfg = state.config.read().await;
    let db_path = PathBuf::from(&cfg.db_path);
    let db_filename = db_path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let path = db_backup_dir(&db_filename).join(&query.filename);

    if !path.exists() {
        return Err(AppError::NotFound);
    }

    let filename = query.filename.clone();
    let bytes = tokio::task::spawn_blocking(move || std::fs::read(&path))
        .await
        .map_err(|e| AppError::Internal(format!("Task join error: {}", e)))?
        .map_err(|e| AppError::Internal(format!("Failed to read backup file: {}", e)))?;

    let disposition = format!("attachment; filename=\"{}\"", filename);
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        bytes,
    ))
}

// ============ Todo 备份文件操作（独立端点） ============

/// 删除 Todo 备份文件
#[derive(Deserialize)]
pub struct DeleteTodoBackupRequest {
    pub filename: String,
}

pub async fn delete_todo_backup_file(
    State(_state): State<AppState>,
    axum::Json(req): axum::Json<DeleteTodoBackupRequest>,
) -> Result<ApiResponse<String>, AppError> {
    // 安全检查：文件名不能包含路径分隔符
    if req.filename.contains('/') || req.filename.contains('\\') || req.filename.contains("..") {
        return Err(AppError::BadRequest("Invalid filename".to_string()));
    }
    let path = todo_backup_dir().join(&req.filename);
    if !path.exists() {
        return Err(AppError::NotFound);
    }
    tokio::task::spawn_blocking(move || std::fs::remove_file(&path))
        .await
        .map_err(|e| AppError::Internal(format!("Task join error: {}", e)))?
        .map_err(|e| AppError::Internal(format!("Failed to delete: {}", e)))?;
    Ok(ApiResponse::ok("已删除".to_string()))
}

/// 下载 Todo 备份文件
#[derive(Deserialize)]
pub struct DownloadTodoBackupQuery {
    pub filename: String,
}

pub async fn download_todo_backup_file(
    Query(query): Query<DownloadTodoBackupQuery>,
) -> Result<impl IntoResponse, AppError> {
    if query.filename.contains('/') || query.filename.contains('\\') || query.filename.contains("..") {
        return Err(AppError::BadRequest("Invalid filename".to_string()));
    }
    let path = todo_backup_dir().join(&query.filename);
    if !path.exists() {
        return Err(AppError::NotFound);
    }

    let filename = query.filename.clone();
    let bytes = tokio::task::spawn_blocking(move || std::fs::read(&path))
        .await
        .map_err(|e| AppError::Internal(format!("Task join error: {}", e)))?
        .map_err(|e| AppError::Internal(format!("Failed to read backup file: {}", e)))?;

    let disposition = format!("attachment; filename=\"{}\"", filename);
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        bytes,
    ))
}

/// 执行数据库文件备份（供定时任务调用）
pub fn perform_database_backup(db_path: &str, max_files: usize) -> Result<String, String> {
    let db_path = PathBuf::from(db_path);

    if !db_path.exists() {
        return Err("Database file not found".to_string());
    }

    // 获取原始数据库文件名
    let db_filename = db_path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let dir = db_backup_dir(&db_filename);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create backup dir: {}", e))?;

    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let backup_filename = format!("{}-{}.db", db_filename, timestamp);
    let backup_path = dir.join(&backup_filename);

    std::fs::copy(&db_path, &backup_path)
        .map_err(|e| format!("Failed to copy database: {}", e))?;

    cleanup_old_db_backups(&dir, max_files);

    Ok(format!("Auto backup: {}", backup_path.display()))
}

fn cleanup_old_db_backups(dir: &PathBuf, keep: usize) {
    if !dir.exists() {
        return;
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == "db"))
                .collect()
        })
        .unwrap_or_default();

    if files.len() <= keep {
        return;
    }

    files.sort_by(|a, b| {
        let a_time = std::fs::metadata(a).and_then(|m| m.created()).ok();
        let b_time = std::fs::metadata(b).and_then(|m| m.created()).ok();
        b_time.cmp(&a_time)
    });

    for old_file in files.iter().skip(keep) {
        std::fs::remove_file(old_file).ok();
    }
}

/// 启动自动备份定时任务
pub fn start_auto_backup(
    cron_expr: &str,
    config: std::sync::Arc<tokio::sync::RwLock<crate::config::Config>>,
) -> Result<(), String> {
    let schedule = cron::Schedule::from_str(cron_expr)
        .map_err(|e| format!("Invalid cron: {}", e))?;

    tokio::spawn(async move {
        loop {
            let next = schedule.upcoming(chrono::Utc).next();
            let delay = match next {
                Some(dt) => {
                    let now = chrono::Utc::now();
                    (dt - now).to_std().unwrap_or(std::time::Duration::from_secs(60))
                }
                None => std::time::Duration::from_secs(3600),
            };
            tokio::time::sleep(delay).await;

            // Read current config from in-memory state
            let (db_path, max_files) = {
                let cfg = config.read().await;
                (cfg.db_path.clone(), cfg.auto_backup_max_files)
            };

            match tokio::task::spawn_blocking(move || perform_database_backup(&db_path, max_files)).await {
                Ok(Ok(msg)) => tracing::info!("{}", msg),
                Ok(Err(e)) => tracing::error!("Auto backup failed: {}", e),
                Err(e) => tracing::error!("Auto backup task panicked: {}", e),
            }
        }
    });

    Ok(())
}

// ============ Todo 备份 ============

#[derive(Serialize)]
pub struct TodoBackupStatus {
    pub auto_backup_enabled: bool,
    pub auto_backup_cron: String,
    pub auto_backup_max_files: usize,
    pub last_backup: Option<String>,
    pub files: Vec<BackupFile>,
}

/// 获取 Todo 备份状态
pub async fn get_todo_backup_status(
    State(state): State<AppState>,
) -> Result<ApiResponse<TodoBackupStatus>, AppError> {
    let cfg = state.config.read().await;
    let auto_backup_enabled = cfg.auto_todo_backup_enabled;
    let auto_backup_cron = cfg.auto_todo_backup_cron.clone();
    let auto_backup_max_files = cfg.auto_todo_backup_max_files;
    drop(cfg);

    let dir = todo_backup_dir();

    let files = tokio::task::spawn_blocking(move || {
        let mut files = Vec::new();
        if dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|ext| ext == "zip") {
                        let meta = entry.metadata().ok();
                        let created = meta.as_ref()
                            .and_then(|m| m.created().ok())
                            .map(|t| {
                                let dt: chrono::DateTime<chrono::Local> = t.into();
                                dt.format("%Y-%m-%d %H:%M:%S").to_string()
                            })
                            .unwrap_or_default();
                        files.push(BackupFile {
                            name: path.file_name().unwrap_or_default().to_string_lossy().to_string(),
                            size: meta.map(|m| m.len()).unwrap_or(0),
                            created_at: created,
                        });
                    }
                }
            }
        }
        files.sort_by(|a, b| b.name.cmp(&a.name));
        files
    })
    .await
    .map_err(|e| AppError::Internal(format!("Task join error: {}", e)))?;

    let last_backup = files.first().map(|f| f.created_at.clone());

    Ok(ApiResponse::ok(TodoBackupStatus {
        auto_backup_enabled,
        auto_backup_cron,
        auto_backup_max_files,
        last_backup,
        files,
    }))
}

/// 手动触发 Todo 备份（打包为 zip 文件）
pub async fn trigger_todo_backup(
    State(state): State<AppState>,
) -> Result<ApiResponse<String>, AppError> {
    let cfg = state.config.read().await;
    let max_files = cfg.auto_todo_backup_max_files;
    drop(cfg);

    let dir = todo_backup_dir();
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let backup_filename = format!("todo-backup-{}.zip", timestamp);
    let backup_path = dir.join(&backup_filename);
    let backup_path_display = backup_path.display().to_string();

    // 获取 Todo 数据
    let tags = state.db.get_tag_backups().await?;
    let todos = state.db.get_todo_backups().await?;
    let data = BackupData {
        version: "1.0".to_string(),
        created_at: utc_timestamp(),
        tags,
        todos,
    };
    let yaml = serde_yaml::to_string(&data).map_err(|e| AppError::Internal(e.to_string()))?;

    // 创建 zip 文件
    let yaml_clone = yaml.clone();
    let dir_clone = dir.clone();
    let backup_path_clone = backup_path.clone();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&dir_clone)?;

        let file = std::fs::File::create(&backup_path_clone)?;
        let mut zip = ZipWriter::new(file);
        let options = FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);

        zip.start_file("backup.yaml", options)?;
        zip.write_all(yaml_clone.as_bytes())?;
        zip.finish()?;

        Ok::<(), std::io::Error>(())
    })
    .await
    .map_err(|e| AppError::Internal(format!("Task join error: {}", e)))?
    .map_err(|e| AppError::Internal(format!("Failed to create zip: {}", e)))?;

    // 清理旧备份
    cleanup_old_todo_backups(&dir, max_files);

    Ok(ApiResponse::ok(format!("备份成功: {}", backup_path_display)))
}

/// 更新 Todo 自动备份配置
#[derive(Deserialize)]
pub struct UpdateTodoAutoBackupRequest {
    pub enabled: bool,
    pub cron: String,
    pub max_files: Option<usize>,
}

pub async fn update_todo_auto_backup(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<UpdateTodoAutoBackupRequest>,
) -> Result<ApiResponse<String>, AppError> {
    // 验证 cron 表达式
    if req.enabled {
        let schedule = cron::Schedule::from_str(&req.cron)
            .map_err(|e| AppError::BadRequest(format!("Invalid cron expression: {}", e)))?;
        // 验证能产生下一次执行时间
        schedule.upcoming(chrono::Utc).next()
            .ok_or_else(|| AppError::BadRequest("Cron expression has no future executions".to_string()))?;
    }

    let mut cfg = state.config.write().await;
    cfg.auto_todo_backup_enabled = req.enabled;
    cfg.auto_todo_backup_cron = req.cron;
    if let Some(max_files) = req.max_files {
        if max_files == 0 {
            return Err(AppError::BadRequest("保留数量不能为 0".to_string()));
        }
        cfg.auto_todo_backup_max_files = max_files;
    }
    cfg.normalize_paths();

    let cfg_clone = cfg.clone();
    tokio::task::spawn_blocking(move || cfg_clone.save())
        .await
        .map_err(|e| AppError::Internal(format!("Join error: {}", e)))?
        .map_err(|e| AppError::Internal(format!("Failed to save config: {}", e)))?;

    Ok(ApiResponse::ok("Todo 自动备份配置已更新".to_string()))
}

/// Start Todo auto backup scheduler
pub fn start_todo_auto_backup(
    db: std::sync::Arc<crate::db::Database>,
    config: std::sync::Arc<tokio::sync::RwLock<crate::config::Config>>,
) -> Result<(), String> {

    let db_clone = db.clone();
    tokio::spawn(async move {
        loop {
            // Read current config from in-memory state
            let (enabled, next_delay) = {
                let cfg = config.read().await;
                if !cfg.auto_todo_backup_enabled {
                    // Auto backup disabled, wait and check again
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    continue;
                }
                let schedule = cron::Schedule::from_str(&cfg.auto_todo_backup_cron)
                    .unwrap_or_else(|_| cron::Schedule::from_str("0 0 4 * * *").unwrap());
                let next = schedule.upcoming(chrono::Utc).next();
                let delay = match next {
                    Some(dt) => {
                        let now = chrono::Utc::now();
                        (dt - now).to_std().unwrap_or(std::time::Duration::from_secs(60))
                    }
                    None => std::time::Duration::from_secs(3600),
                };
                (cfg.auto_todo_backup_enabled, delay)
            };

            tokio::time::sleep(next_delay).await;

            // Skip backup if disabled while sleeping
            if !enabled {
                continue;
            }

            let db = db_clone.clone();
            let max_files = {
                let cfg = config.read().await;
                cfg.auto_todo_backup_max_files
            };

            match perform_todo_backup_async(&db, max_files).await {
                Ok(msg) => tracing::info!("{}", msg),
                Err(e) => tracing::error!("Auto Todo backup failed: {}", e),
            }
        }
    });

    Ok(())
}

/// Perform Todo backup asynchronously
async fn perform_todo_backup_async(db: &std::sync::Arc<crate::db::Database>, max_files: usize) -> Result<String, String> {
    let dir = todo_backup_dir();
    let dir_clone = dir.clone();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&dir_clone)
            .map_err(|e| format!("Failed to create backup dir: {}", e))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))??;

    // Get Todo data
    let tags = db.get_tag_backups().await
        .map_err(|e| format!("Failed to get tag backups: {}", e))?;
    let todos = db.get_todo_backups().await
        .map_err(|e| format!("Failed to get todo backups: {}", e))?;

    let data = BackupData {
        version: "1.0".to_string(),
        created_at: utc_timestamp(),
        tags,
        todos,
    };
    let yaml = serde_yaml::to_string(&data)
        .map_err(|e| format!("Failed to serialize backup: {}", e))?;

    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let backup_filename = format!("todo-backup-{}.zip", timestamp);
    let backup_path = dir.join(&backup_filename);
    let backup_path_for_display = backup_path.display().to_string();

    // Create zip file in blocking task
    let yaml_clone = yaml.clone();
    let backup_path_clone = backup_path.clone();
    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::create(&backup_path_clone)?;
        let mut zip = ZipWriter::new(file);
        let options = FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);

        zip.start_file("backup.yaml", options)?;
        zip.write_all(yaml_clone.as_bytes())?;
        zip.finish()?;

        Ok::<(), std::io::Error>(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
    .map_err(|e| format!("Failed to create zip: {}", e))?;

    // Cleanup old backups
    let dir_for_cleanup = dir.clone();
    tokio::task::spawn_blocking(move || {
        cleanup_old_todo_backups(&dir_for_cleanup, max_files);
    }).await
    .map_err(|e| format!("Task join error: {}", e))?;

    Ok(format!("Auto Todo backup: {}", backup_path_for_display))
}

fn cleanup_old_todo_backups(dir: &PathBuf, keep: usize) {
    if !dir.exists() {
        return;
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == "zip"))
                .collect()
        })
        .unwrap_or_default();

    if files.len() <= keep {
        return;
    }

    files.sort_by(|a, b| {
        let a_time = std::fs::metadata(a).and_then(|m| m.created()).ok();
        let b_time = std::fs::metadata(b).and_then(|m| m.created()).ok();
        b_time.cmp(&a_time)
    });

    for old_file in files.iter().skip(keep) {
        std::fs::remove_file(old_file).ok();
    }
}
