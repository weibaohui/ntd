# ntd 项目综合优化建议

> ⚠️ **本文为早期分析报告，未实际全部实施**
> 最后核对日期：2026-06-08
>
> 本文档基于代码质量、架构设计和性能优化的深入分析。
>
> 分析日期：2026-04-25
>
> 分析工具：Rust 专家 + 后端架构专家
>
> **重要提示**：文中"实施路线图"原版把所有阶段都标注为 `[x]`（已完成），这与实际情况不符——本文罗列的多数建议是**预期目标**而非已完成项，详见下方"实施路线图（修订）"。

---

## 📊 项目现状总览

**项目类型**：全栈应用（Rust 后端 + React 前端 + Desktop + Feishu Bot）

**技术栈**：Rust + Axum + SQLite

**代码规模**：约 2,319 行（早期估算）

**架构模式**：分层架构（HTTP 层 → 业务逻辑层 → 数据访问层 → 适配器层）

### 整体评分

| 维度 | 评分 | 说明 |
|------|------|------|
| 代码质量 | ⭐⭐⭐☆☆ (3/5) | 基础功能完善，但缺少错误处理规范 |
| 架构设计 | ⭐⭐⭐☆☆ (3/5) | 层次清晰，但缺少服务层抽象 |
| 性能优化 | ⭐⭐⭐☆☆ (3/5) | 异步设计良好，但数据库存在瓶颈 |
| 安全性 | ⭐☆☆☆☆ (1/5) | ⚠️ **完全没有认证授权机制** |
| 可维护性 | ⭐⭐⭐☆☆ (3/5) | 模块职责清晰，但配置管理缺失 |
| 可测试性 | ⭐⭐☆☆☆ (2/5) | 缺少单元测试和依赖注入 |

---

## 🚨 高优先级问题（立即解决）

### 1. 安全性缺失 ⚠️ 严重风险

**问题描述**：
- 项目完全没有认证授权机制
- 任何人都可访问所有 API
- 没有输入验证
- 缺少速率限制和 CORS 配置

**影响**：数据泄露、未授权访问、API 滥用

**解决方案**：

```rust
// 添加 JWT 认证中间件
use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,  // user id
    pub exp: usize,   // expiration time
    pub iat: usize,   // issued at
}

pub struct AuthService {
    jwt_secret: String,
}

impl AuthService {
    pub fn generate_token(&self, user_id: &str) -> Result<String, Error> {
        let now = Utc::now();
        let exp = now + Duration::hours(24);

        let claims = Claims {
            sub: user_id.to_string(),
            exp: exp.timestamp() as usize,
            iat: now.timestamp() as usize,
        };

        encode(&Header::default(), &claims, &EncodingKey::from_secret(self.jwt_secret.as_ref()))
            .map_err(|e| Error::TokenError(e.to_string()))
    }

    pub fn verify_token(&self, token: &str) -> Result<Claims, Error> {
        decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.jwt_secret.as_ref()),
            &Validation::default(),
        )
        .map(|data| data.claims)
        .map_err(|e| Error::TokenError(e.to_string()))
    }
}

// 认证中间件
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req.headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok());

    let token = auth_header
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let claims = state.auth_service.verify_token(token)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    req.extensions_mut().insert(claims);
    Ok(next.run(req).await)
}

// 输入验证
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct CreateTodoRequest {
    #[validate(length(min = 1, max = 200, message = "标题长度必须在1-200之间"))]
    pub title: String,

    #[validate(length(max = 5000, message = "描述长度不能超过5000"))]
    pub description: String,

    #[validate(length(max = 10, message = "标签数量不能超过10"))]
    #[serde(default)]
    pub tag_ids: Vec<i64>,
}

// 速率限制
use tower_governor::{Governor, GovernorConfigBuilder};

pub fn create_rate_limiter() -> Governor {
    let governor_conf = Box::new(
        GovernorConfigBuilder::default()
            .per_millisecond(1000)
            .burst_size(10)
            .finish()
            .unwrap(),
    );
    Governor {
        config: governor_conf,
    }
}

// CORS 配置
use tower_http::cors::CorsLayer;

let cors = CorsLayer::new()
    .allow_origin(Any)
    .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
    .allow_headers([CONTENT_TYPE, AUTHORIZATION]);
```

**需要的依赖**：
```toml
[dependencies]
jsonwebtoken = "9"
validator = "0.16"
tower-governor = "0.4"
```

**实施步骤**：
1. 添加认证相关依赖
2. 实现 AuthService
3. 创建认证中间件
4. 为需要保护的端点添加中间件
5. 实现输入验证
6. 添加速率限制和 CORS 配置

**优先级**：🔴 最高（1周内完成）

**文件位置**：`backend/src/handlers/auth.rs`, `backend/src/middleware/auth.rs`

---

### 2. 错误处理不规范 ❌

**问题描述**：
- 大量使用 `.unwrap()` 和 `.expect()`
- 返回 `StatusCode` 而非结构化错误响应
- 缺少统一的错误类型
- 错误信息不够详细

**影响**：生产环境 panic、错误难以调试、用户体验差

**解决方案**：

```rust
// 定义统一的错误类型
#[derive(Debug)]
pub enum AppError {
    Database(String),
    Validation(String),
    Authentication(String),
    NotFound(String),
    Internal(String),
}

impl From<rusqlite::Error> for AppError {
    fn from(err: rusqlite::Error) -> Self {
        AppError::Database(err.to_string())
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Database(msg) => write!(f, "Database error: {}", msg),
            AppError::Validation(msg) => write!(f, "Validation error: {}", msg),
            AppError::Authentication(msg) => write!(f, "Authentication error: {}", msg),
            AppError::NotFound(msg) => write!(f, "Not found: {}", msg),
            AppError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

// 错误响应结构
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub timestamp: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            AppError::Database(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                msg
            ),
            AppError::Validation(msg) => (
                StatusCode::BAD_REQUEST,
                "VALIDATION_ERROR",
                msg
            ),
            AppError::Authentication(msg) => (
                StatusCode::UNAUTHORIZED,
                "AUTHENTICATION_ERROR",
                msg
            ),
            AppError::NotFound(msg) => (
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                msg
            ),
            AppError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                msg
            ),
        };

        let error_response = ErrorResponse {
            error: ErrorDetail {
                code: code.to_string(),
                message,
                details: None,
                timestamp: Utc::now().to_rfc3339(),
            },
        };

        (status, Json(error_response)).into_response()
    }
}

// 使用示例
pub async fn create_todo(
    State(state): State<AppState>,
    Json(req): Json<CreateTodoRequest>,
) -> Result<Json<Todo>, AppError> {
    // 验证输入
    req.validate()
        .map_err(|e| AppError::Validation(e.to_string()))?;

    // 创建 todo
    let id = state.db.create_todo(&req.title, &req.description)?;

    // 获取创建的 todo
    let todo = state.db.get_todo_by_id(id)
        .ok_or_else(|| AppError::NotFound(format!("Todo {} not found", id)))?;

    Ok(Json(todo))
}
```

