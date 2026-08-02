# 编辑事项

## 功能位置

事项详情页 → `PageCard` 右上角 `extra` 操作按钮组的「编辑」按钮（`EditOutlined`，`type="text"`，`className="icon-btn"`，`aria-label="编辑任务"`）。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[点击编辑按钮] --> onEdit["TodoDetailActions onEdit → TodoDetail onActionsReady ctx.onEdit"]
  onEdit --> drawerOpen["setTodoDrawerOpen(true)"]
  drawerOpen --> TD["TodoDrawer todo=selectedTodo 打开编辑模式"]
  TD -->|handleSave isEditMode=true| FE["db.updateTodo(ws, todo.id, title, prompt, status, executor, schedulerEnabled, schedulerConfig, workspaceId, webhookEnabled, acceptanceCriteria, undefined, expertName, model)"]
  FE -->|PUT /api/v1/workspaces/:ws/todos/:id| H["handlers/todo.rs::update_todo"]
  H --> guard["workspace_guard::verify_todo_belongs_to_ws"]
  H --> DAO["db/todo.rs::update_todo_full (TodoUpdate)"]
  DAO --> T[(todos 表 多列更新)]
  TD -->|保存后补充| Sched["db.updateScheduler(ws, todo.id, enabled, config)"]
  TD -->|保存后补充| Tags["db.updateTodoTags(ws, todo.id, selectedTags)"]
  H --> Resp["Todo 返回"]
  Resp --> reload["TodoDetail reloadSelectedTodo + getAllTodos SET_TODOS_BY_WORKSPACE"]
  Resp --> messageOk["message.success '任务已更新'"]
```

## 调用关系链路图

```mermaid
flowchart TD
  Btn["TodoDetailActions EditOutlined onClick=onEdit"] --> ctx["onActionsReady ctx.onEdit = () => setTodoDrawerOpen(true)"]
  ctx --> TodoDrawer["TodoDrawer todo=selectedTodo isEditMode=true"]
  TodoDrawer --> handleSave["handleSave 校验 title.trim()"]
  handleSave --> dbUpdate["db.updateTodo(全字段透传)"]
  dbUpdate --> update_todo["handler update_todo"]
  update_todo --> verify["verify_todo_belongs_to_ws"]
  update_todo --> current["require_todo 取当前值填充缺省字段"]
  update_todo --> update_todo_full["DB.update_todo_full"]
  update_todo --> workspaceSync["update_todo_workspace 双字段同步"]
  TodoDrawer --> updateScheduler["db.updateScheduler"]
  TodoDrawer --> updateTodoTags["db.updateTodoTags"]
  TodoDrawer --> onSaved["onSaved → reloadSelectedTodo + dispatch SET_TODOS_BY_WORKSPACE"]
```

## 数据结构图

```mermaid
classDiagram
class Todo {
  +id: number
  +title: string
  +prompt: string
  +status: TodoStatus
  +executor: string
  +scheduler_enabled: boolean
  +scheduler_config: string
  +workspace_id: number
  +webhook_enabled: boolean
  +acceptance_criteria: string
  +expert_name: string
  +model: string
}
class UpdateTodoRequest {
  +title: Option~String~
  +prompt: Option~String~
  +status: Option~TodoStatus~
  +executor: Option~String~
  +workspace_id: Option~i64~
  +scheduler_enabled: Option~bool~
  +scheduler_config: Option~String~
  +webhook_enabled: Option~bool~
  +acceptance_criteria: Option~String~
  +expert_name: Option~String~
  +model: Option~String~
}
class TodoUpdate {
  +id: i64
  +title: str
  +prompt: str
  +status: TodoStatus
  +executor: Option~str~
  +workspace_id: Option~i64~
  +scheduler_enabled: Option~bool~
  +scheduler_config: Option~str~
}
Todo --> UpdateTodoRequest : TodoDrawer handleSave body
UpdateTodoRequest --> TodoUpdate : handler 构造
TodoUpdate --> todos_table : DAO 落库
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 事项原态: selectedTodo 持旧值
  事项原态 --> 编辑中: 点编辑 → TodoDrawer 打开 todo=selectedTodo
  编辑中 --> 校验中: handleSave title.trim() 非空
  校验中 --> 更新中: db.updateTodo PUT /todos/:id
  更新中 --> 已更新: update_todo_full 落库 + Todo 返回
  已更新 --> 同步前端: reloadSelectedTodo + dispatch SET_TODOS_BY_WORKSPACE
  已更新 --> [*]: todos 表多列更新
  编辑中 --> 事项原态: onClose 关闭抽屉不保存
  校验中 --> 编辑中: title 空 → message.error 不提交
```

## 开发指导

- **前端入口**：`frontend/src/components/todo-detail/TodoDetailActions.tsx` 的 `TodoDetailActions`（编辑 `Button`，L63）；回调链在 `frontend/src/components/TodoDetail.tsx` 的 `onActionsReady`（L333-345，`onEdit: () => setTodoDrawerOpen(true)`）与 `TodoDrawer` 渲染（L485-499）；`TodoDrawer` 编辑保存在 `frontend/src/components/TodoDrawer.tsx` 的 `handleSave`（L204-278，`isEditMode && todo` 分支 L222-238）。
- **后端入口**：`backend/src/handlers/todo.rs::update_todo`（L283，PUT `/api/v1/workspaces/:ws/todos/:id`），DAO 在 `backend/src/db/todo.rs::update_todo_full`（L511）。调度器补充更新走 `db.updateScheduler` → `backend/src/handlers/scheduler.rs`，标签走 `db.updateTodoTags` → `handlers/todo.rs::update_todo_tags`。
- **注意**：编辑入口由 `TodoDetail` 内部 `setTodoDrawerOpen(true)` 打开抽屉，`todo=selectedTodo` 让 `TodoDrawer` 走 `isEditMode=true` 分支。`handleSave` 先校验 `title.trim()` 非空，再 `db.updateTodo` 全字段透传（保持未改字段不变）。后端 `update_todo` handler 先 `verify_todo_belonds_to_ws` 归属校验，再 `require_todo` 取当前值填充请求中缺省字段（None 表示不改）。`model` 字段 `null → ''` 清除任务级模型，非空 → 设置。`onSaved` 用 `reloadSelectedTodo` 拉最新数据避免 local state 滞后，并 `getAllTodos` 同步 workspace 桶。
- **扩展**：新增可编辑字段（如 `review_enabled`）时，在 `TodoDrawer` 表单增控件 → `handleSave` 的 `db.updateTodo` 参数增字段 → `UpdateTodoRequest` / `TodoUpdate` 加字段 → `update_todo_full` DAO 写列 → `Todo` 响应同步。
