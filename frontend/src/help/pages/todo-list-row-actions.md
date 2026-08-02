# 单行操作（删除/执行/带参执行）

## 功能位置

事项列表页 → 列表视图 `TodoListView` Table 每行末列的「更多操作」`Dropdown`（`MoreOutlined`，`aria-label="更多操作"`）菜单项：执行一次 / 带参执行 / 编辑 / 删除。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[点击行末 Dropdown 菜单项] --> Guard["guard: info.domEvent.stopPropagation 后调 action(todo)"]
  Guard -->|"执行一次"| Exec["db.executeTodo(ws, id, executor)"]
  Guard -->|"带参执行"| ArgsModal["弹 ExecuteWithArgsModal"]
  Guard -->|"编辑"| Edit["onEditTodo → App.tsx setEditingTodo 打开 TodoDrawer 编辑模式"]
  Guard -->|"删除"| Confirm["Modal.confirm 二次确认"]
  Exec -->|POST /api/v1/workspaces/:ws/executions| EH["handlers/execution.rs::execute_handler"]
  EH --> start_todo_execution["start_todo_execution (record_id + task_id)"]
  start_todo_execution --> ER[(execution_records 表)]
  Confirm -->|"onOk"| Del["db.deleteTodo(ws, id)"]
  Del -->|DELETE /api/v1/workspaces/:ws/todos/:id| DH["handlers/todo.rs::delete_todo"]
  DH --> loopCheck["count_loop_steps_by_todo 引用校验"]
  DH --> softDel["db.delete_todo 软删 deleted_at"]
  softDel --> T[(todos 表 deleted_at = now)]
  EH --> reload["onReload 刷新列表"]
  Del --> reload
```

## 调用关系链路图

```mermaid
flowchart TD
  row["TodoListView 行末 Dropdown"] --> items["buildRowActionItems(todo, callbacks)"]
  items --> execute["key=execute onClick → onExecuteTodo"]
  items --> args["key=execute-with-args onClick → onExecuteWithArgs"]
  items --> edit["key=edit onClick → onEditTodo"]
  items --> del["key=delete danger onClick → onDeleteTodo"]
  onExecuteTodo["useTodoRowActions.handleExecuteTodo"] -->|"无 params"| dbExec["db.executeTodo(ws, id, executor, undefined)"]
  onExecuteWithArgs["useTodoRowActions.handleExecuteWithArgs"] --> setPending["setPendingExecuteTodo + setExecuteArgs('') + Modal open"]
  confirm["confirmExecuteWithArgs"] --> dbExec2["db.executeTodo(ws, id, executor, {message})"]
  onDeleteTodo["useTodoRowActions.handleDeleteTodo"] --> confirmModal["Modal.confirm"]
  confirmModal --> dbDel["db.deleteTodo"]
  dbExec --> messageOk["message.success '任务已开始执行'"]
  dbExec --> reload["onReload"]
```

## 数据结构图

```mermaid
classDiagram
class TodoCenterItem_row {
  +id: number
  +title: string
  +executor: string
  +workspace_id: number
}
class ExecuteRequest {
  +todo_id: i64
  +executor: Option~String~
  +params: Option~Record~String,String~
  +message: Option~String~
  +model: Option~String~
}
class delete_todo_DAO {
  +id: i64
  +deleted_at: Set(Some(now))
  +软删 不物理删除
}
class batch_todo_ids {
  +loop_ref_count: i64
  +被 loop_steps 引用时不允许删除
}
TodoCenterItem_row --> ExecuteRequest : executeTodo
TodoCenterItem_row --> delete_todo_DAO : deleteTodo
delete_todo_DAO ..> batch_todo_ids : handler 先校验引用计数
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> pending: 事项存在 deleted_at=NULL
  pending --> running: 执行一次 / 带参执行 start_todo_execution
  running --> completed: 执行结束 execution_records 落库
  running --> failed: 执行失败
  pending --> 已删除: 删除二次确认 → delete_todo 软删 deleted_at=now
  已删除 --> [*]: 列表 reload 后该行消失（deleted_at IS NULL 过滤）
  已删除 --> 阻删: loop_steps 引用计数>0 → BadRequest
  pending --> pending: 编辑 → TodoDrawer updateTodo
```

## 开发指导

- **前端入口**：`frontend/src/components/todo-list/TodoListPageParts.tsx` 的 `useTodoRowActions`（L109-186，`handleExecuteTodo` / `handleDeleteTodo` / `handleExecuteWithArgs` / `confirmExecuteWithArgs`）；菜单项构造在 `frontend/src/components/todo-list/TodoListView.tsx` 的 `buildRowActionItems`（L126-167）与 `renderActionsColumn`（L170-185）。
- **后端入口**：执行 `backend/src/handlers/execution.rs::execute_handler`（L200，POST `/api/v1/workspaces/:ws/executions`）；删除 `backend/src/handlers/todo.rs::delete_todo`（L399，DELETE `/api/v1/workspaces/:ws/todos/:id`）；DAO 删除在 `backend/src/db/todo.rs::delete_todo`（L1060，软删 `deleted_at`）。
- **注意**：`guard` 包装器统一先 `stopPropagation` 再执行业务，避免行点击跳详情与菜单项冒泡冲突。删除是破坏性操作，`handleDeleteTodo` 用 `Modal.confirm` 二次确认（`okButtonProps: { danger: true }`）。后端删除前先校验 `count_loop_steps_by_todo` 引用计数，被 Loop 环节引用的事项返回 BadRequest 不允许删。带参执行的 `params` 构造为 `{ message: executeArgs.trim() }`，与后端 `ExecuteRequest.message` 对齐（后端把 message 注入 `{{message}}` 占位符）。执行成功后前端调 `onReload` 刷新列表，详情页执行另走 `ADD_EXECUTION_RECORD` dispatch。
- **扩展**：新增菜单项（如「归档」）时，在 `buildRowActionItems` 的 items 数组增项 → `useTodoRowActions` 加对应 handler → 透传给 `TodoListView` 的 callbacks。
