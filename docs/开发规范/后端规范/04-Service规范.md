# 后端规范 04：Service 规范

> 定义 Service 层（业务逻辑层）的编写规则。

---

## 1. Service 结构体

每个 Service 是一个 struct，通过构造函数接收所需依赖：

```rust
// 每个 Service 封装一个业务领域的全部操作，
// 通过构造函数注入依赖（Repository、其他 Service、外部客户端）。
// 不包含 HTTP 相关类型。
pub struct TodoService {
    repo: TodoRepository,
    user_service: Arc<UserService>,
}

impl TodoService {
    pub fn new(repo: TodoRepository, user_service: Arc<UserService>) -> Self {
        Self { repo, user_service }
    }
}
```

---

## 2. 方法签名

- 输入：自定义 DTO 或原始类型（`i64`、`String` 等）
- 输出：`Result<T, AppError>`
- 避免接受 `axum::extract::Query` / `Json` 等 HTTP 类型

```rust
// ✅ Service 方法以纯 Rust 类型作为输入输出，不依赖 Axum 提取器。
// 这样 Service 可被 Handler 和非 HTTP 上下文（如 CLI、执行器）复用。
pub async fn create(&self, input: CreateTodoInput) -> Result<TodoResponse, AppError> {
    // ... 业务逻辑
}
```

---

## 3. 业务逻辑组织

### 3.1 事务边界

```rust
// Service 层控制事务起点和终点：
// 在 Service 方法中建立事务，然后通过事务上下文调用 Repository 方法。
// 这样事务边界清晰，且 Repository 方法仍然可独立测试。
pub async fn create_with_tags(&self, input: CreateTodoInput) -> Result<TodoResponse, AppError> {
    let txn = self.repo.begin().await?;
    // ... 多个 Repository 调用
    txn.commit().await?;
    Ok(response)
}
```

### 3.2 外部服务调用

```rust
// 调用外部服务时使用适配器（Adapter）模式：
// 在 Service 中持有外部服务的 trait 对象引用，便于测试时 mock。
pub async fn notify_executor(&self, todo_id: i64) -> Result<(), AppError> {
    self.executor_adapter.dispatch(todo_id).await?;
    Ok(())
}
```

### 3.3 缓存

如果需要缓存，使用 `AppContext` 中的缓存实例，不在 Service 内部创建：

```rust
// 缓存实例在 AppContext 中统一管理（生命周期、失效策略），
// Service 通过构造函数获取，不自行初始化缓存。
```

---

## 4. 错误处理

Service 方法返回 `AppError`：

```rust
// 业务逻辑中的各种失败场景用 AppError 枚举的不同变体表达，
// 让 Handler 层可以据此决定 HTTP 状态码。
if !user.is_active {
    return Err(AppError::Forbidden("User is inactive".into()));
}
```

---

## 5. 禁止行为

- ❌ 引用 `axum` / `http` crate 的类型
- ❌ 直接返回 `StatusCode` 或 HTTP Response
- ❌ 在 Service 中处理请求参数的格式校验（应在 Handler 中完成）
- ❌ 在 Service 中操作文件系统或网络 I/O（使用适配器封装）
