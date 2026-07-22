# 后端规范 08：数据库与 SeaORM 规范

> 定义 SeaORM 数据库操作规范。

---

## 1. 数据库连接

数据库连接池在应用启动时初始化，通过 `AppContext` 共享：

```rust
// 使用 sqlx::SqlitePool 或 SeaORM 的 Database::connect 建立连接池。
// 连接配置从 config.yaml 读取，区分开发/生产环境。
// 开发环境：~/.ntd/data.dev.db，端口 18088
// 生产环境：~/.ntd/data.db，端口 8088
pub async fn init_db(config: &DbConfig) -> Result<DatabaseConnection, DbErr> {
    let url = format!("sqlite:{}?mode=rwc", config.path);
    Database::connect(&url).await
}
```

---

## 2. Repository 模式

使用 Repository 封装数据访问，而不是在 Service 中直接使用 SeaORM：

```rust
// Repository 层封装所有数据查询逻辑：
// - 提供语义化的方法名（find_by_xxx, list_by_xxx）
// - 返回 Option<T> 或 Result<T, AppError>
// - 不暴露 SeaORM 查询细节到 Service 层
pub struct TodoRepository {
    db: DatabaseConnection,
}

impl TodoRepository {
    pub async fn find_by_id(&self, id: i64) -> Result<Option<TodoModel>, DbErr> {
        Entity::find_by_id(id).one(&self.db).await
    }

    pub async fn list_by_status(&self, status: StatusEnum) -> Result<Vec<TodoModel>, DbErr> {
        Entity::find()
            .filter(Column::Status.eq(status))
            .order_by(Column::CreatedAt, Order::Desc)
            .all(&self.db)
            .await
    }
}
```

---

## 3. 查询编写

```rust
// 优先使用 SeaORM 的 Query Builder，仅在复杂查询时使用 raw SQL。
// 所有查询参数使用绑定变量，禁止拼接 SQL 字符串（防范 SQL 注入）。
//
// ❌ 禁止：format!("SELECT * FROM todos WHERE id = {}", id)
// ✅ 推荐：Entity::find_by_id(id)
// ✅ 复杂查询：使用 .filter() / .find() 链式调用
```

---

## 4. 迁移管理

```rust
// 数据库迁移在应用启动时自动执行。
// 迁移文件以时间戳 + 描述命名，按顺序执行。
// 禁止手动修改已合入的迁移文件（通过新迁移文件做变更）。
//
// 迁移流程：
// 1. 新建迁移文件（增加/修改表结构）
// 2. 更新对应的 Entity 定义
// 3. 应用启动时自动执行未应用的迁移
```

---

## 5. 禁止行为

- ❌ Service/Handler 中直接使用 SeaORM Entity 的 `find()`/`insert()` 方法
- ❌ 生产环境代码中使用 N+1 查询模式（使用 `find_with_related` 或显式 `join`）
- ❌ 在事务外执行"读后写"的竞态敏感操作
- ❌ 使用 SeaORM `ActiveModel` 的未定义字段（通过 `Set(value)` 显式设置）
