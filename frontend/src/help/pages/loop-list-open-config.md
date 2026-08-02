# 打开工作空间环路配置页

## 功能位置

环路（列表） → 顶部 `PageCard` 的 `extra` 区 → `LoopListHeader` 「配置」按钮（`Button` 带 `SettingOutlined`，`workspaceId == null` 时禁用）。

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户点击配置按钮] --> OC["handleOpenLoopConfig"]
  OC --> GD["getProjectDirectories()"]
  GD --> API["GET /api/project-directories"]
  API --> H1[project_directories handler]
  H1 --> DB[(project_directories 表)]
  GD --> FIND["dirs.find id === workspaceId"]
  FIND -->|"未找到"| WARN["message.warning 未找到当前工作空间"]
  FIND -->|"找到"| SC["setCurrentWorkspace found"]
  SC --> SO["setLoopConfigOpen true"]
  SO --> LLP["LoopListPage 渲染 WorkspaceLoopConfigPage 替代列表"]
  LLP --> RT[ReviewTemplatesPanel 评审模板管理]
```

## 调用关系链路图

```mermaid
flowchart TD
  Header["LoopListHeader Button onClick"] -->|"onOpenConfig"| LLP["LoopListPage handleOpenLoopConfig"]
  LLP -->|"useLoopConfig"| CFG["useLoopConfig handleOpenLoopConfig"]
  CFG -->|"getProjectDirectories"| DBT["database/todos getProjectDirectories"]
  DBT -->|"unwrap"| API["api.get /api/project-directories"]
  CFG -->|"setCurrentWorkspace + setLoopConfigOpen true"| ST["useLoopConfig state"]
  LLP -->|"loopConfigOpen && currentWorkspace"| WCP["WorkspaceLoopConfigPage workspace onBack"]
  WCP --> RTP["ReviewTemplatesPanel workspaceId"]
```

## 数据结构图

```mermaid
classDiagram
  class UseLoopConfigArgs {
    +workspaceId: number | null
  }
  class UseLoopConfigState {
    +loopConfigOpen: boolean
    +currentWorkspace: ProjectDirectory | null
    +handleOpenLoopConfig(): void
    +handleCloseLoopConfig(): void
  }
  class ProjectDirectory {
    +id: number
    +name: string
    +path: string
    +git_worktree_enabled: boolean
    +auto_cleanup: boolean
  }
  class WorkspaceLoopConfigPage {
    +workspace: ProjectDirectory
    +onBack(): void
  }
  UseLoopConfigState --> ProjectDirectory : currentWorkspace
  WorkspaceLoopConfigPage --> ProjectDirectory : props.workspace
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> 列表态: loopConfigOpen false
  列表态 --> 加载中: 点击配置 handleOpenLoopConfig
  加载中 --> 列表态: getProjectDirectories 失败 message.error
  加载中 --> 未找到: 未匹配当前 workspaceId message.warning
  加载中 --> 配置态: 找到 setLoopConfigOpen true
  配置态 --> 列表态: onBack handleCloseLoopConfig 或 workspace 切换 useEffect 关闭
  配置态 --> 配置态: ReviewTemplatesPanel 内管理评审模板
```

## 开发指导

- **前端入口**：`frontend/src/components/loop-list/LoopListPageParts.tsx` 的 `useLoopConfig` hook（`handleOpenLoopConfig`/`handleCloseLoopConfig`），渲染分支在 `frontend/src/components/loop-list/index.tsx` 的 `LoopListPage`（`if (loopConfigOpen && currentWorkspace) return <WorkspaceLoopConfigPage ...>`）。配置页本体在 `frontend/src/components/settings/workspace/WorkspaceLoopConfigPage.tsx`。
- **后端入口**：仅 `getProjectDirectories`（`GET /api/project-directories`）取工作空间目录，环路配置页本体是前端 `ReviewTemplatesPanel`（评审模板管理），不直接落环路后端接口。
- **注意**：`handleOpenLoopConfig` 在 `workspaceId == null` 时直接 return；工作空间切换时 `LoopListPage` 的 `useEffect([workspaceId, handleCloseLoopConfig])` 会自动关闭配置页回列表态；`handleCloseLoopConfig` 同步清 `currentWorkspace` 避免残留引用。
- **扩展**：要在配置页加环路相关设置，在 `WorkspaceLoopConfigPage` 内新增子面板，需要工作空间上下文时复用 `workspace.id`；`ReviewTemplatesPanel` 与环路「评审模板」概念在 044 后已解耦，保留是为了给工艺门禁制评审提供模板管理入口。
