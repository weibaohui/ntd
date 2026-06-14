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
use sea_orm::{ConnectionTrait, DbBackend, Statement};

use crate::handlers::{AppError, AppState};
use crate::models::{ApiResponse, BackupData, TagBackup, TodoBackup, utc_timestamp};
use crate::db::Database;
use crate::services::usage_stats::UsageStatsService;

/// 数据库备份压缩级别 (0-9, 9 为最强压缩)
const BACKUP_COMPRESSION_LEVEL: Option<i64> = Some(9);

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

/// 手动下载数据库文件（zip 压缩格式）
pub async fn download_database(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    // 把 db_path 拷出 owned 值,释放读锁,避免后续的 spawn_blocking().await
    // 持 std::sync 读锁卫跨 .await(否则 future 变 !Send)。
    let db_path = {
        let cfg = state.config.read().unwrap();
        PathBuf::from(&cfg.db_path)
    };

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

    // 使用规范化后的路径读取并压缩数据库
    let canonical_path = canonicalized;
    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, std::io::Error> {
        let db_data = std::fs::read(&canonical_path)?;

        // 创建 zip 文件，使用最强压缩级别
        let file = std::io::Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(file);
        let options = FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(BACKUP_COMPRESSION_LEVEL)
            .unix_permissions(0o644);

        zip.start_file("database.db", options)?;
        zip.write_all(&db_data)?;
        Ok(zip.finish()?.into_inner())
    })
    .await
    .map_err(|e| AppError::Internal(format!("Task join error: {}", e)))?
    .map_err(|e| AppError::Internal(format!("Failed to create zip: {}", e)))?;

    let filename = format!("ntd-database-{}.zip",
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

/// 将数据库压缩备份到目录
pub async fn trigger_local_backup(
    State(state): State<AppState>,
) -> Result<ApiResponse<String>, AppError> {
    // 块作用域内拷出 owned 值,锁卫立即 drop,避免后续的 await 持 std 读锁卫。
    let (db_path, max_files) = {
        let cfg = state.config.read().unwrap();
        (PathBuf::from(&cfg.db_path), cfg.auto_backup_max_files)
    };

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
    let db_filename_clone = db_filename.clone();
    let dir_clone = dir.clone();
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    // 备份文件名包含原始数据库文件名，使用 zip 格式
    let backup_filename = format!("{}-{}.zip", db_filename_clone, timestamp);
    let backup_path = dir.join(&backup_filename);

    let backup_path_clone = backup_path.clone();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&dir_clone)?;

        // 读取数据库文件
        let db_data = std::fs::read(&db_path_clone)?;

        // 创建 zip 文件，使用最强压缩级别 (level 9)
        let file = std::fs::File::create(&backup_path_clone)?;
        let mut zip = ZipWriter::new(file);
        let options = FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(BACKUP_COMPRESSION_LEVEL)
            .unix_permissions(0o644);

        zip.start_file("database.db", options)?;
        zip.write_all(&db_data)?;
        zip.finish()?;

        Ok::<(), std::io::Error>(())
    })
    .await
    .map_err(|e| AppError::Internal(format!("Task join error: {}", e)))?
    .map_err(|e| AppError::Internal(format!("Failed to create zip backup: {}", e)))?;

    // 备份成功后清理旧的 zip 保留文件，避免积压堆积；
    // 仅在写入成功后删除旧文件，防止备份失败时误删历史数据。
    // 放入 spawn_blocking 执行文件 I/O，统一与 perform_*_async 的行为对齐，
    // 避免同步 I/O 阻塞 tokio 运行时线程池。
    let dir_for_cleanup = dir.clone();
    tokio::task::spawn_blocking(move || {
        cleanup_old_zip_backups(&dir_for_cleanup, max_files);
    })
    .await
    .map_err(|e| AppError::Internal(format!("Cleanup task join error: {}", e)))?;

    Ok(ApiResponse::ok(format!("备份成功: {}", backup_path.display())))
}