**需要的依赖**：
```toml
[dependencies]
validator = "0.16"
```

**实施步骤**：
1. 定义统一的 AppError 枚举
2. 实现错误转换 trait（From）
3. 实现 IntoResponse trait
4. 替换所有 `.unwrap()` 为 `?` 或 `map_err`
5. 更新所有处理器返回类型

**优先级**：🔴 高（1周内完成）

**文件位置**：`backend/src/error.rs`, 所有处理器文件

---

### 3. API 设计不规范 🔄

**问题描述**：
- 使用非标准的 `/api` 前缀
- 缺少版本控制
- 错误响应不统一
- 没有分页机制
- 端点组织不够规范

**影响**：API 难以维护、版本升级困难、客户端开发困难

**解决方案**：

```rust
// 重构 API 端点
// 当前端点：
/api/todos                    # GET/POST 获取/创建 Todo
/api/todos/{id}               # GET/PUT/DELETE 获取/更新/删除
/api/todos/{id}/tags          # PUT 更新标签
/api/todos/{id}/force-status  # PUT 强制更新状态
/api/todos/{id}/summary       # GET 获取执行摘要
/api/todos/{id}/scheduler     # PUT 更新调度配置
/api/tags                     # GET/POST 获取/创建标签
/api/execute                  # POST 执行任务
/api/events                   # WebSocket 实时事件

// 规范化后的端点：
/api/v1/todos                        # GET/POST 列表/创建
/api/v1/todos/{id}                   # GET/PUT/PATCH/DELETE 操作单个 Todo
/api/v1/todos/{id}/tags              # GET/PUT 标签管理
/api/v1/todos/{id}/executions        # GET 执行记录列表
/api/v1/todos/{id}/executions/{execution_id}  # GET 单个执行记录
/api/v1/todos/{id}/summary           # GET 执行摘要
/api/v1/tags                         # GET/POST 标签列表/创建
/api/v1/tags/{id}                    # GET/PUT/PATCH/DELETE 标签操作
/api/v1/executions                   # POST 触发执行
/api/v1/scheduled-todos              # GET 获取调度中的任务
/api/v1/events                       # WebSocket 实时事件

// 特殊操作使用动作端点
/api/v1/todos/{id}/actions/execute   # POST 执行
/api/v1/todos/{id}/actions/recover   # POST 恢复

// 分页响应结构
#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub pagination: PaginationMeta,
}

#[derive(Debug, Serialize)]
pub struct PaginationMeta {
    pub page: u32,
    pub page_size: u32,
    pub total: u64,
    pub total_pages: u32,
}

// 查询参数
#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    #[serde(default = "default_page")]
    pub page: u32,

    #[serde(default = "default_page_size")]
    pub page_size: u32,

    #[serde(default)]
    pub sort_by: Option<String>,

    #[serde(default)]
    pub sort_order: Option<String>,
}

fn default_page() -> u32 { 1 }
fn default_page_size() -> u32 { 20 }

// 使用示例
pub async fn get_todos(
    Query(params): Query<PaginationQuery>,
    State(state): State<AppState>,
) -> Result<Json<PaginatedResponse<Todo>>, AppError> {
    let page = params.page.max(1);
    let page_size = params.page_size.min(100).max(1);
    let offset = (page - 1) * page_size;

    let todos = state.db.get_todos_paginated(page_size, offset)?;
    let total = state.db.get_todos_count()?;
    let total_pages = (total as f32 / page_size as f32).ceil() as u32;

    Ok(Json(PaginatedResponse {
        data: todos,
        pagination: PaginationMeta {
            page,
            page_size,
            total,
            total_pages,
        },
    }))
}
```

**需要的依赖**：
```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
```

**实施步骤**：
1. 定义新的端点结构
2. 实现分页响应结构
3. 更新路由配置
4. 实现分页查询
5. 更新前端 API 调用
6. 添加 API 版本管理

**优先级**：🟡 中（2-3周内完成）

**文件位置**：`backend/src/handlers/mod.rs`, `backend/src/routes.rs`, `backend/src/models/pagination.rs`

---

### 4. 数据库性能瓶颈 🐌

**问题描述**：
- 使用单个 `Mutex<Connection>`，所有操作串行化
- N+1 查询问题（`get_todos` 中先查 todos 再查 tags）
- 没有连接池
- 缺少事务支持
- SQL 注入风险

**影响**：性能差、并发能力低、数据一致性风险

**解决方案**：

