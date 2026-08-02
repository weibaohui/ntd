# 新建事项

## 功能位置

事项列表页 → 顶部 header「新建」主按钮（`PlusOutlined`）。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[点击新建按钮] --> TD[TodoDrawer 打开创建模式]
  TD -->|handleSave| DB_FE[db.createTodo / db.updateTodo]
  DB_FE -->|POST /api/v1/workspaces/:ws/todos| H[handlers/todo.rs::create_todo]
  H -->|create_todo_with_extras| DAO[db/todo.rs::create_todo_with_extras]
  DAO --> T[(todos 表)]
  DAO --> WS[(project_directories 表 — 反查 path)]
  H -->|add_todo_tag| TT[(todo_tags 表)]
  H -->|update_todo_scheduler| SC[(scheduler 表)]
  H --> OK[Todo 返回前端]
  OK --> REFRESH[dispatch SET_TODOS_BY_WORKSPACE + TODO_LIST_REFRESH_EVENT]
```

## 调用关系链路图

```mermaid
flowchart TD
  TodoListPage.onCreate -->|App.tsx setEditingTodo null| TodoDrawer["TodoDrawer (isEditMode=false)"]
  TodoDrawer.handleSave -->|title.trim 校验| DBCreate["db.createTodo"]
  DBCreate -->|fetch POST| create_todo["handler create_todo"]
  create_todo --> get_default_executor_name["DB.get_default_executor_name"]
  create_todo --> get_project_directory_by_id["DB.get_project_directory_by_id"]
  create_todo --> create_todo_with_extras["DB.create_todo_with_extras"]
  create_todo --> add_todo_tag["DB.add_todo_tag (循环 tag_ids)"]
  create_todo --> update_todo_scheduler["DB.update_todo_scheduler"]
  TodoDrawer -->|创建后补充| DBUpdate["db.updateTodo"]
  DBUpdate --> update_todo_full["DB.update_todo_full (action_type/expert/model)"]
```

## 数据结构图

```mermaid
classDiagram
  class CreateTodoRequest {
    +title: String
    +prompt: Option~String~
    +workspace_id: Option~i64~
    +executor: Option~String~
    +tag_ids: Vec~i64~
    +scheduler_enabled: Option~bool~
    +scheduler_config: Option~String~
    +acceptance_criteria: Option~String~
    +webhook_enabled: Option~bool~
    +action_type: Option~String~
    +action_key: Option~String~
    +expert_name: Option~String~
    +model: Option~String~
  }
  class Todo {
    +id: i64
    +title: String
    +prompt: String
    +status: TodoStatus
    +workspace_id: Option~i64~
    +executor: Option~String~
    +expert_name: Option~String~
    +model: Option~String~
    +scheduler_enabled: bool
    +scheduler_config: Option~String~
    +tag_ids: Vec~i64~
    +acceptance_criteria: Option~String~
    +webhook_enabled: bool
    +auto_review_enabled: bool
  }
  class todos_table {
    +id INTEGER PK
    +title TEXT
    +prompt TEXT
    +status TEXT
    +workspace_id INTEGER FK
    +workspace_path TEXT
    +executor TEXT
    +expert_name TEXT
    +model TEXT
    +action_type TEXT
    +action_key TEXT
    +acceptance_criteria TEXT
    +webhook_enabled INTEGER
    +auto_review_enabled INTEGER
    +created_at TEXT
    +updated_at TEXT
  }
  CreateTodoRequest ..> Todo : handler 返回
  Todo ..> todos_table : DAO 落库
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 不存在
  不存在 --> pending: create_todo_with_extras
  pending --> 有标签: add_todo_tag (循环)
  pending --> 有调度: update_todo_scheduler
  pending --> 有action/expert/model: update_todo_full
  有标签 --> [*]
  有调度 --> [*]
  有action_expert_model --> [*]
```

## 开发指导

- **前端入口**：`frontend/src/components/TodoDrawer.tsx` 的 `handleSave`（约 L204）。创建分支在 `isEditMode && todo` 为 false 的 else 块（L239-269）。
- **后端入口**：`backend/src/handlers/todo.rs::create_todo`（L116）。DAO 在 `backend/src/db/todo.rs::create_todo_with_extras`。
- **注意**：
  - `title.trim()` 必须非空，否则 handler 返回 `BadRequest("Title is required")`。
  - `workspace_id` 必填（L145-147），handler 按 id 反查 `project_directories` 拿 path，同步写入 `todos.workspace_id` + `workspace_path` 两列。
  - `executor` 缺省时取 DB 默认执行器（`get_default_executor_name`），handler 层只解析一次再下传 DAO。
  - 创建后若需补 `action_type`/`expert_name`/`model`，走 `update_todo_full`（handler L176-195），因为 `create_todo_with_extras` 不支持这些字段。
  - `tag_ids` 是创建后循环 `add_todo_tag` 写 `todo_tags` 关联表。
- **扩展**：新增字段时，在 `CreateTodoRequest` 加字段 → `create_todo` handler 取值 → `create_todo_with_extras` 或后续 `update_todo_full` 落库 → `Todo` 响应结构同步加字段。