/// 执行数据库压缩优化
pub async fn database_optimize(
    State(state): State<AppState>,
) -> Result<ApiResponse<String>, AppError> {
    // 块作用域拷出 owned 值,锁卫立即 drop,避免后续的 await 持 std 读锁卫。
    let db_path = {
        let cfg = state.config.read().unwrap();
        PathBuf::from(&cfg.db_path)
    };

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
    // 块作用域内取读锁、拷出 owned 值、立即释放锁卫,避免后续 spawn_blocking().await
    // 持 std::sync::RwLockReadGuard 跨 .await(否则 future 变 !Send,handler trait
    // 不再 implement)。NLL 对显式 drop() 的处理在 async fn state machine 中
    // 过于保守,直接走块作用域把锁卫边界收紧。
    let (db_path, auto_backup_enabled, auto_backup_cron, auto_backup_max_files) = {
        let cfg = state.config.read().unwrap();
        (
            PathBuf::from(&cfg.db_path),
            cfg.auto_backup_enabled,
            cfg.auto_backup_cron.clone(),
            cfg.auto_backup_max_files,
        )
    };

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

    // 关键：先把改动落到 owned clone 上，再立刻释放 std::sync 写锁，最后才 await
    // 落盘。`std::sync::RwLockWriteGuard` 跨 .await 会让 future 变成 !Send，
    // 破坏 tokio 多线程 runtime 的 spawn 约束；这里用块作用域把锁卫在 await 之前 drop。
    let cfg_clone = {
        let mut cfg = state.config.write().unwrap();
        cfg.auto_backup_enabled = req.enabled;
        cfg.auto_backup_cron = req.cron;
        if let Some(max_files) = req.max_files {
            if max_files == 0 {
                return Err(AppError::BadRequest("保留数量不能为 0".to_string()));
            }
            cfg.auto_backup_max_files = max_files;
        }
        cfg.normalize_paths();
        cfg.clamp_execution_timeout_secs();
        cfg.clone()
    };

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

    // 拷出 path 释放读锁,避免 spawn_blocking().await 持锁卫跨 .await。
    let path = {
        let cfg = state.config.read().unwrap();
        let db_path = PathBuf::from(&cfg.db_path);
        let db_filename = db_path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        db_backup_dir(&db_filename).join(&req.filename)
    };

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

    // 拷出 path 释放读锁,避免 spawn_blocking().await 持锁卫跨 .await。
    let path = {
        let cfg = state.config.read().unwrap();
        let db_path = PathBuf::from(&cfg.db_path);
        let db_filename = db_path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        db_backup_dir(&db_filename).join(&query.filename)
    };

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

    // 读取数据库文件
    let db_data = std::fs::read(&db_path)
        .map_err(|e| format!("Failed to read database: {}", e))?;

    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let backup_filename = format!("{}-{}.zip", db_filename, timestamp);
    let backup_path = dir.join(&backup_filename);

    // 创建 zip 文件，使用最强压缩级别 (level 9)
    let file = std::fs::File::create(&backup_path)
        .map_err(|e| format!("Failed to create backup file: {}", e))?;
    let mut zip = ZipWriter::new(file);
    let options = FileOptions::<()>::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(BACKUP_COMPRESSION_LEVEL)
        .unix_permissions(0o644);

    zip.start_file("database.db", options)
        .map_err(|e| format!("Failed to start zip entry: {}", e))?;
    zip.write_all(&db_data)
        .map_err(|e| format!("Failed to write to zip: {}", e))?;
    zip.finish()
        .map_err(|e| format!("Failed to finish zip: {}", e))?;

    // 定时备份执行后仅保留最近 max_files 份 .zip，避免磁盘被旧备份占满；
    // 与 trigger_local_backup / trigger_todo_backup / trigger_skill_backup
    // 共享同一份 zip 清理逻辑，确保行为一致。
    cleanup_old_zip_backups(&dir, max_files);

    Ok(format!("Auto backup: {}", backup_path.display()))
}

/// 清理早于指定天数的 execution_logs 记录
pub async fn cleanup_old_logs(db: &Database, days: usize) -> Result<u64, String> {
    // 边界校验：days 必须在 1-3650 范围内
    let days = days.clamp(1, 3650);

    let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);
    let cutoff_str = cutoff.format("%Y-%m-%dT%H:%M:%S").to_string();

    // 使用 DELETE FROM execution_logs WHERE timestamp < cutoff
    // 由于 timestamp 格式是 ISO8601 字符串，可以直接字符串比较
    let sql = format!(
        "DELETE FROM execution_logs WHERE timestamp < '{}'",
        cutoff_str
    );

    db.exec(&sql).await.map_err(|e| format!("Failed to cleanup logs: {}", e))?;

    // 获取实际删除的行数
    let changes: u64 = db.conn
        .query_one(Statement::from_string(DbBackend::Sqlite, "SELECT changes()".to_string()))
        .await
        .map_err(|e| format!("Failed to get changes count: {}", e))?
        .and_then(|row| row.try_get_by_index::<i64>(0).ok())
        .unwrap_or(0) as u64;

    Ok(changes)
}

/// 通用 zip 备份清理：按创建时间倒序保留最近 `keep` 个 .zip 文件，其余删除。
///
/// 取代原先 db / todo / skill 三处完全相同的 `cleanup_old_*_backups` 实现：
/// 三个场景（数据库目录、Todo 目录、Skill 目录）共享同一份清理语义，
/// 原本的逐文件复制导致任何 bug 修复都得在三处同步，DRY 违反且容易遗漏。
///
/// 排序策略：按创建时间倒序（新建在前），确保删除旧备份、保留新备份。
/// 若文件系统不支持创建时间（如某些网络文件系统），回退到修改时间；
/// 若仍不可用，以文件名字母序作为最终兜底，确保跨平台行为确定且可复现。
fn cleanup_old_zip_backups(dir: &PathBuf, keep: usize) {
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

    // 优先使用创建时间倒序排列，最新的排最前；
    // 若文件系统不支持 created（如部分网络文件系统）则回退到 modified；
    // 时间仍不可用时以文件名字母序作为稳定兜底，避免跨平台非确定性删除。
    files.sort_by(|a, b| {
        let a_meta = std::fs::metadata(a).ok();
        let b_meta = std::fs::metadata(b).ok();
        let a_time = a_meta.as_ref().and_then(|m| m.created().ok())
            .or_else(|| a_meta.as_ref().and_then(|m| m.modified().ok()));
        let b_time = b_meta.as_ref().and_then(|m| m.created().ok())
            .or_else(|| b_meta.as_ref().and_then(|m| m.modified().ok()));
        // 时间相同时以文件名作为稳定排序键，确保结果可复现
        b_time.cmp(&a_time).then_with(|| b.file_name().cmp(&a.file_name()))
    });

    for old_file in files.iter().skip(keep) {
        std::fs::remove_file(old_file).ok();
    }
}