```rust
// 使用连接池
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;

pub struct Database {
    pool: Pool<SqliteConnectionManager>,
}

impl Database {
    pub fn new(path: &str) -> Result<Self, DbError> {
        let manager = SqliteConnectionManager::file(path);
        let pool = Pool::builder()
            .max_size(10)
            .connection_timeout(Duration::from_secs(30))
            .build(manager)
            .map_err(|e| DbError::PoolError(e.to_string()))?;

        Ok(Self { pool })
    }

    pub fn get_conn(&self) -> Result<PooledConnection<SqliteConnectionManager>, DbError> {
        self.pool.get()
            .map_err(|e| DbError::PoolError(e.to_string()))
    }
}

// 解决 N+1 查询
pub fn get_todos(&self) -> Result<Vec<Todo>, DbError> {
    let conn = self.get_conn()?;
    let mut stmt = conn.prepare(r#"
        SELECT
            t.id, t.title, t.description, t.status,
            t.created_at, t.updated_at, t.executor,
            t.scheduler_enabled, t.scheduler_config, t.task_id,
            GROUP_CONCAT(tt.tag_id) as tag_ids
        FROM todos t
        LEFT JOIN todo_tags tt ON t.id = tt.todo_id
        WHERE t.deleted_at IS NULL
        GROUP BY t.id
        ORDER BY t.created_at DESC
    "#)?;

    let todos = stmt.query_map([], |row| {
        let tag_ids_str: Option<String> = row.get(10)?;
        let tag_ids: Vec<i64> = tag_ids_str
            .unwrap_or_default()
            .split(',')
            .filter_map(|s| s.parse().ok())
            .collect();

        Ok(Todo {
            id: row.get(0)?,
            title: row.get(1)?,
            description: row.get(2)?,
            status: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
            tag_ids,
            executor: row.get(6)?,
            scheduler_enabled: row.get(7)?,
            scheduler_config: row.get(8)?,
            task_id: row.get(9)?,
            deleted_at: None,
        })
    })?
    .collect::<Result<Vec<_>, _>>()?;

    Ok(todos)
}

// 添加事务支持
pub fn create_todo_with_tags(
    &self,
    title: &str,
    description: &str,
    tag_ids: &[i64],
) -> Result<i64, DbError> {
    let conn = self.get_conn()?;
    let tx = conn.unchecked_transaction()?;

    // 创建 Todo
    tx.execute(
        "INSERT INTO todos (title, description) VALUES (?1, ?2)",
        params![title, description],
    )?;

    let todo_id = tx.last_insert_rowid();

    // 添加标签关联
    for &tag_id in tag_ids {
        tx.execute(
            "INSERT INTO todo_tags (todo_id, tag_id) VALUES (?1, ?2)",
            params![todo_id, tag_id],
        )?;
    }

    tx.commit()?;
    Ok(todo_id)
}

// 分页查询
pub fn get_todos_paginated(
    &self,
    page_size: u32,
    offset: u32,
) -> Result<Vec<Todo>, DbError> {
    let conn = self.get_conn()?;
    let mut stmt = conn.prepare(r#"
        SELECT
            t.id, t.title, t.description, t.status,
            t.created_at, t.updated_at, t.executor,
            GROUP_CONCAT(tt.tag_id) as tag_ids
        FROM todos t
        LEFT JOIN todo_tags tt ON t.id = tt.todo_id
        WHERE t.deleted_at IS NULL
        GROUP BY t.id
        ORDER BY t.created_at DESC
        LIMIT ?1 OFFSET ?2
    "#)?;

    let todos = stmt.query_map(params![page_size, offset], |row| {
        // ... 解析逻辑
    })?
    .collect::<Result<Vec<_>, _>>()?;

    Ok(todos)
}

pub fn get_todos_count(&self) -> Result<u64, DbError> {
    let conn = self.get_conn()?;
    let count: u64 = conn.query_row(
        "SELECT COUNT(*) FROM todos WHERE deleted_at IS NULL",
        [],
        |row| row.get(0),
    )?;

    Ok(count)
}
```

**需要的依赖**：
```toml
[dependencies]
r2d2 = "0.8"
r2d2_sqlite = "0.23"
```

**实施步骤**：
1. 添加连接池依赖
2. 重构 Database 结构
3. 使用连接池替代单个连接
4. 解决 N+1 查询问题
5. 添加事务支持
6. 实现分页查询

**优先级**：🟡 中（2-3周内完成）

**文件位置**：`backend/src/db/mod.rs`

---

### 5. 配置管理缺失 ⚙️

**问题描述**：
- 硬编码配置（端口、数据库路径）
- 缺少环境变量支持
- 没有配置文件
- 无法灵活切换环境

**影响**：部署困难、配置不灵活、安全性差

**解决方案**：

```rust
// 使用 config 库
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub logging: LoggingConfig,
    #[serde(default)]
    pub executors: ExecutorConfig,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub pool_size: u32,
}

#[derive(Debug, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct ExecutorConfig {
    pub claudecode_path: Option<String>,
    pub mobilecoder_path: Option<String>,
    pub opencode_path: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        // 优先从环境变量加载
        if let Ok(config) = Self::from_env() {
            return Ok(config);
        }

        // 从配置文件加载
        Self::from_file("config.toml")
            .or_else(|_| Self::from_file("config/development.toml"))
            .or_else(|_| Self::from_file("config/production.toml"))
            .or_else(|_| Self::default())
            .map_err(|e| e.into())
    }

    pub fn from_env() -> Result<Self, envy::Error> {
        envy::prefixed("NTD_")
            .from_env()
    }

    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3000,
            },
            database: DatabaseConfig {
                url: "ntd.db".to_string(),
                pool_size: 10,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "json".to_string(),
            },
            executors: ExecutorConfig::default(),
        }
    }
}

// 在 main.rs 中使用
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 加载配置
    let config = Config::load()?;

    // 初始化日志
    init_logging(&config.logging);

    // 初始化数据库
    let db = Database::new(&config.database.url)?;

    // 启动服务器
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!("Server listening on {}", addr);
    // ...
}

fn init_logging(config: &LoggingConfig) {
    let level = match config.level.as_str() {
        "trace" => tracing::Level::TRACE,
        "debug" => tracing::Level::DEBUG,
        "info" => tracing::Level::INFO,
        "warn" => tracing::Level::WARN,
        "error" => tracing::Level::ERROR,
        _ => tracing::Level::INFO,
    };

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(level)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");
}
```

**需要的依赖**：
```toml
[dependencies]
config = "0.13"
toml = "0.8"
serde = { version = "1", features = ["derive"] }
envy = "0.4"
```

**配置文件示例（config.toml）**：
```toml
[server]
host = "127.0.0.1"
port = 3000

[database]
url = "ntd.db"
pool_size = 10

[logging]
level = "info"
format = "json"

[executors]
claudecode_path = "/path/to/claudecode"
mobilecoder_path = "/path/to/mobilecoder"
opencode_path = "/path/to/opencode"
```

**环境变量示例**：
```bash
export NTD_SERVER_HOST=0.0.0.0
export NTD_SERVER_PORT=8080
export NTD_DATABASE_URL=/var/lib/ntd/ntd.db
export NTD_DATABASE_POOL_SIZE=20
export NTD_LOGGING_LEVEL=debug
```

**实施步骤**：
1. 添加配置相关依赖
2. 定义配置结构体
3. 实现配置加载逻辑
4. 创建配置文件示例
5. 更新 main.rs 使用配置
6. 添加环境变量支持

**优先级**：🟡 中（2周内完成）

**文件位置**：`backend/src/config.rs`, `config.toml.example`, `config/development.toml`, `config/production.toml`

---

## 🔧 中期优化（1-2个月）

### 6. 架构重构：清洁架构 🏗️

**当前问题**：
- 缺少服务层抽象
- 业务逻辑散落在 Handler 和 Service 之间
- 数据库模型直接暴露给 HTTP 层
- 紧耦合的依赖关系

**推荐架构模式**：清洁架构 + Hexagonal 架构

