# 打开消息配置抽屉

## 功能位置

消息页 → 顶部 `MessageHeader` 的「配置」按钮（`SettingOutlined`）

## 数据流图（前端 → 后端）

```mermaid
flowchart LR
  U[用户点击配置按钮] --> HDR["MessageHeader onOpenConfig"]
  HDR --> SO["setConfigDrawerOpen(true)"]
  SO --> MCD[MessageConfigDrawer 打开]
  MCD --> WSP["WorkspaceSlashCommandsPanel 挂载"]
  MCD --> DRP["DefaultResponseConfigPanel 挂载"]
  WSP --> DB1["db.getWorkspaceSlashCommands(workspaceId)"]
  WSP --> DB2["db.getAllTodos(workspaceId)"]
  WSP --> DB3["dbLoops.listLoops(workspaceId)"]
  DB1 --> API1["GET /api/v1/workspaces/{ws}/slash-commands"]
  DB2 --> API2["GET /api/v1/workspaces/{ws}/todos"]
  DB3 --> API3["GET /api/v1/workspaces/{ws}/loops"]
  DRP --> DB4["db.getWorkspaceSettings(workspaceId)"]
  DRP --> DB5["db.getAllTodos(workspaceId)"]
  DRP --> DB6["dbLoops.listLoops(workspaceId)"]
  DB4 --> API4["GET /api/v1/workspaces/{ws}/settings"]
```

## 调用关系链路图

```mermaid
flowchart TD
  MessagesPage --> setConfigDrawerOpen
  setConfigDrawerOpen --> MessageConfigDrawer
  MessageConfigDrawer --> WorkspaceSlashCommandsPanel
  MessageConfigDrawer --> DefaultResponseConfigPanel
  WorkspaceSlashCommandsPanel --> slash_load["加载斜杠命令列表"]
  WorkspaceSlashCommandsPanel --> todo_load["加载 todo 列表供绑定"]
  WorkspaceSlashCommandsPanel --> loop_load["加载 loop 列表供绑定"]
  DefaultResponseConfigPanel --> settings_load["加载 workspace 设置"]
  DefaultResponseConfigPanel --> dr_todo["加载 todo 列表"]
  DefaultResponseConfigPanel --> dr_loop["加载 loop 列表"]
  MessageConfigDrawer --> onChanged["onChanged 回调 = handleRefresh"]
```

## 数据结构图

```mermaid
classDiagram
  class MessageConfigDrawerProps {
    +open: boolean
    +workspaceId: number
    +onClose: void_fn
    +onChanged: void_fn
  }
  class WorkspaceSlashCommand {
    +id: number
    +workspace_id: number
    +command: string
    +todo_id: number
    +loop_id: Option_number
    +created_at: Option_String
  }
  class WorkspaceSettings {
    +system_prompt: Option_String
    +default_response_enabled: Option_bool
    +default_response_todo_id: Option_i64
    +default_response_executor: Option_String
    +default_response_loop_id: Option_i64
  }
  MessageConfigDrawer --> WorkspaceSlashCommandsPanel
  MessageConfigDrawer --> DefaultResponseConfigPanel
```

## 数据变更图

```mermaid
stateDiagram-v2
  [*] --> Closed: configDrawerOpen = false
  Closed --> Open: 点击配置按钮 setConfigDrawerOpen(true)
  Open --> Loading: 子面板挂载拉取数据
  Loading --> Ready: 斜杠命令 + 默认响应数据就绪
  Ready --> Editing: 用户修改配置
  Editing --> Saving: 子面板内部保存
  Saving --> Changed: onChanged 触发 handleRefresh
  Changed --> Refreshing: 刷新消息列表 + 统计
  Refreshing --> Ready: 刷新完成
  Ready --> Closed: 点击抽屉 onClose
```

## 开发指导

- **前端入口**：`frontend/src/components/message-monitor/MessageConfigDrawer.tsx` 的 `MessageConfigDrawer` 组件，由 `MessagesPage` 的 `onOpenConfig` 控制
- **后端入口**：`backend/src/handlers/workspace_slash_command.rs`（斜杠命令 CRUD）和 `backend/src/handlers/workspace_setting.rs`（工作空间设置读写）
- **注意**：`MessageConfigDrawer` 用 `destroyOnClose` 保证每次打开时子面板重新挂载拉取最新数据；`onChanged` 回调绑定到 `handleRefresh`，保存配置后会自动刷新消息列表与统计，不要额外再绑一次刷新
- **扩展**：若要在配置抽屉中新增配置面板，在 `MessageConfigDrawer` 的 JSX 内追加新分区（Divider 分隔），传入 `workspaceId` 和 `onChanged` 即可；新增配置字段时同步更新 `WorkspaceSettings` 接口和后端 `workspace_settings` 表 migration
