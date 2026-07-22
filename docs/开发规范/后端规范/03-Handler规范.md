# 后端规范 03：Handler 规范

> 定义 Handler（路由处理器）的编写规则。

---

## 1. Handler 函数签名

- 使用 Axum 标准的 `Extension` 或 `State` 注入共享状态
- 返回类型统一为 `impl IntoResponse` 或 `Result<impl IntoResponse, AppError>`

```rust
// ✅ 标准写法：
// 通过 State 注入 AppContext，通过 Json/Query/Path 提取请求参数，
// 返回 Result<Json<Response>, AppError> 让统一错误处理接管。
pub async fn list_todos(
    State(ctx): State<AppContext>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<TodoResponse>>, AppError> {
    let todos = ctx.todo_service.list(params).await?;
    Ok(Json(todos))
}
```

---

## 2. 参数提取

| 来源 | 提取器 | 示例 |
|------|--------|------|
| URL 路径参数 | `Path<String>` | `Path(todo_id)` |
| URL 查询参数 | `Query<T>` | `Query(ListParams)` |
| JSON 请求体 | `Json<T>` | `Json(CreateTodoRequest)` |
| 应用状态 | `State<AppContext>` | `State(ctx)` |
| 请求头 | `HeaderMap` | `headers: HeaderMap` |

---

## 3. 路由注册

在 `handlers/mod.rs` 中统一注册：

```rust
// 所有路由在 mod.rs 中聚合注册，按资源分组调用 router() 方法。
// 每个 handler 模块导出一个 router() 函数返回 Router，通过 .nest() 或 .merge() 组合。
pub fn router() -> Router<AppContext> {
    Router::new()
        .nest("/api/v1/todos", todo::router())
        .nest("/api/v1/executions", execution::router())
}
```

---

## 4. 参数校验

- 在 Handler 入口使用 `validator` crate 做声明式校验
- 复杂校验（跨字段依赖、数据库唯一性）放在 Service 层

```rust
// ✅ 使用 validate 宏做声明式参数校验：
// 请求体结构体上的 validate 属性会在反序列化后自动执行校验逻辑。
#[derive(Debug, Deserialize, Validate)]
pub struct CreateTodoRequest {
    #[validate(length(min = 1, max = 200))]
    pub title: String,
}
```

---

## 5. Response 构建

所有 Response 通过 `Json<T>` 或 `AppError` 统一处理：

```rust
// ✅ 成功返回 Json 包裹的结构体，让 axum 自动设置 Content-Type 为 application/json。
Ok(Json(todo_response))

// ✅ 错误通过 AppError 传播，由统一错误处理器转换为 HTTP 错误码和 JSON 错误体。
Err(AppError::NotFound(format!("Todo {} not found", id)))
```

---

## 6. 禁止行为

- ❌ Handler 中直接调用 SeaORM / 数据库方法
- ❌ Handler 中包含 if-else 业务判断逻辑
- ❌ Handler 中打印日志到 stdout（使用 tracing）
- ❌ 在 Handler 中处理事务开始/提交