```
domain/          # 领域层（核心业务逻辑）
├── entities/
│   ├── todo.rs           # Todo 实体
│   ├── execution.rs       # 执行记录实体
│   └── scheduler.rs       # 调度配置实体
├── value_objects/
│   ├── todo_status.rs    # 状态值对象
│   └── executor_type.rs  # 执行器类型
├── repositories/
│   ├── todo_repository.rs     # Todo 仓储接口
│   └── execution_repository.rs # 执行记录仓储接口
└── services/
    ├── todo_service.rs   # Todo 业务服务
    └── execution_service.rs # 执行服务

application/      # 应用层（用例）
├── commands/
│   ├── create_todo.rs
│   ├── execute_todo.rs
│   └── update_scheduler.rs
├── queries/
│   ├── get_todos.rs
│   └── get_execution_summary.rs
└── dto/                 # 数据传输对象
    ├── todo_dto.rs
    └── execution_dto.rs

infrastructure/   # 基础设施层
├── persistence/
│   ├── sqlite/
│   │   ├── todo_repository_impl.rs
│   │   └── execution_repository_impl.rs
│   └── database.rs
├── adapters/
│   ├── claude_code_adapter.rs
│   ├── mobilecoder_adapter.rs
│   └── opencode_adapter.rs
└── config/
    └── settings.rs

interfaces/       # 接口层
├── http/
│   ├── handlers/
│   ├── middleware/
│   └── routes.rs
└── websocket/
    └── event_handler.rs
```

**核心原则**：
1. **依赖倒置**：高层模块不依赖低层模块，都依赖抽象
2. **单一职责**：每个模块只负责一个职责
3. **开闭原则**：对扩展开放，对修改关闭
4. **接口隔离**：客户端不应依赖不需要的接口

**实施示例**：

```rust
// domain/repositories/todo_repository.rs
#[async_trait::async_trait]
pub trait TodoRepository: Send + Sync {
    async fn create(&self, todo: &Todo) -> Result<i64, RepositoryError>;
    async fn find_by_id(&self, id: i64) -> Result<Option<Todo>, RepositoryError>;
    async fn find_all(&self) -> Result<Vec<Todo>, RepositoryError>;
    async fn update(&self, todo: &Todo) -> Result<(), RepositoryError>;
    async fn delete(&self, id: i64) -> Result<(), RepositoryError>;
}

// domain/entities/todo.rs
#[derive(Debug, Clone)]
pub struct Todo {
    pub id: Option<i64>,
    pub title: String,
    pub description: String,
    pub status: TodoStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tag_ids: Vec<i64>,
    pub executor: String,
    pub scheduler_enabled: bool,
    pub scheduler_config: Option<String>,
    pub task_id: Option<String>,
}

impl Todo {
    pub fn new(title: String, description: String) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            title,
            description,
            status: TodoStatus::Pending,
            created_at: now,
            updated_at: now,
            tag_ids: vec![],
            executor: String::new(),
            scheduler_enabled: false,
            scheduler_config: None,
            task_id: None,
        }
    }

    pub fn can_execute(&self) -> bool {
        self.status == TodoStatus::Pending || self.status == TodoStatus::Failed
    }
}

// domain/services/todo_service.rs
pub struct TodoService<T: TodoRepository> {
    repository: T,
}

impl<T: TodoRepository> TodoService<T> {
    pub fn new(repository: T) -> Self {
        Self { repository }
    }

    pub async fn create_todo(
        &self,
        title: String,
        description: String,
        tag_ids: Vec<i64>,
    ) -> Result<Todo, ServiceError> {
        let mut todo = Todo::new(title, description);
        todo.tag_ids = tag_ids;

        let id = self.repository.create(&todo).await?;
        todo.id = Some(id);

        Ok(todo)
    }

    pub async fn execute_todo(&self, id: i64) -> Result<Execution, ServiceError> {
        let mut todo = self.repository.find_by_id(id).await?
            .ok_or_else(|| ServiceError::NotFound(format!("Todo {} not found", id)))?;

        if !todo.can_execute() {
            return Err(ServiceError::InvalidState(
                "Todo cannot be executed in current state".to_string()
            ));
        }

        // 执行逻辑...
        Ok(Execution::new(id))
    }
}

// infrastructure/persistence/sqlite/todo_repository_impl.rs
pub struct SqliteTodoRepository {
    pool: Arc<Pool<SqliteConnectionManager>>,
}

#[async_trait::async_trait]
impl TodoRepository for SqliteTodoRepository {
    async fn create(&self, todo: &Todo) -> Result<i64, RepositoryError> {
        let pool = self.pool.clone();
        let todo = todo.clone();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            // 数据库操作
            Ok(conn.last_insert_rowid())
        })
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?
    }

    async fn find_by_id(&self, id: i64) -> Result<Option<Todo>, RepositoryError> {
        let pool = self.pool.clone();

        let result = tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            // 数据库查询
            Ok(Some(Todo::new("test".to_string(), "test".to_string())))
        })
        .await;

        match result {
            Ok(res) => res.map_err(|e| RepositoryError::Database(e.to_string())),
            Err(e) => Err(RepositoryError::Database(e.to_string())),
        }
    }

    // ... 其他方法实现
}

// interfaces/http/handlers/todo_handler.rs
pub struct TodoHandler {
    service: Arc<TodoService<SqliteTodoRepository>>,
}

impl TodoHandler {
    pub fn new(service: Arc<TodoService<SqliteTodoRepository>>) -> Self {
        Self { service }
    }

    pub async fn create_todo(
        State(state): State<AppState>,
        Json(req): Json<CreateTodoRequest>,
    ) -> Result<Json<TodoDto>, AppError> {
        let todo = state.todo_service.create_todo(
            req.title,
            req.description,
            req.tag_ids,
        ).await?;

        Ok(Json(TodoDto::from(todo)))
    }
}
```

**需要的依赖**：
```toml
[dependencies]
async-trait = "0.1"
tokio = { version = "1", features = ["full"] }
```

**实施步骤**：
1. 设计领域层（entities, repositories, services）
2. 实现应用层（commands, queries, dto）
3. 重构基础设施层（persistence, adapters）
4. 更新接口层（handlers, routes）
5. 编写单元测试
6. 逐步迁移现有功能

**优先级**：🟢 低（1-2个月）

**文件位置**：创建新的目录结构

---

### 7. 添加监控和日志 📊

**当前状态**：基本的 `env_logger`

**改进方案**：

