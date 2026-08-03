# 新建工作空间

## 功能位置
工作空间页 →「新建工作空间」Card → 名称 `Input` + 路径 `Input` +「添加」按钮（`PlusOutlined`）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  User["输入名称+路径<br>点击添加"] --> handleAddProjectDirectory["handleAddProjectDirectory"]
  handleAddProjectDirectory --> validate{path/name 非空?}
  validate -->|"否"| errMsg["message.error"]
  validate -->|"是"| db1["db.createProjectDirectory(path, name)"]
  db1 -->|"POST"| API["POST /api/v1/project-directories"]
  API -->|"ProjectDirectory"| setProjectDirectories["setProjectDirectories<br>insert + sort by path"]
```

## 调用关系链路图

```mermaid
flowchart TD
  Panel["ProjectDirectoriesPanel.tsx<br>ProjectDirectoriesPanel()"] --> newDirNameState["useState newDirName"]
  Panel --> newDirPathState["useState newDirPath"]
  Panel --> addingDirState["useState addingDir"]
  Panel --> handleAddProjectDirectory["handleAddProjectDirectory"]
  handleAddProjectDirectory --> trimCheck["path/name trim 非空校验"]
  trimCheck --> setAddingDir["setAddingDir(true)"]
  setAddingDir --> db1["db.createProjectDirectory(path, name)"]
  db1 --> setProjectDirectories["setProjectDirectories<br>filter d.id !== dir.id + append + sort"]
  db1 --> resetInputs["setNewDirPath('')<br>setNewDirName('')"]
  db1 --> messageSuccess["message.success"]
```

## 数据结构图

```mermaid
classDiagram
  class ProjectDirectory {
    id: number
    path: string
    name: string|null
    created_at: string
    updated_at: string
    git_worktree_enabled: boolean
    auto_cleanup: boolean
  }
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Idle: 页面加载
  Idle --> Adding: 点击添加（名称+路径非空）
  Adding --> Idle: 添加成功 setProjectDirectories
  Adding --> Idle: 添加失败
  Idle --> Idle: 名称或路径为空 message.error
```

## 开发指导
- **前端入口**：`frontend/src/components/settings/ProjectDirectoriesPanel.tsx` 的 `handleAddProjectDirectory` 回调；`Input onPressEnter` 也绑定此函数
- **后端入口**：`backend/src/handlers/project_directory.rs` 处理 `POST /api/v1/project-directories`，要求 `path` 和 `name` 必填
- **注意**：新建时不传 `gitWorktreeEnabled` / `autoCleanup`（后端 create 不消费这些字段），策略需在创建后通过 update 设置；插入后按 `path.localeCompare` 排序保持列表稳定
- **扩展**：如需新建时同步设策略，后端 create handler 需接收并写入 `git_worktree_enabled` / `auto_cleanup`，前端 `createProjectDirectory` 恢复 options 参数