/// 启动自动备份定时任务
pub fn start_auto_backup(
    cron_expr: &str,
    db: std::sync::Arc<Database>,
    config: std::sync::Arc<std::sync::RwLock<crate::config::Config>>,
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
            let (db_path, max_files, cleanup_days) = {
                let cfg = config.read().unwrap();
                (cfg.db_path.clone(), cfg.auto_backup_max_files, cfg.auto_cleanup_logs_days)
            };

            match tokio::task::spawn_blocking(move || perform_database_backup(&db_path, max_files)).await {
                Ok(Ok(msg)) => {
                    tracing::info!("{}", msg);
                    // 执行日志清理
                    if let Some(days) = cleanup_days {
                        let db = db.clone();
                        match cleanup_old_logs(&db, days).await {
                            Ok(count) => tracing::info!("Cleaned up {} old log entries", count),
                            Err(e) => tracing::error!("Log cleanup failed: {}", e),
                        }
                    }
                }
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
    // 块作用域拷出 owned 值,锁卫立即 drop,避免后续 spawn_blocking().await 持 std 读锁卫。
    let (auto_backup_enabled, auto_backup_cron, auto_backup_max_files) = {
        let cfg = state.config.read().unwrap();
        (
            cfg.auto_todo_backup_enabled,
            cfg.auto_todo_backup_cron.clone(),
            cfg.auto_todo_backup_max_files,
        )
    };

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
    // 块作用域拷出 owned 值,锁卫立即 drop,避免后续 .await 持 std 读锁卫。
    let max_files = {
        let cfg = state.config.read().unwrap();
        cfg.auto_todo_backup_max_files
    };

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

    // 备份成功写入后清理旧备份，仅在压缩文件落盘后才执行删除，
    // 防止备份过程中出错导致旧备份被误清。
    // 放入 spawn_blocking 执行文件 I/O，统一与 perform_todo_backup_async 行为对齐，
    // 避免同步 I/O 阻塞 tokio 运行时线程池。
    let dir_for_cleanup = dir.clone();
    tokio::task::spawn_blocking(move || {
        cleanup_old_zip_backups(&dir_for_cleanup, max_files);
    })
    .await
    .map_err(|e| AppError::Internal(format!("Cleanup task join error: {}", e)))?;

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

    // 块作用域内 clone 出 owned 值,await 落盘前写锁已 drop,
    // 避免 std::sync::RwLockWriteGuard 跨 .await 让 future 变成 !Send。
    let cfg_clone = {
        let mut cfg = state.config.write().unwrap();
        cfg.auto_todo_backup_enabled = req.enabled;
        cfg.auto_todo_backup_cron = req.cron;
        if let Some(max_files) = req.max_files {
            if max_files == 0 {
                return Err(AppError::BadRequest("保留数量不能为 0".to_string()));
            }
            cfg.auto_todo_backup_max_files = max_files;
        }
        cfg.normalize_paths();
        cfg.clamp_execution_timeout_secs();
        cfg.clone()
    };

    tokio::task::spawn_blocking(move || cfg_clone.save())
        .await
        .map_err(|e| AppError::Internal(format!("Join error: {}", e)))?
        .map_err(|e| AppError::Internal(format!("Failed to save config: {}", e)))?;

    Ok(ApiResponse::ok("Todo 自动备份配置已更新".to_string()))
}

/// Start Todo auto backup scheduler
pub fn start_todo_auto_backup(
    db: std::sync::Arc<crate::db::Database>,
    config: std::sync::Arc<std::sync::RwLock<crate::config::Config>>,
) -> Result<(), String> {

    let db_clone = db.clone();
    tokio::spawn(async move {
        loop {
            // 关键:std::sync 读锁卫不能跨 .await(否则 future 变 !Send,无法
            // tokio::spawn)。这里用显式 if-else + 块作用域把 disabled 分支
            // 的 sleep().await 放在 cfg 锁卫作用域外,正常分支也把所有需要的
            // 字段拷出 owned 值,锁卫在块作用域末尾 drop,后续的 sleep().await
            // 不再持有锁。
            let (enabled, next_delay) = {
                let enabled = {
                    let cfg = config.read().unwrap();
                    cfg.auto_todo_backup_enabled
                };
                if !enabled {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    continue;
                }
                let (enabled, delay) = {
                    let cfg = config.read().unwrap();
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
                (enabled, delay)
            };

            tokio::time::sleep(next_delay).await;

            // Skip backup if disabled while sleeping
            if !enabled {
                continue;
            }

            let db = db_clone.clone();
            let max_files = {
                let cfg = config.read().unwrap();
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

    // 定时 Todo 备份成功后清理旧备份，限定 spawn_blocking 内执行
    // 文件 I/O，避免阻塞 tokio 运行时线程池。
    // 使用通用 zip 清理 helper 统一保留策略，与手动触发行为对齐。
    let dir_for_cleanup = dir.clone();
    tokio::task::spawn_blocking(move || {
        cleanup_old_zip_backups(&dir_for_cleanup, max_files);
    }).await
    .map_err(|e| format!("Task join error: {}", e))?;

    Ok(format!("Auto Todo backup: {}", backup_path_for_display))
}

// ============ 日志清理 ============

#[derive(Serialize)]
pub struct LogCleanupStatus {
    pub cleanup_days: Option<usize>,
}

pub async fn get_log_cleanup_status(
    State(state): State<AppState>,
) -> Result<ApiResponse<LogCleanupStatus>, AppError> {
    let cfg = state.config.read().unwrap();
    Ok(ApiResponse::ok(LogCleanupStatus {
        cleanup_days: cfg.auto_cleanup_logs_days,
    }))
}

#[derive(Deserialize)]
pub struct UpdateLogCleanupRequest {
    pub days: Option<usize>,
}

pub async fn update_log_cleanup(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<UpdateLogCleanupRequest>,
) -> Result<ApiResponse<String>, AppError> {
    if let Some(days) = req.days {
        if days == 0 {
            return Err(AppError::BadRequest("保留天数不能为 0".to_string()));
        }
    }

    // 块作用域内 clone 出 owned 值,await 落盘前写锁已 drop。
    let cfg_clone = {
        let mut cfg = state.config.write().unwrap();
        cfg.auto_cleanup_logs_days = req.days;
        cfg.clone()
    };

    tokio::task::spawn_blocking(move || cfg_clone.save())
        .await
        .map_err(|e| AppError::Internal(format!("Join error: {}", e)))?
        .map_err(|e| AppError::Internal(format!("Failed to save config: {}", e)))?;

    Ok(ApiResponse::ok("日志清理配置已更新".to_string()))
}

pub async fn trigger_log_cleanup(
    State(state): State<AppState>,
) -> Result<ApiResponse<String>, AppError> {
    let days = {
        let cfg = state.config.read().unwrap();
        cfg.auto_cleanup_logs_days
    };

    let days = days.ok_or_else(|| AppError::BadRequest("日志清理未启用，请先设置保留天数".to_string()))?;

    let db = state.db.clone();
    let count = cleanup_old_logs(&db, days).await
        .map_err(|e| AppError::Internal(format!("Cleanup failed: {}", e)))?;

    Ok(ApiResponse::ok(format!("已清理 {} 条日志记录", count)))
}

// ============ Skill 备份 ============

/// Skill 备份目录
fn skill_backup_dir() -> PathBuf {
    backup_dir().join("skills")
}

/// 获取所有执行器的 skills 目录
fn all_executor_skills_dirs() -> Vec<(&'static str, PathBuf)> {
    let home = dirs::home_dir();
    if home.is_none() {
        return vec![];
    }
    let home = home.unwrap();
    vec![
        ("claudecode", home.join(".claude").join("skills")),
        ("hermes", home.join(".hermes").join("skills")),
        ("codex", home.join(".codex").join("skills")),
        ("codebuddy", home.join(".codebuddy").join("skills")),
        ("opencode", home.join(".opencode").join("skills")),
        ("atomcode", home.join(".atomcode").join("skills")),
        ("kimi", home.join(".kimi").join("skills")),
        ("mobilecoder", home.join(".mobile-coder").join("skills")),
    ]
}

/// 递归统计目录下包含 SKILL.md 的目录数量（跟随符号链接）
fn count_skills_recursive(dir: &std::path::Path) -> usize {
    if !dir.is_dir() {
        return 0;
    }
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            // 使用 is_dir() 会跟随符号链接
            if path.is_dir() {
                // 使用 canonicalize 获取真实路径，以正确处理符号链接
                let real_path = std::fs::canonicalize(&path).unwrap_or(path.clone());
                if real_path.join("SKILL.md").exists() {
                    count += 1;
                } else {
                    // 递归检查子目录
                    count += count_skills_recursive(&real_path);
                }
            }
        }
    }
    count
}

/// Skill 备份状态
#[derive(Serialize)]
pub struct SkillBackupStatus {
    pub auto_backup_enabled: bool,
    pub auto_backup_cron: String,
    pub auto_backup_max_files: usize,
    pub last_backup: Option<String>,
    pub files: Vec<BackupFile>,
    pub executor_skills: Vec<ExecutorSkillInfo>,
}

/// 执行器 skills 信息
#[derive(Serialize)]
pub struct ExecutorSkillInfo {
    pub executor: String,
    pub skills_count: usize,
    pub skills_dir_exists: bool,
}

/// 获取 Skill 备份状态
pub async fn get_skill_backup_status(
    State(state): State<AppState>,
) -> Result<ApiResponse<SkillBackupStatus>, AppError> {
    // 块作用域拷出 owned 值,锁卫立即 drop,避免后续 .await 持 std 读锁卫。
    let (auto_backup_enabled, auto_backup_cron, auto_backup_max_files) = {
        let cfg = state.config.read().unwrap();
        (
            cfg.auto_skill_backup_enabled,
            cfg.auto_skill_backup_cron.clone(),
            cfg.auto_skill_backup_max_files,
        )
    };

    let dir = skill_backup_dir();

    // 获取执行器 skills 信息
    let executor_skills = tokio::task::spawn_blocking(move || {
        all_executor_skills_dirs()
            .into_iter()
            .map(|(name, path)| {
                let skills_count = if path.exists() {
                    count_skills_recursive(&path)
                } else {
                    0
                };
                ExecutorSkillInfo {
                    executor: name.to_string(),
                    skills_count,
                    skills_dir_exists: path.exists(),
                }
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| AppError::Internal(format!("Task join error: {}", e)))?;

    // 获取备份文件列表
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

    Ok(ApiResponse::ok(SkillBackupStatus {
        auto_backup_enabled,
        auto_backup_cron,
        auto_backup_max_files,
        last_backup,
        files,
        executor_skills,
    }))
}

/// 复制目录到目标位置（用于备份），遇到错误时记录并继续
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<u32> {
    if !src.is_dir() {
        return Ok(0);
    }
    if let Err(e) = std::fs::create_dir_all(dst) {
        tracing::warn!("Failed to create directory {:?}: {}", dst, e);
        return Err(e);
    }
    let mut count = 0u32;
    let entries = match std::fs::read_dir(src) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Failed to read directory {:?}: {}", src, e);
            return Err(e);
        }
    };
    for entry in entries.flatten() {
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            match copy_dir_recursive(&src_path, &dst_path) {
                Ok(n) => count += n,
                Err(e) => tracing::warn!("Failed to backup directory {:?}: {}", src_path, e),
            }
        } else {
            match std::fs::copy(&src_path, &dst_path) {
                Ok(_) => count += 1,
                Err(e) => tracing::warn!("Failed to copy file {:?}: {}", src_path, e),
            }
        }
    }
    Ok(count)
}

/// 手动触发 Skill 备份
pub async fn trigger_skill_backup(
    State(state): State<AppState>,
) -> Result<ApiResponse<String>, AppError> {
    // 块作用域拷出 owned 值,锁卫立即 drop,避免后续 .await 持 std 读锁卫。
    let max_files = {
        let cfg = state.config.read().unwrap();
        cfg.auto_skill_backup_max_files
    };

    let dir = skill_backup_dir();
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let backup_filename = format!("skill-backup-{}.zip", timestamp);
    let backup_path = dir.join(&backup_filename);
    let backup_path_display = backup_path.display().to_string();

    // 获取所有执行器的 skills 目录
    let executor_dirs = all_executor_skills_dirs();

    // 在 /tmp 创建临时聚合目录
    let temp_base = std::env::temp_dir().join(format!("ntd-skill-backup-{}", timestamp));
    let temp_base_clone = temp_base.clone();

    // 复制各个执行器的 skills 到临时目录
    let copied_count = tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&temp_base_clone)?;

        let mut total_files = 0u32;
        for (executor_name, skills_path) in &executor_dirs {
            if skills_path.exists() {
                let executor_temp_dir = temp_base_clone.join(executor_name);
                let count = copy_dir_recursive(skills_path, &executor_temp_dir)?;
                total_files += count;
            }
        }
        Ok::<u32, std::io::Error>(total_files)
    })
    .await
    .map_err(|e| AppError::Internal(format!("Task join error: {}", e)))?
    .map_err(|e| AppError::Internal(format!("Failed to copy skills: {}", e)))?;

    // 创建 zip 文件
    let temp_base_for_zip = temp_base.clone();
    let backup_path_clone = backup_path.clone();
    let dir_clone = dir.clone();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&dir_clone)?;

        let file = std::fs::File::create(&backup_path_clone)?;
        let mut zip = ZipWriter::new(file);
        let options = FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);

        // 将临时目录打包为 zip
        add_dir_to_zip_skill(&mut zip, &temp_base_for_zip, "", &options)?;

        zip.finish()?;

        // 清理临时目录
        std::fs::remove_dir_all(&temp_base_for_zip).ok();

        Ok::<(), std::io::Error>(())
    })
    .await
    .map_err(|e| AppError::Internal(format!("Task join error: {}", e)))?
    .map_err(|e| AppError::Internal(format!("Failed to create zip: {}", e)))?;

    // 手动 Skill 备份成功后清理旧 zip 保留文件，防止积压堆积；
    // 仅在压缩文件落盘后执行清理，避免备份失败时误删历史数据。
    // 放入 spawn_blocking 执行文件 I/O，统一与 perform_skill_backup_async 行为对齐，
    // 避免同步 I/O 阻塞 tokio 运行时线程池。
    let dir_for_cleanup = dir.clone();
    tokio::task::spawn_blocking(move || {
        cleanup_old_zip_backups(&dir_for_cleanup, max_files);
    })
    .await
    .map_err(|e| AppError::Internal(format!("Cleanup task join error: {}", e)))?;

    Ok(ApiResponse::ok(format!("备份成功: {} ({} 个文件)", backup_path_display, copied_count)))
}

