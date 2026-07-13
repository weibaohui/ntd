use std::sync::Arc;
use tokio::sync::broadcast;

use crate::adapters::ExecutorRegistry;
use crate::config::Config;
use crate::db::Database;
use crate::expert::ExpertIndexManager;
use crate::executor_service::ExecEvent;
use crate::task_manager::TaskManager;

/// Shared context passed to services and schedulers.
/// Reduces boilerplate of passing the same 5 Arc-dependencies everywhere.
///
/// `config` deliberately uses `std::sync::RwLock` rather than
/// `tokio::sync::RwLock`:
///   * Config is read on every hot path (executor dispatch, health check,
///     API request) but mutated only via `PUT /api/config`, so a heavy async
///     lock that needs to be scheduled through the tokio runtime is overkill.
///   * `std::sync::RwLock::read()` is roughly 2-5x faster than
///     `tokio::sync::RwLock::read().await` because it does not cross an
///     await point.
///   * Callers must NOT hold the guard across `.await`. They either copy
///     the needed fields out under the lock and release it before the first
///     await, or use a block scope that drops the guard before any await.
#[derive(Clone)]
pub struct ServiceContext {
    pub db: Arc<Database>,
    pub executor_registry: Arc<ExecutorRegistry>,
    pub tx: broadcast::Sender<ExecEvent>,
    pub task_manager: Arc<TaskManager>,
    pub config: Arc<std::sync::RwLock<Config>>,
    /// 专家索引管理器（内存缓存，启动时从 ~/.ntd/experts/ 加载）
    pub expert_manager: Arc<ExpertIndexManager>,
}