```rust
// 结构化日志
use tracing::{info, error, warn, instrument};

#[instrument(skip(db), fields(title = %req.title))]
pub async fn create_todo(
    State(state): State<AppState>,
    Json(req): Json<CreateTodoRequest>,
) -> Result<Json<Todo>, AppError> {
    info!("Creating new todo");

    let id = state.db.create_todo(&req.title, &req.description)?;

    info!(todo_id = id, "Todo created successfully");

    Ok(Json(Todo { /* ... */ }))
}

#[instrument(skip(state))]
pub async fn execute_todo(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Execution>, AppError> {
    info!(todo_id = id, "Starting todo execution");

    match state.executor_service.execute(id).await {
        Ok(execution) => {
            info!(todo_id = id, execution_id = execution.id, "Todo executed successfully");
            Ok(Json(execution))
        }
        Err(e) => {
            error!(todo_id = id, error = %e, "Failed to execute todo");
            Err(AppError::Internal(e.to_string()))
        }
    }
}

// 添加 Prometheus 指标
use prometheus::{Counter, Histogram, IntGauge};

lazy_static! {
    static ref HTTP_REQUESTS_TOTAL: Counter = register_counter!(
        "http_requests_total",
        "Total number of HTTP requests"
    ).unwrap();

    static ref HTTP_REQUEST_DURATION: Histogram = register_histogram!(
        "http_request_duration_seconds",
        "HTTP request latencies in seconds"
    ).unwrap();

    static ref ACTIVE_EXECUTIONS: IntGauge = register_int_gauge!(
        "active_executions",
        "Number of active task executions"
    ).unwrap();

    static ref TODO_COUNT: IntGauge = register_int_gauge!(
        "todo_count",
        "Total number of todos"
    ).unwrap();
}

// 指标中间件
pub async fn metrics_middleware(
    req: Request,
    next: Next,
) -> Response {
    let start = Instant::now();
    let method = req.method().to_string();
    let path = req.uri().path().to_string();

    let response = next.run(req).await;

    let duration = start.elapsed();
    HTTP_REQUESTS_TOTAL.inc();
    HTTP_REQUEST_DURATION.observe(duration.as_secs_f64());

    info!(
        method = %method,
        path = %path,
        status = %response.status(),
        duration_ms = duration.as_millis(),
        "Request completed"
    );

    response
}

// 指标端点
pub async fn metrics_handler() -> Result<String, AppError> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer)?;

    Ok(String::from_utf8(buffer)?)
}

// 指标集成
pub async fn create_todo(
    State(state): State<AppState>,
    Json(req): Json<CreateTodoRequest>,
) -> Result<Json<Todo>, AppError> {
    TODO_COUNT.inc();

    let id = state.db.create_todo(&req.title, &req.description)?;

    Ok(Json(Todo { /* ... */ }))
}

pub async fn execute_todo(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Execution>, AppError> {
    ACTIVE_EXECUTIONS.inc();

    let execution = state.executor_service.execute(id).await?;

    ACTIVE_EXECUTIONS.dec();

    Ok(Json(execution))
}
```

**需要的依赖**：
```toml
[dependencies]
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
prometheus = "0.13"
lazy_static = "1.4"
```

**配置日志**：
```rust
fn init_logging(config: &LoggingConfig) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.level));

    let subscriber = if config.format == "json" {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(env_filter)
            .finish()
    } else {
        tracing_subscriber::fmt()
            .pretty()
            .with_env_filter(env_filter)
            .finish()
    };

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");
}
```

**Prometheus 配置示例**：
```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'ntd'
    static_configs:
      - targets: ['localhost:3000']
    scrape_interval: 15s
```

**Grafana 面板示例**：
```json
{
  "title": "ntd Dashboard",
  "panels": [
    {
      "title": "Request Rate",
      "targets": [
        {
          "expr": "rate(http_requests_total[1m])"
        }
      ]
    },
    {
      "title": "Request Latency",
      "targets": [
        {
          "expr": "histogram_quantile(0.95, rate(http_request_duration_seconds_bucket[5m]))"
        }
      ]
    },
    {
      "title": "Active Executions",
      "targets": [
        {
          "expr": "active_executions"
        }
      ]
    },
    {
      "title": "Todo Count",
      "targets": [
        {
          "expr": "todo_count"
        }
      ]
    }
  ]
}
```

**实施步骤**：
1. 添加日志和监控依赖
2. 初始化 tracing
3. 为关键操作添加日志
4. 实现指标收集
5. 添加指标端点
6. 部署 Prometheus 和 Grafana
7. 创建监控面板

**优先级**：🟢 低（1个月）

**文件位置**：`backend/src/logging.rs`, `backend/src/metrics.rs`, `backend/src/middleware/metrics.rs`

---

### 8. 添加单元测试 ✅

**当前状态**：测试覆盖率未知

**改进方案**：

```rust
// domain/services/todo_service.rs 测试
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::repositories::MockTodoRepository;
    use mockall::predicate::*;

    #[tokio::test]
    async fn test_create_todo_success() {
        let mut mock_repo = MockTodoRepository::new();
        mock_repo
            .expect_create()
            .with(always())
            .returning(|_| Ok(1));

        let service = TodoService::new(mock_repo);

        let todo = service.create_todo(
            "Test Title".to_string(),
            "Test Description".to_string(),
            vec![],
        ).await;

        assert!(todo.is_ok());
        let todo = todo.unwrap();
        assert_eq!(todo.id, Some(1));
        assert_eq!(todo.title, "Test Title");
    }

    #[tokio::test]
    async fn test_create_todo_validation_error() {
        let mut mock_repo = MockTodoRepository::new();
        mock_repo
            .expect_create()
            .with(always())
            .returning(|_| Err(RepositoryError::Database("Error".to_string())));

        let service = TodoService::new(mock_repo);

        let result = service.create_todo(
            "".to_string(),  // 空标题
            "Test Description".to_string(),
            vec![],
        ).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_todo_success() {
        let mut mock_repo = MockTodoRepository::new();

        let test_todo = Todo::new("Test".to_string(), "Test".to_string());
        test_todo.id = Some(1);

        mock_repo
            .expect_find_by_id()
            .with(eq(1))
            .times(1)
            .returning(|_| Ok(Some(test_todo)));

        let service = TodoService::new(mock_repo);

        let result = service.execute_todo(1).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_todo_not_found() {
        let mut mock_repo = MockTodoRepository::new();
        mock_repo
            .expect_find_by_id()
            .with(eq(999))
            .times(1)
            .returning(|_| Ok(None));

        let service = TodoService::new(mock_repo);

        let result = service.execute_todo(999).await;

        assert!(matches!(result, Err(ServiceError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_execute_todo_invalid_state() {
        let mut mock_repo = MockTodoRepository::new();

        let mut test_todo = Todo::new("Test".to_string(), "Test".to_string());
        test_todo.id = Some(1);
        test_todo.status = TodoStatus::Completed;  // 已完成状态

        mock_repo
            .expect_find_by_id()
            .with(eq(1))
            .times(1)
            .returning(|_| Ok(Some(test_todo)));

        let service = TodoService::new(mock_repo);

        let result = service.execute_todo(1).await;

        assert!(matches!(result, Err(ServiceError::InvalidState(_))));
    }
}

// infrastructure/persistence/sqlite/todo_repository_impl.rs 测试
#[cfg(test)]
mod tests {
    use super::*;

    fn setup_in_memory_db() -> Pool<SqliteConnectionManager> {
        let manager = SqliteConnectionManager::file(":memory:");
        Pool::builder()
            .max_size(1)
            .build(manager)
            .unwrap()
    }

    #[tokio::test]
    async fn test_create_and_find_todo() {
        let pool = setup_in_memory_db();

        // 初始化数据库
        let conn = pool.get().unwrap();
        conn.execute_batch(r#"
            CREATE TABLE todos (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                description TEXT,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                executor TEXT,
                scheduler_enabled INTEGER NOT NULL DEFAULT 0,
                scheduler_config TEXT,
                task_id TEXT,
                deleted_at TEXT
            );
        "#).unwrap();

        let repo = SqliteTodoRepository::new(Arc::new(pool));

        // 创建 todo
        let todo = Todo::new("Test Todo".to_string(), "Test Description".to_string());
        let id = repo.create(&todo).await.unwrap();

        // 查找 todo
        let found = repo.find_by_id(id).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.title, "Test Todo");
    }

    #[tokio::test]
    async fn test_find_nonexistent_todo() {
        let pool = setup_in_memory_db();
        let repo = SqliteTodoRepository::new(Arc::new(pool));

        let result = repo.find_by_id(999).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}

// 集成测试
#[tokio::test]
async fn test_create_todo_integration() {
    // 启动测试服务器
    let app = create_test_app().await;

    // 创建 todo
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/todos")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"Test","description":"Test"}"#))
                .unwrap()
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let todo: Todo = serde_json::from_slice(&body).unwrap();
    assert_eq!(todo.title, "Test");
}
```

