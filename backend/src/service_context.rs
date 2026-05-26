use std::sync::Arc;
use tokio::sync::broadcast;

use crate::adapters::ExecutorRegistry;
use crate::config::Config;
use crate::db::Database;
use crate::handlers::ExecEvent;
use crate::task_manager::TaskManager;

/// Shared context passed to services and schedulers.
/// Reduces boilerplate of passing the same 5 Arc-dependencies everywhere.
#[derive(Clone)]
pub struct ServiceContext {
    pub db: Arc<Database>,
    pub executor_registry: Arc<ExecutorRegistry>,
    pub tx: broadcast::Sender<ExecEvent>,
    pub task_manager: Arc<TaskManager>,
    pub config: Arc<tokio::sync::RwLock<Config>>,
}