/// 将目录添加到 zip
fn add_dir_to_zip_skill<W: std::io::Write + std::io::Seek>(
    zip_writer: &mut ZipWriter<W>,
    dir: &std::path::Path,
    prefix: &str,
    options: &FileOptions<()>,
) -> std::io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = if prefix.is_empty() {
            path.file_name().unwrap().to_string_lossy().to_string()
        } else {
            format!("{}/{}", prefix, path.file_name().unwrap().to_string_lossy())
        };

        if path.is_dir() {
            add_dir_to_zip_skill(zip_writer, &path, &name, options)?;
        } else {
            zip_writer.start_file(name, *options)?;
            let mut file = std::fs::File::open(&path)?;
            std::io::copy(&mut file, zip_writer)?;
        }
    }

    Ok(())
}

/// 更新 Skill 自动备份配置
#[derive(Deserialize)]
pub struct UpdateSkillAutoBackupRequest {
    pub enabled: bool,
    pub cron: String,
    pub max_files: Option<usize>,
}

pub async fn update_skill_auto_backup(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<UpdateSkillAutoBackupRequest>,
) -> Result<ApiResponse<String>, AppError> {
    // 验证 cron 表达式
    if req.enabled {
        let schedule = cron::Schedule::from_str(&req.cron)
            .map_err(|e| AppError::BadRequest(format!("Invalid cron expression: {}", e)))?;
        schedule.upcoming(chrono::Utc).next()
            .ok_or_else(|| AppError::BadRequest("Cron expression has no future executions".to_string()))?;
    }

    // 块作用域内 clone 出 owned 值,await 落盘前写锁已 drop。
    let cfg_clone = {
        let mut cfg = state.config.write().unwrap();
        cfg.auto_skill_backup_enabled = req.enabled;
        cfg.auto_skill_backup_cron = req.cron;
        if let Some(max_files) = req.max_files {
            if max_files == 0 {
                return Err(AppError::BadRequest("保留数量不能为 0".to_string()));
            }
            cfg.auto_skill_backup_max_files = max_files;
        }
        cfg.normalize_paths();
        cfg.clamp_execution_timeout_secs();
        cfg.clone()
    };

    tokio::task::spawn_blocking(move || cfg_clone.save())
        .await
        .map_err(|e| AppError::Internal(format!("Join error: {}", e)))?
        .map_err(|e| AppError::Internal(format!("Failed to save config: {}", e)))?;

    Ok(ApiResponse::ok("Skill 自动备份配置已更新".to_string()))
}