**需要的依赖**：
```toml
[dev-dependencies]
tokio-test = "0.4"
mockall = "0.11"
wiremock = "0.5"
criterion = "0.4"
```

**测试覆盖率配置**：
```toml
# .cargo/config.toml
[workspace.metadata.dylint]
libraries = []

[profile.dev]
debug = true

[profile.test]
opt-level = 0
debug = true
```

**运行测试**：
```bash
# 运行所有测试
cargo test

# 运行特定测试
cargo test test_create_todo

# 运行测试并显示输出
cargo test -- --nocapture

# 运行集成测试
cargo test --test '*'

# 生成覆盖率报告
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
```

**CI/CD 集成**：
```yaml
# .github/workflows/test.yml
name: Test

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - uses: actions-rs/toolchain@v1
        with:
          profile: minimal
          toolchain: stable
      - name: Run tests
        run: cargo test --verbose
      - name: Generate coverage
        run: cargo tarpaulin --out Xml
      - name: Upload coverage
        uses: codecov/codecov-action@v2
```

**实施步骤**：
1. 添加测试依赖
2. 为领域层编写单元测试
3. 为基础设施层编写集成测试
4. 为接口层编写端到端测试
5. 设置测试覆盖率目标（建议 80%+）
6. 配置 CI/CD 自动运行测试

**优先级**：🟢 低（持续进行）

**文件位置**：各个模块的 `tests` 子模块

---

## 📈 长期规划（3-6个月）

### 9. 数据库迁移 🗄️

**推荐**：从 SQLite 迁移到 PostgreSQL

**优势**：
- 更好的并发性能
- 支持事务
- 丰富的数据类型
- 更好的查询优化
- 复制和备份支持
- 成熟的工具链

**迁移方案**：

```sql
-- PostgreSQL Schema
CREATE TYPE todo_status AS ENUM ('pending', 'running', 'completed', 'failed');

CREATE TABLE todos (
    id SERIAL PRIMARY KEY,
    title VARCHAR(200) NOT NULL,
    description TEXT,
    status todo_status NOT NULL DEFAULT 'pending',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    executor VARCHAR(50),
    scheduler_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    scheduler_config JSONB,
    task_id VARCHAR(100),
    deleted_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX idx_todos_status ON todos(status);
CREATE INDEX idx_todos_created_at ON todos(created_at DESC);
CREATE INDEX idx_todos_deleted_at ON todos(deleted_at) WHERE deleted_at IS NULL;

CREATE TABLE tags (
    id SERIAL PRIMARY KEY,
    name VARCHAR(50) NOT NULL UNIQUE,
    color VARCHAR(7),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE TABLE todo_tags (
    todo_id INTEGER REFERENCES todos(id) ON DELETE CASCADE,
    tag_id INTEGER REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (todo_id, tag_id)
);

CREATE INDEX idx_todo_tags_todo_id ON todo_tags(todo_id);
CREATE INDEX idx_todo_tags_tag_id ON todo_tags(tag_id);

CREATE TABLE executions (
    id SERIAL PRIMARY KEY,
    todo_id INTEGER REFERENCES todos(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'running',
    output TEXT,
    error TEXT,
    logs JSONB,
    started_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX idx_executions_todo_id ON executions(todo_id);
CREATE INDEX idx_executions_status ON executions(status);

-- 使用 JSONB 存储配置和日志
-- 支持高效的 JSON 查询
CREATE INDEX idx_todos_scheduler_config ON todos USING GIN (scheduler_config);
CREATE INDEX idx_executions_logs ON executions USING GIN (logs);
```

**迁移代码**：

```rust
// migration.rs
use sqlx::{Pool, Postgres, migrate::MigrateDatabase};

pub async fn migrate_database(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !Postgres::database_exists(url).await? {
        Postgres::create_database(url).await?;
    }

    let pool = Pool::<Postgres>::connect(url).await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(())
}

// 使用 sqlx 进行查询
use sqlx::FromRow;

#[derive(Debug, FromRow)]
pub struct Todo {
    pub id: i32,
    pub title: String,
    pub description: Option<String>,
    pub status: TodoStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub tag_ids: Vec<i32>,
}

pub async fn get_todos(pool: &Pool<Postgres>) -> Result<Vec<Todo>, sqlx::Error> {
    sqlx::query_as!(
        Todo,
        r#"
        SELECT
            t.id, t.title, t.description, t.status as "status: TodoStatus",
            t.created_at, t.updated_at,
            ARRAY_AGG(tt.tag_id) FILTER (WHERE tt.tag_id IS NOT NULL) as tag_ids
        FROM todos t
        LEFT JOIN todo_tags tt ON t.id = tt.todo_id
        WHERE t.deleted_at IS NULL
        GROUP BY t.id
        ORDER BY t.created_at DESC
        "#
    )
    .fetch_all(pool)
    .await
}
```

**需要的依赖**：
```toml
[dependencies]
sqlx = { version = "0.7", features = ["postgres", "runtime-tokio-rustls", "chrono", "json"] }
```

