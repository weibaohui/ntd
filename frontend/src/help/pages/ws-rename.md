# 编辑工作空间名称

## 功能位置
工作空间页 → 工作空间卡右侧「更多」`Dropdown` →「编辑」→ 名称 `Input` + 「保存」按钮

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  User["更多→编辑"] --> setEditingDir["setEditingDirId + setEditingDirName"]
  User["输入名称"] --> setEditingDirName["setEditingDirName"]
  User["点击保存"] --> handleUpdate["handleUpdateWorkspaceName(id)"]
  handleUpdate -->|"db.updateWorkspace(id, name)"| API["PUT /api/v1/workspaces/{id}"]
  API --> setWorkspaces["setWorkspaces map name"]
```

## 谑用关系链路图

```mermaid
flowchart TD
  Panel["WorkspacesPanel.tsx<br>WorkspacesPanel()"] --> editingWorkspaceIdState["useState editingWorkspaceId"]
  Panel --> editingWorkspaceNameState["useState editingWorkspaceName"]
  Panel --> Dropdown["更多 Dropdown onClick key=edit"]
  Dropdown --> setEditingDir["setEditingDirId(dir.id)<br>setEditingDirName(dir.name)"]
  Panel --> EditInput["Input value=editingWorkspaceName<br>onPressEnter=handleUpdate"]
  Panel --> SaveBtn["保存 Button<br>onClick=handleUpdate"]
  Panel --> CancelBtn["取消 Button<br>setEditingDirId(null)"]
  SaveBtn --> handleUpdateWorkspaceName["handleUpdateWorkspaceName(id)"]
  handleUpdateWorkspaceName --> trimCheck["name trim 非空"]
  trimCheck --> db1["db.updateWorkspace(id, name)"]
  db1 --> setWorkspaces["setWorkspaces map name"]
  db1 --> resetEdit["setEditingDirId(null)<br>setEditingDirName('')"]
```

## 数据结构图

```mermaid
classDiagram
  class Workspace {
    id: number
    name: string|null
  }
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> DisplayMode: 卡片展示名称
  DisplayMode --> EditMode: 更多→编辑
  EditMode --> DisplayMode: 取消
  EditMode --> Saving: 点击保存/回车
  Saving --> DisplayMode: 更新成功
  Saving --> EditMode: 更新失败
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/WorkspacesPanel.tsx` 的 `handleUpdateWorkspaceName` 回调
- **后端入口**：`backend/src/handlers/workspace.rs` 处理 `PUT /api/v1/workspaces/{id}`，`name` 必填
- **注意**：`updateWorkspace` 的 `name` 是必传字段，即使只想改 worktree 开关也需带上当前 `name`；后端 handler 区分 `None`/`Some` 语义，前端用 `hasOwnProperty` 表达「故意不传」
- **扩展**：如需编辑其他字段（如 `path`），后端 update handler 的 body 追加可选字段，前端编辑模式追加对应 Input