/// 删除 Skill 备份文件
#[derive(Deserialize)]
pub struct DeleteSkillBackupRequest {
    pub filename: String,
}

pub async fn delete_skill_backup_file(
    State(_state): State<AppState>,
    axum::Json(req): axum::Json<DeleteSkillBackupRequest>,
) -> Result<ApiResponse<String>, AppError> {
    // 安全检查：文件名不能包含路径分隔符
    if req.filename.contains('/') || req.filename.contains('\\') || req.filename.contains("..") {
        return Err(AppError::BadRequest("Invalid filename".to_string()));
    }
    let path = skill_backup_dir().join(&req.filename);
    if !path.exists() {
        return Err(AppError::NotFound);
    }
    tokio::task::spawn_blocking(move || std::fs::remove_file(&path))
        .await
        .map_err(|e| AppError::Internal(format!("Task join error: {}", e)))?
        .map_err(|e| AppError::Internal(format!("Failed to delete: {}", e)))?;
    Ok(ApiResponse::ok("已删除".to_string()))
}

/// 下载 Skill 备份文件
#[derive(Deserialize)]
pub struct DownloadSkillBackupQuery {
    pub filename: String,
}

pub async fn download_skill_backup_file(
    Query(query): Query<DownloadSkillBackupQuery>,
) -> Result<impl IntoResponse, AppError> {
    if query.filename.contains('/') || query.filename.contains('\\') || query.filename.contains("..") {
        return Err(AppError::BadRequest("Invalid filename".to_string()));
    }
    let path = skill_backup_dir().join(&query.filename);
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

/// 清理旧 Skill 备份
/// Start Skill auto backup scheduler
pub fn start_skill_auto_backup(
    config: std::sync::Arc<std::sync::RwLock<crate::config::Config>>,
) -> Result<(), String> {
    tokio::spawn(async move {
        loop {
            // 关键:std::sync 读锁卫不能跨 .await。用显式 if-else + 块作用域
            // 把 disabled 分支的 sleep().await 放在 cfg 锁卫作用域外。
            let (_enabled, next_delay) = {
                let enabled = {
                    let cfg = config.read().unwrap();
                    cfg.auto_skill_backup_enabled
                };
                if !enabled {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    continue;
                }
                let (enabled, delay) = {
                    let cfg = config.read().unwrap();
                    let schedule = cron::Schedule::from_str(&cfg.auto_skill_backup_cron)
                        .unwrap_or_else(|_| cron::Schedule::from_str("0 0 5 * * *").unwrap());
                    let next = schedule.upcoming(chrono::Utc).next();
                    let delay = match next {
                        Some(dt) => {
                            let now = chrono::Utc::now();
                            (dt - now).to_std().unwrap_or(std::time::Duration::from_secs(60))
                        }
                        None => std::time::Duration::from_secs(3600),
                    };
                    (cfg.auto_skill_backup_enabled, delay)
                };
                (enabled, delay)
            };

            tokio::time::sleep(next_delay).await;

            // Sleep 之后重新检查 enabled 状态，避免使用过期值
            let enabled_now = {
                let cfg = config.read().unwrap();
                cfg.auto_skill_backup_enabled
            };
            if !enabled_now {
                continue;
            }

            let max_files = {
                let cfg = config.read().unwrap();
                cfg.auto_skill_backup_max_files
            };

            match perform_skill_backup_async(max_files).await {
                Ok(msg) => tracing::info!("{}", msg),
                Err(e) => tracing::error!("Auto Skill backup failed: {}", e),
            }
        }
    });

    Ok(())
}

/// 启动 AI 使用统计自动归档定时任务
pub fn start_usage_stats_archival(
    db: std::sync::Arc<Database>,
    config: std::sync::Arc<std::sync::RwLock<crate::config::Config>>,
) -> Result<(), String> {
    let db_clone = db.clone();
    tokio::spawn(async move {
        loop {
            // 关键:std::sync 读锁卫不能跨 .await。用显式 if-else + 块作用域
            // 把 disabled 分支的 sleep().await 放在 cfg 锁卫作用域外。
            let (_enabled, next_delay) = {
                let enabled = {
                    let cfg = config.read().unwrap();
                    cfg.auto_usage_stats_enabled
                };
                if !enabled {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    continue;
                }
                let (enabled, delay) = {
                    let cfg = config.read().unwrap();
                    let schedule = cron::Schedule::from_str(&cfg.auto_usage_stats_cron)
                        .unwrap_or_else(|_| cron::Schedule::from_str("0 0 1 * * *").unwrap());
                    let next = schedule.upcoming(chrono::Utc).next();
                    let delay = match next {
                        Some(dt) => {
                            let now = chrono::Utc::now();
                            (dt - now).to_std().unwrap_or(std::time::Duration::from_secs(60))
                        }
                        None => std::time::Duration::from_secs(3600),
                    };
                    (cfg.auto_usage_stats_enabled, delay)
                };
                (enabled, delay)
            };

            tokio::time::sleep(next_delay).await;

            let enabled_now = {
                let cfg = config.read().unwrap();
                cfg.auto_usage_stats_enabled
            };
            if !enabled_now {
                continue;
            }

            let db = db_clone.clone();
            let service = UsageStatsService::new(db.clone());

            match archive_yesterday_stats(&service).await {
                Ok(msg) => tracing::info!("{}", msg),
                Err(e) => tracing::error!("Auto usage stats archival failed: {}", e),
            }
        }
    });

    Ok(())
}

/// 归档昨天的统计数据
async fn archive_yesterday_stats(service: &UsageStatsService) -> Result<String, String> {
    // Get yesterday's date
    let yesterday = (chrono::Utc::now() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();

    let entries = service.collect_all_entries().await;

    if entries.is_empty() {
        return Ok(format!("Usage stats archival: no data found for {}", yesterday));
    }

    // Filter entries for yesterday
    let yesterday_entries: Vec<_> = entries.iter()
        .filter(|e| e.date == yesterday)
        .cloned()
        .collect();

    if yesterday_entries.is_empty() {
        return Ok(format!("Usage stats archival: no usage data for {}", yesterday));
    }

    // Aggregate by day
    let (daily_stats, breakdowns) = UsageStatsService::aggregate_by_day(&yesterday_entries);

    // Save to database
    service.save_daily_stats(&daily_stats, &breakdowns).await?;

    Ok(format!("Usage stats archival: saved {} stats for {}", daily_stats.len(), yesterday))
}

/// 执行 Skill 备份
async fn perform_skill_backup_async(max_files: usize) -> Result<String, String> {
    let dir = skill_backup_dir();
    let dir_clone = dir.clone();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&dir_clone)
            .map_err(|e| format!("Failed to create backup dir: {}", e))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))??;

    // 获取所有执行器的 skills 目录
    let executor_dirs = all_executor_skills_dirs();

    // 在 /tmp 创建临时聚合目录
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let temp_base = std::env::temp_dir().join(format!("ntd-skill-backup-{}", timestamp));

    // 复制各个执行器的 skills 到临时目录
    let temp_base_clone = temp_base.clone();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&temp_base_clone)?;

        for (executor_name, skills_path) in &executor_dirs {
            if skills_path.exists() {
                let executor_temp_dir = temp_base_clone.join(executor_name);
                copy_dir_recursive(skills_path, &executor_temp_dir)?;
            }
        }
        Ok::<(), std::io::Error>(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
    .map_err(|e| format!("Failed to copy skills: {}", e))?;

    let backup_filename = format!("skill-backup-{}.zip", timestamp);
    let backup_path = dir.join(&backup_filename);
    let backup_path_for_display = backup_path.display().to_string();

    // 创建 zip 文件
    let temp_base_for_zip = temp_base.clone();
    let backup_path_clone = backup_path.clone();
    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::create(&backup_path_clone)?;
        let mut zip = ZipWriter::new(file);
        let options = FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);

        add_dir_to_zip_skill(&mut zip, &temp_base_for_zip, "", &options)?;
        zip.finish()?;

        // 清理临时目录
        std::fs::remove_dir_all(&temp_base_for_zip).ok();

        Ok::<(), std::io::Error>(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
    .map_err(|e| format!("Failed to create zip: {}", e))?;

    // 定时 Skill 备份成功后清理旧备份，限定 spawn_blocking 内执行
    // 文件 I/O，避免阻塞 tokio 运行时线程池。
    // 使用通用 zip 清理 helper 统一保留策略，与手动触发行为对齐。
    let dir_for_cleanup = dir.clone();
    tokio::task::spawn_blocking(move || {
        cleanup_old_zip_backups(&dir_for_cleanup, max_files);
    }).await
    .map_err(|e| format!("Task join error: {}", e))?;

    Ok(format!("Auto Skill backup: {}", backup_path_for_display))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::thread;
    use std::time::Duration;

    /// 创建一个空 zip 文件并返回路径；空 zip 是合法格式，避免 `is_zip` 类型检查失败。
    fn make_empty_zip(dir: &std::path::Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let file = fs::File::create(&path).unwrap();
        let zip = zip::ZipWriter::new(file);
        zip.finish().unwrap();
        path
    }

    /// 验证 `keep >= 总数` 时不会删除任何文件（保留语义不变）。
    #[test]
    fn cleanup_old_zip_backups_keeps_all_when_count_le_keep() {
        // 临时目录隔离，测试结束自动回收
        let dir = std::env::temp_dir().join(format!(
            "ntd-backup-test-keep-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        // 仅创建 2 个 zip，keep=5 ⇒ 保留全部
        make_empty_zip(&dir, "a.zip");
        make_empty_zip(&dir, "b.zip");

        cleanup_old_zip_backups(&dir, 5);

        // 不应有任何文件被删除
        let remaining: Vec<_> = fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(remaining.len(), 2, "keep 大于总数时应保留全部文件");

        fs::remove_dir_all(&dir).ok();
    }

    /// 验证目录不存在时安全 no-op，不 panic。
    #[test]
    fn cleanup_old_zip_backups_noop_when_dir_missing() {
        let dir = std::env::temp_dir().join(format!(
            "ntd-backup-test-missing-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        // 确保目录不存在
        let _ = fs::remove_dir_all(&dir);
        assert!(!dir.exists());

        // 不存在的目录应当直接返回，不能 panic
        cleanup_old_zip_backups(&dir, 3);
    }

    /// 验证非 zip 文件不会被清理函数当作备份删除。
    #[test]
    fn cleanup_old_zip_backups_ignores_non_zip_files() {
        let dir = std::env::temp_dir().join(format!(
            "ntd-backup-test-mix-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        // 2 个 zip + 2 个非 zip，keep=1 ⇒ 只保留最新 zip，删除其余 zip，不动其它
        make_empty_zip(&dir, "old.zip");
        make_empty_zip(&dir, "newer.zip");
        fs::write(dir.join("readme.txt"), "hi").unwrap();
        fs::write(dir.join("data.json"), "{}").unwrap();

        // 让两个 zip 的 created 时间有可分辨的差异
        thread::sleep(Duration::from_millis(50));
        make_empty_zip(&dir, "newest.zip");

        cleanup_old_zip_backups(&dir, 1);

        let names: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();

        // 非 zip 文件不应被删除
        assert!(names.iter().any(|n| n == "readme.txt"));
        assert!(names.iter().any(|n| n == "data.json"));
        // zip 数量应被裁剪到 keep=1
        let zip_count = names.iter().filter(|n| n.ends_with(".zip")).count();
        assert_eq!(zip_count, 1, "zip 数量应等于 keep=1");
        // 保留的 zip 应该是最新创建的 newest.zip，否则排序或清理逻辑有误
        assert!(names.iter().any(|n| n == "newest.zip"),
            "应保留最新创建的 newest.zip，实际保留: {:?}", names);
        // 旧 zip 文件应被删除
        assert!(!names.iter().any(|n| n == "old.zip"), "old.zip 应被删除");
        assert!(!names.iter().any(|n| n == "newer.zip"), "newer.zip 应被删除");

        fs::remove_dir_all(&dir).ok();
    }
}