**实施步骤**：
1. 设计 PostgreSQL schema
2. 编写迁移脚本
3. 更新数据访问层
4. 测试数据迁移
5. 性能测试
6. 切换到 PostgreSQL
7. 监控和优化

**优先级**：⚪ 非常低（3-6个月）

**文件位置**：`backend/migrations/`, `backend/src/db/postgres.rs`

---

### 10. 添加缓存层 💾

**推荐**：使用 Redis 缓存热点数据

**应用场景**：
- 缓存 Todo 列表
- 缓存标签列表
- 缓存执行摘要
- 缓存用户会话

**实现方案**：

```rust
// cache/redis_cache.rs
use redis::{AsyncCommands, Client};

#[derive(Clone)]
pub struct RedisCache {
    client: Client,
}

impl RedisCache {
    pub async fn new(url: &str) -> Result<Self, CacheError> {
        let client = Client::open(url)?;
        Ok(Self { client })
    }

    pub async fn get<T: for<'de> Deserialize<'de>>(
        &self,
        key: &str,
    ) -> Result<Option<T>, CacheError> {
        let mut conn = self.client.get_async_connection().await?;

        let value: Option<String> = conn.get(key).await?;

        match value {
            Some(v) => {
                let data: T = serde_json::from_str(&v)?;
                Ok(Some(data))
            }
            None => Ok(None),
        }
    }

    pub async fn set<T: Serialize>(
        &self,
        key: &str,
        value: &T,
        ttl: u64,
    ) -> Result<(), CacheError> {
        let mut conn = self.client.get_async_connection().await?;

        let value = serde_json::to_string(value)?;
        conn.set_ex(key, value, ttl).await?;

        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<(), CacheError> {
        let mut conn = self.client.get_async_connection().await?;
        conn.del(key).await?;
        Ok(())
    }

    pub async fn invalidate_pattern(&self, pattern: &str) -> Result<(), CacheError> {
        let mut conn = self.client.get_async_connection().await?;

        let keys: Vec<String> = conn.keys(pattern).await?;
        if !keys.is_empty() {
            conn.del(keys).await?;
        }

        Ok(())
    }
}

// 在服务中使用缓存
pub struct CachedTodoService {
    inner: TodoService<SqliteTodoRepository>,
    cache: RedisCache,
}

impl CachedTodoService {
    pub async fn get_todos(&self) -> Result<Vec<Todo>, ServiceError> {
        let cache_key = "todos:all";

        // 尝试从缓存获取
        if let Some(cached) = self.cache.get::<Vec<Todo>>(cache_key).await? {
            return Ok(cached);
        }

        // 从数据库获取
        let todos = self.inner.get_todos().await?;

        // 写入缓存（5分钟 TTL）
        self.cache.set(cache_key, &todos, 300).await?;

        Ok(todos)
    }

    pub async fn create_todo(&self, req: CreateTodoRequest) -> Result<Todo, ServiceError> {
        let todo = self.inner.create_todo(req).await?;

        // 使缓存失效
        self.cache.delete("todos:all").await?;

        Ok(todo)
    }
}
```

**需要的依赖**：
```toml
[dependencies]
redis = { version = "0.23", features = ["tokio-comp", "connection-manager"] }
```

**缓存配置**：
```toml
# config.toml
[cache]
enabled = true
url = "redis://localhost:6379"
default_ttl = 300
```

**实施步骤**：
1. 设计缓存策略
2. 实现 Redis 缓存层
3. 集成到服务层
4. 添加缓存失效逻辑
5. 性能测试
6. 监控缓存命中率

**优先级**：⚪ 非常低（3-6个月）

**文件位置**：`backend/src/cache/redis_cache.rs`

---

### 11. 实现消息队列 📨

**推荐**：使用 RabbitMQ 或 Kafka

**应用场景**：
- 异步任务执行
- 事件驱动架构
- 解耦服务
- 任务重试和死信队列

**RabbitMQ 实现方案**：

```rust
// messaging/rabbitmq_client.rs
use lapin::{Connection, ConnectionProperties, Channel};
use lapin::options::*;
use lapin::types::FieldTable;

#[derive(Clone)]
pub struct RabbitMQClient {
    connection: Arc<Connection>,
    channel: Arc<Channel>,
}

impl RabbitMQClient {
    pub async fn new(url: &str) -> Result<Self, MessagingError> {
        let connection = Connection::connect(url, ConnectionProperties::default()).await?;
        let channel = connection.create_channel().await?;

        // 声明队列
        channel
            .queue_declare(
                "todo_execution",
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await?;

        channel
            .queue_declare(
                "todo_execution_retry",
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await?;

        channel
            .queue_declare(
                "todo_execution_dead_letter",
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await?;

        Ok(Self {
            connection: Arc::new(connection),
            channel: Arc::new(channel),
        })
    }

    pub async fn publish_execution(
        &self,
        todo_id: i64,
    ) -> Result<(), MessagingError> {
        let payload = serde_json::to_vec(&ExecutionMessage { todo_id })?;

        self.channel
            .basic_publish(
                "",
                "todo_execution",
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default(),
            )
            .await?;

        Ok(())
    }

    pub async fn consume_executions(
        &self,
    ) -> Result<impl Stream<Item = Result<ExecutionMessage, MessagingError>>, MessagingError> {
        let consumer = self.channel
            .basic_consume(
                "todo_execution",
                "consumer",
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;

        Ok(consumer.filter_map(|delivery| async {
            match delivery {
                Ok(delivery) => {
                    let data = String::from_utf8_lossy(&delivery.data);
                    serde_json::from_str(&data).ok()
                }
                Err(e) => None,
            }
        }))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionMessage {
    pub todo_id: i64,
}

// 任务处理器
pub struct TaskProcessor {
    mq_client: RabbitMQClient,
    todo_service: Arc<TodoService<SqliteTodoRepository>>,
}

impl TaskProcessor {
    pub async fn start(&self) -> Result<(), MessagingError> {
        let mut stream = self.mq_client.consume_executions().await?;

        while let Some(message) = stream.next().await {
            if let Ok(msg) = message {
                self.process_execution(msg).await;
            }
        }

        Ok(())
    }

    async fn process_execution(&self, msg: ExecutionMessage) {
        info!("Processing execution for todo {}", msg.todo_id);

        match self.todo_service.execute_todo(msg.todo_id).await {
            Ok(_) => info!("Execution completed for todo {}", msg.todo_id),
            Err(e) => {
                error!("Execution failed for todo {}: {}", msg.todo_id, e);
                // 发送到重试队列
                self.mq_client.publish_to_retry_queue(msg.todo_id).await;
            }
        }
    }
}
```

**需要的依赖**：
```toml
[dependencies]
lapin = "2.1"
futures = "0.3"
```

