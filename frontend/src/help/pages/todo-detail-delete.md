# 删除事项

## 功能位置

事项详情页 → `PageCard` 右上角 `extra` 操作按钮组的「删除」按钮（`DeleteOutlined`，`Popconfirm` 二次确认 `title="删除任务" description="确定要删除吗？"`，`type="text"`，`className="icon-btn"`，`aria-label="删除任务"`）。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[点击删除按钮] --> Pop["Popconfirm 弹出确认"]
  Pop -->|"onConfirm"| handleDelete["TodoDetail.handleDelete"]
  handleDelete --> FE["db.deleteTodo(ws, todo.id)"]
  FE -->|DELETE /api/v1/workspaces/:ws/todos/:id| H["handlers/todo.rs::delete_todo"]
  H --> guard["workspace_guard::verify_todo_belongs_to_ws 归属校验"]
  H --> loopCheck["count_loop_steps_by_todo 引用计数校验"]
  loopCheck -->|">0"| Block["BadRequest 阻删提示移除 Loop 引用"]
  loopCheck -->|"=0"| clean["先清理 scheduler task + cancel running task"]
  clean --> DAO["db.delete_todo(id) 软删 deleted_at=now"]
  DAO --> T[(todos 表 deleted_at 列更新)]
  H --> OK["返回前端"]
  OK --> dispatch["dispatch DELETE_TODO + SELECT_TODO null"]
  OK --> messageOk["message.success '删除成功'"]
```

## 调用关系链路图

```mermaid
flowchart TD
  Btn["TodoDetailActions DeleteOutlined Popconfirm onConfirm=onDelete"] --> ctx["onActionsReady ctx.onDelete = handleDelete"]
  ctx --> handleDelete["TodoDetail.handleDelete useCallback"]
  handleDelete --> dbDelete["db.deleteTodo(selectedTodo.workspace_id, selectedTodo.id)"]
  dbDelete --> delete_todo["handler delete_todo"]
  delete_todo --> verifyTodo["verify_todo_belongs_to_ws"]
  delete_todo --> getTodo["DB.get_todo 取信息"]
  delete_todo --> countLoop["count_loop_steps_by_todo"]
  countLoop -->|"loop_ref_count > 0"| BadRequest["AppError::BadRequest 阻删"]
  countLoop -->|"=0"| removeScheduler["scheduler.remove_task_for_todo"]
  removeScheduler --> cancelTask["task_manager.cancel(task_id) 若正执行"]
  cancelTask --> softDelete["DB.delete_todo deleted_at=Set(Some(now))"]
  softDelete --> dispatchDelete["dispatch DELETE_TODO payload=todo.id"]
  dispatchDelete --> dispatchSelect["dispatch SELECT_TODO payload=null"]
```

## 数据结构图

```mermaid
classDiagram
class Todo_delete {
  +id: number
  +workspace_id: number
  +task_id: string
}
class delete_todo_DAO {
  +id: i64
  +deleted_at: ActiveValue Set(Some(now))
  +软删 不物理删除
}
class loop_steps_ref {
  +todo_id: i64 FK
  +引用计数 > 0 时阻删
}
class todos_table_del {
  +deleted_at TEXT
  +NULL 表示未删
  +非空表示软删
}
Todo_delete --> delete_todo_DAO : deleteTodo
delete_todo_DAO --> todos_table_del : exec_update
delete_todo_DAO ..> loop_steps_ref : handler 先校验引用
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 存在: todo deleted_at=NULL
  存在 --> 确认中: 点删除 Popconfirm 弹出
  确认中 --> 存在: 取消确认 onClose 不删
  确认中 --> 校验中: onConfirm handleDelete → db.deleteTodo
  校验中 --> 阻删: loop_steps 引用计数 > 0 → BadRequest
  阻删 --> 存在: 用户移除 Loop 引用后可重试
  校验中 --> 清理: 引用计数 = 0 remove scheduler + cancel running task
  清理 --> 软删: delete_todo deleted_at=now
  软删 --> [*]: dispatch DELETE_TODO + SELECT_TODO null UI 回列表态
```

## 开发指导

- **前端入口**：`frontend/src/components/todo-detail/TodoDetailActions.tsx` 的 `TodoDetailActions`（删除 `Popconfirm` + `Button`，L64-66）；回调链在 `frontend/src/components/TodoDetail.tsx` 的 `handleDelete`（L318-328），经 `onActionsReady` 上报给 `TodoDetailPage` 渲染到 `extra`。
- **后端入口**：`backend/src/handlers/todo.rs::delete_todo`（L399，DELETE `/api/v1/workspaces/:ws/todos/:id`），DAO 在 `backend/src/db/todo.rs::delete_todo`（L1060，软删 `deleted_at`）。
- **注意**：删除是破坏性操作，前端 `Popconfirm` 二次确认。后端删除前校验 `count_loop_steps_by_todo` 引用计数，被 Loop 环节引用的事项返回 BadRequest 不允许删（关注数据完整性，禁用环节也算引用）。删除前先 `scheduler.remove_task_for_todo` 清调度任务，若 todo 正在执行则 `task_manager.cancel(task_id)` 取消。DAO 是软删（`deleted_at=now`），不物理删除行，后续查询 `deleted_at IS NULL` 过滤。前端删除成功后 dispatch `DELETE_TODO` + `SELECT_TODO null`，UI 回列表态（详情页 selectedTodo 清空）。
- **扩展**：若需「恢复已删事项」，后端已有 `restore_todo` handler（POST `/api/v1/workspaces/:ws/todos/:id/archive` 的 restore 对偶），前端在列表增「已归档/已删」分区展示 deleted_at 非空项并提供恢复按钮即可。
