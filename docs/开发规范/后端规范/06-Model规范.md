# 后端规范 06：Model 规范

> 定义 SeaORM Entity、DTO 及枚举的编写规范。

---

## 1. Entity 定义

使用 SeaORM 的 `DeriveEntityModel` 派生宏：

```rust
// Entity 对应数据库表，使用 SeaORM 的 derive 宏。
// 每个字段的 Column 枚举变体用 #[sea_orm(column_type = "...")] 显式标注类型，
// 避免 SeaORM 的类型推断在迁移后失效。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "todos")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub title: String,
    pub description: Option<String>,
    pub status: StatusEnum,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}
```

---

## 2. 枚举定义

使用 SeaORM 的 `DeriveActiveEnum`：

```rust
// SeaORM 枚举需要与数据库中的字符串/varchar 列对应。
// enum_name 是 SeaORM 内部引用的名称，db_type 需匹配列类型。
// 修改枚举变体时需要同步更新数据库迁移脚本。
#[derive(Debug, Clone, PartialEq, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(Some(20))")]
pub enum StatusEnum {
    #[sea_orm(string_value = "active")]
    Active,
    #[sea_orm(string_value = "completed")]
    Completed,
    #[sea_orm(string_value = "archived")]
    Archived,
}
```

---

## 3. DTO 定义

```rust
// DTO 用于 API 请求/响应，使用 serde 做序列化/反序列化。
// 请求 DTO：带 Deserialize + Validate
// 响应 DTO：带 Serialize
#[derive(Debug, Deserialize, Validate)]
pub struct CreateTodoRequest {
    #[validate(length(min = 1, max = 200))]
    pub title: String,

    #[validate(length(max = 2000))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TodoResponse {
    pub id: i64,
    pub title: String,
    pub status: String,
    pub created_at: String,
}
```

---

## 4. Entity ↔ DTO 转换

通过 `From` trait 或 `impl From<Entity> for DTO` 做转换：

```rust
// 使用 From trait 做 Entity → DTO 的转换，
// 避免在 Handler/Service 中手写字段映射逻辑。
impl From<TodoModel> for TodoResponse {
    fn from(t: TodoModel) -> Self {
        Self {
            id: t.id,
            title: t.title,
            status: t.status.to_string(),
            created_at: t.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }
}
```

---

## 5. 文件组织

```text
models/
├── mod.rs        # 重新导出所有 Model 类型
├── todo.rs       # Todo Entity + DTO + 枚举
├── execution.rs  # Execution Entity + DTO + 枚举
└── ...
```

- 每个业务实体放在独立文件
- `mod.rs` 做 `pub mod` 和 `pub use` 统一导出
- 一个文件内 Entity / DTO / 枚举共存（按职责划分，不按类型划分）
