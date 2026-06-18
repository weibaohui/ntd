//! AppState 子状态分组 —— Facade Pattern 实现
//!
//! ## 设计背景
//! AppState 有 9 个字段，导致：
//! - handler 方法签名过长
//! - 隐式依赖不清晰
//! - 新增字段需要改多处
//!
//! ## 解决方案
//! 按领域将 AppState 拆分为多个子状态：
//! - ExecutionState: 执行相关（db + executor_registry + tx + task_manager）
//! - FeishuState: 飞书相关（feishu_listener + feishu_push_mutator）
//! - ConfigState: 配置（config）
//! - SchedulerState: 调度（scheduler）
//! - HookState: 钩子（hook_service）
//!
//! ## 使用方式
//! ```rust,no_run
//! // 新增 handler 使用子状态：只需要 State<ExecutionState> 一个参数，
//! // 比旧的 State<AppState>（9 个字段）清爽很多，编译期也能阻挡意外依赖。
//! async fn my_handler() {
//!     // ... handler 实现 ...
//! }
//!
//! // 现有 AppState 保持向后兼容，逐步迁移
//! ```

use std::sync::Arc;
use tokio::sync::broadcast;

use crate::db::Database;
use crate::adapters::ExecutorRegistry;
use crate::scheduler::TodoScheduler;
use crate::task_manager::TaskManager;
use crate::services::feishu_listener::FeishuListener;
use crate::services::feishu_push::PushConfigUpdate;
use crate::hooks::HookService;
use crate::config::Config;

// ============================================================================
// 子状态定义
// ============================================================================

/// 执行相关状态 — 包含执行、数据库、任务管理
#[derive(Clone)]
pub struct ExecutionState {
    pub db: Arc<Database>,
    pub executor_registry: Arc<ExecutorRegistry>,
    pub tx: broadcast::Sender<crate::handlers::ExecEvent>,
    pub task_manager: Arc<TaskManager>,
}

impl ExecutionState {
    pub fn new(
        db: Arc<Database>,
        executor_registry: Arc<ExecutorRegistry>,
        tx: broadcast::Sender<crate::handlers::ExecEvent>,
        task_manager: Arc<TaskManager>,
    ) -> Self {
        Self {
            db,
            executor_registry,
            tx,
            task_manager,
        }
    }
}

/// 飞书相关状态
#[derive(Clone)]
pub struct FeishuState {
    pub feishu_listener: Arc<FeishuListener>,
    pub feishu_push_mutator: broadcast::Sender<PushConfigUpdate>,
}

impl FeishuState {
    pub fn new(
        feishu_listener: Arc<FeishuListener>,
        feishu_push_mutator: broadcast::Sender<PushConfigUpdate>,
    ) -> Self {
        Self {
            feishu_listener,
            feishu_push_mutator,
        }
    }
}

/// 配置状态
#[derive(Clone)]
pub struct ConfigState {
    pub config: Arc<std::sync::RwLock<Config>>,
}

impl ConfigState {
    pub fn new(config: Arc<std::sync::RwLock<Config>>) -> Self {
        Self { config }
    }
}

/// 调度器状态
#[derive(Clone)]
pub struct SchedulerState {
    pub scheduler: Arc<TodoScheduler>,
}

impl SchedulerState {
    pub fn new(scheduler: Arc<TodoScheduler>) -> Self {
        Self { scheduler }
    }
}

/// 钩子服务状态
#[derive(Clone)]
pub struct HookState {
    pub hook_service: Arc<HookService>,
}

impl HookState {
    pub fn new(hook_service: Arc<HookService>) -> Self {
        Self { hook_service }
    }
}

// ============================================================================
// 从 AppState 提取子状态
// ============================================================================

impl From<&crate::handlers::AppState> for ExecutionState {
    fn from(state: &crate::handlers::AppState) -> Self {
        Self::new(
            state.db.clone(),
            state.executor_registry.clone(),
            state.tx.clone(),
            state.task_manager.clone(),
        )
    }
}

impl From<&crate::handlers::AppState> for FeishuState {
    fn from(state: &crate::handlers::AppState) -> Self {
        Self::new(
            state.feishu_listener.clone(),
            state.feishu_push_mutator.clone(),
        )
    }
}

impl From<&crate::handlers::AppState> for ConfigState {
    fn from(state: &crate::handlers::AppState) -> Self {
        Self::new(state.config.clone())
    }
}

impl From<&crate::handlers::AppState> for SchedulerState {
    fn from(state: &crate::handlers::AppState) -> Self {
        Self::new(state.scheduler.clone())
    }
}

impl From<&crate::handlers::AppState> for HookState {
    fn from(state: &crate::handlers::AppState) -> Self {
        Self::new(state.hook_service.clone())
    }
}

// ============================================================================
// 便捷构造函数
// ============================================================================

impl crate::handlers::AppState {
    /// 从 AppState 提取执行相关子状态
    pub fn execution_state(&self) -> ExecutionState {
        ExecutionState::from(self)
    }

    /// 从 AppState 提取飞书相关子状态
    pub fn feishu_state(&self) -> FeishuState {
        FeishuState::from(self)
    }

    /// 从 AppState 提取配置子状态
    pub fn config_state(&self) -> ConfigState {
        ConfigState::from(self)
    }

    /// 从 AppState 提取调度器子状态
    pub fn scheduler_state(&self) -> SchedulerState {
        SchedulerState::from(self)
    }

    /// 从 AppState 提取钩子子状态
    pub fn hook_state(&self) -> HookState {
        HookState::from(self)
    }
}