**消息队列配置**：
```toml
# config.toml
[messaging]
enabled = true
url = "amqp://localhost:5672"
```

**实施步骤**：
1. 设计消息结构
2. 实现 RabbitMQ 客户端
3. 创建任务处理器
4. 更新服务发布消息
5. 实现重试机制
6. 添加监控和告警

**优先级**：⚪ 非常低（6个月+）

**文件位置**：`backend/src/messaging/rabbitmq_client.rs`, `backend/src/processor.rs`

---

## 🎯 实施路线图（修订）

> 原文档将所有阶段都标为 `[x]`（已完成），这与代码现状不符。以下是依据 2026-06-08 代码实际情况的修订版。
> 标 `[x]` 表示**已落地**；标 `[ ]` 表示**未实施 / 部分实施**。

### 第 1 周：安全性强化
- [ ] 实现 JWT 认证中间件（项目仍无认证机制，参见 §1）
- [ ] 添加 CORS 配置（当前 `CorsLayer::new().allow_origin(Any)`，未收紧）
- [ ] 实现速率限制（未引入 `tower-governor`）
- [ ] 添加输入验证（仅散落在 handler 中，无统一 `validator`）

### 第 2 周：错误处理改进
- [x] 定义统一的错误类型（`ApiJson` 提取器、JSON 错误响应）
- [ ] 替换所有 `.unwrap()` 和 `.expect()`（代码中仍大量存在）
- [x] 实现结构化错误响应（`{"error": true, "message": "..."}` 格式已统一）

### 第 3-4 周：API 和数据库优化
- [ ] 重构 API 端点（版本控制）（仍使用 `/api/...` 而非 `/api/v1/...`）
- [x] 添加分页功能（`page` / `limit` 参数已实现）
- [ ] 实现数据库连接池（SeaORM 自带 SQLite 池，未调优）
- [x] 解决 N+1 查询问题（部分列表接口已用 JOIN 聚合）

### 第 5-6 周：配置和日志
- [x] 实现配置管理系统（`backend/src/config.rs` 已成熟）
- [x] 添加结构化日志（`tracing` + JSON 输出）
- [ ] 添加基础监控指标（未引入 `prometheus`）

### 第 7-8 周：架构重构
- [ ] 设计清洁架构（仍为分层架构，未拆分 domain/application）
- [ ] 实现领域层
- [ ] 实现应用层
- [ ] 重构基础设施层

### 第 9-12 周：测试和优化
- [x] 添加单元测试（adapter / model / db / config 内联测试）
- [x] 添加集成测试（17 个 `backend/tests/*.rs` 文件）
- [ ] 性能优化（WAL 模式未显式设置）
- [ ] 文档完善（部分文档与代码脱节，正在审计修复）

---

## 📝 额外建议

### 工具使用

```bash
# 运行 Clippy 发现更多问题
cargo clippy -- -W clippy::unwrap_used -W clippy::expect_used

# 检查安全漏洞
cargo install cargo-audit
cargo audit

# 检查测试覆盖率
cargo install cargo-tarpaulin
cargo tarpaulin --out Html

# 代码格式化
cargo fmt

# 检查未使用的依赖
cargo +nightly udeps

# 生成依赖图
cargo tree

# 性能分析
cargo flamegraph

# 生成文档
cargo doc --open
```

### 依赖建议

```toml
[dependencies]
# 配置管理
config = "0.13"
toml = "0.8"
envy = "0.4"

# 日志和监控
tracing = "0.1"
tracing-subscriber = "0.3"
prometheus = "0.13"

# 验证
validator = "0.16"

# 认证
jsonwebtoken = "9"

# 速率限制
tower-governor = "0.4"

# 数据库
r2d2 = "0.8"
r2d2_sqlite = "0.23"
sqlx = { version = "0.7", features = ["postgres", "runtime-tokio-rustls"] }

# 缓存
redis = "0.23"

# 消息队列
lapin = "2.1"

# 异步
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"

[dev-dependencies]
# 测试
tokio-test = "0.4"
mockall = "0.11"
wiremock = "0.5"
criterion = "0.4"
```

### 最佳实践

1. **代码审查**：每次代码合并前进行审查
2. **CI/CD**：自动化测试和部署
3. **文档**：保持文档与代码同步
4. **性能测试**：定期进行性能测试
5. **安全审计**：定期进行安全审计
6. **监控**：实时监控应用状态
7. **备份**：定期备份数据
8. **灾难恢复**：准备灾难恢复计划

---

## ✅ 总结

### 当前项目状态

**优点**：
- ✅ 基础功能完善
- ✅ 代码结构清晰
- ✅ 使用现代化技术栈
- ✅ 异步设计良好
- ✅ 适配器模式便于扩展

**缺点**：
- ❌ 安全性严重不足
- ❌ 错误处理不规范
- ⚠️ 性能有待优化
- ⚠️ 配置管理缺失
- ⚠️ 测试覆盖率低

### 改进收益预期

> ⚠️ **以下数字为"预期目标"**，不是已实现的实测结果。实际收益取决于每项优化的落地效果与硬件/数据规模。

| 维度 | 当前 | 改进后（预期） | 提升 |
|------|------|--------|------|
| 安全性 | ⭐ | ⭐⭐⭐⭐⭐ | +400% |
| 可维护性 | ⭐⭐⭐ | ⭐⭐⭐⭐ | +33% |
| 性能 | ⭐⭐⭐ | ⭐⭐⭐⭐ | +33% |
| 可测试性 | ⭐⭐ | ⭐⭐⭐⭐⭐ | +150% |

### 建议优先级

1. 🔴 **立即实施**：安全性、错误处理
2. 🟡 **尽快实施**：API 规范、数据库优化、配置管理
3. 🟢 **规划实施**：架构重构、监控、测试
4. ⚪ **长期规划**：数据库迁移、缓存、消息队列

### 实施建议

- **渐进式改进**：不要一次性重构所有内容
- **保持稳定**：每个阶段完成后确保系统稳定
- **充分测试**：每个改进都要有对应的测试
- **监控效果**：持续监控改进效果
- **灵活调整**：根据实际情况调整计划

---

## 📚 参考资料

- [Rust 官方文档](https://doc.rust-lang.org/)
- [Axum 框架文档](https://docs.rs/axum/)
- [清洁架构](https://blog.cleancoder.com/uncle-bob/2012/08/13/the-clean-architecture.html)
- [REST API 设计最佳实践](https://restfulapi.net/)
- [Rust 性能优化](https://nnethercote.github.io/perf-book/)

---

**文档版本**：1.0
**最后更新**：2026-04-25
**维护者**：ntd Team
